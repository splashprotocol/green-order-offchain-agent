use std::collections::{HashMap, HashSet};

use bloom_offchain_cardano::orders::green::{AccountId, AlephAccountUtxo};
use cml_chain::transaction::TransactionOutput;
use spectrum_cardano_lib::output::FinalizedTxOut;
use spectrum_cardano_lib::OutputRef;

#[derive(Debug, Default)]
pub struct AccountIndex {
    by_account_id: HashMap<AccountId, FinalizedTxOut>,
    by_output_ref: HashMap<OutputRef, AccountId>,
    predicted_by_output_ref: HashMap<OutputRef, (AccountId, TransactionOutput)>,
    unbound_by_output_ref: HashMap<OutputRef, AlephAccountUtxo>,
    pending_by_account_id: HashSet<AccountId>,
    rollback_by_spent_ref: HashMap<OutputRef, (AccountId, FinalizedTxOut)>,
}

impl AccountIndex {
    pub fn current(&self, account_id: AccountId) -> Option<FinalizedTxOut> {
        self.by_account_id.get(&account_id).cloned()
    }

    pub fn mark_pending_update(
        &mut self,
        account_id: AccountId,
        old_ref: OutputRef,
        new_ref: OutputRef,
        new_output: TransactionOutput,
    ) {
        self.remove_bound_output(old_ref);
        self.by_account_id.remove(&account_id);
        self.pending_by_account_id.insert(account_id);
        self.predicted_by_output_ref
            .insert(new_ref, (account_id, new_output));
    }

    pub fn observe_consumed(&mut self, output_ref: OutputRef) {
        if let Some(account_id) = self.remove_bound_output(output_ref) {
            if let Some(output) = self.by_account_id.remove(&account_id) {
                self.rollback_by_spent_ref
                    .insert(output_ref, (account_id, output));
            }
            self.pending_by_account_id.insert(account_id);
        }
        self.predicted_by_output_ref.remove(&output_ref);
        self.unbound_by_output_ref.remove(&output_ref);
    }

    pub fn observe_rollback_consumed(&mut self, output_ref: OutputRef) {
        if let Some((account_id, output)) = self.rollback_by_spent_ref.remove(&output_ref) {
            self.bind(account_id, output.reference(), output.0);
        }
    }

    pub fn observe_removed_output(&mut self, output_ref: OutputRef) {
        if let Some(account_id) = self.by_output_ref.remove(&output_ref) {
            self.by_account_id.remove(&account_id);
            self.pending_by_account_id.insert(account_id);
        }
        self.predicted_by_output_ref.remove(&output_ref);
        self.unbound_by_output_ref.remove(&output_ref);
    }

    pub fn observe_created_or_updated(&mut self, account: AlephAccountUtxo) {
        if let Some((account_id, _)) = self.predicted_by_output_ref.remove(&account.output_ref) {
            self.bind(account_id, account.output_ref, account.output);
            return;
        }

        if self.pending_by_account_id.len() == 1 {
            let account_id = *self.pending_by_account_id.iter().next().unwrap();
            self.bind(account_id, account.output_ref, account.output);
            return;
        }

        self.unbound_by_output_ref.insert(account.output_ref, account);
    }

    pub fn bind_external_account_id(
        &mut self,
        account_id: AccountId,
        output_ref: OutputRef,
    ) -> Option<FinalizedTxOut> {
        let account = self.unbound_by_output_ref.remove(&output_ref)?;
        self.bind(account_id, account.output_ref, account.output);
        self.current(account_id)
    }

    fn bind(&mut self, account_id: AccountId, output_ref: OutputRef, output: TransactionOutput) {
        if let Some(previous) = self.by_account_id.remove(&account_id) {
            self.by_output_ref.remove(&previous.reference());
        }
        self.by_output_ref.insert(output_ref, account_id);
        self.by_account_id
            .insert(account_id, FinalizedTxOut::new(output, output_ref));
        self.pending_by_account_id.remove(&account_id);
        self.unbound_by_output_ref.remove(&output_ref);
    }

    fn remove_bound_output(&mut self, output_ref: OutputRef) -> Option<AccountId> {
        self.by_output_ref.remove(&output_ref)
    }
}

#[cfg(test)]
mod tests {
    use bloom_offchain_cardano::orders::green::{AccountId, AlephAccountState, AlephAccountUtxo};
    use cml_chain::address::Address;
    use cml_chain::assets::Coin;
    use cml_chain::transaction::TransactionOutput;
    use cml_chain::Value;
    use cml_crypto::TransactionHash;
    use spectrum_cardano_lib::OutputRef;

    use super::AccountIndex;

    fn account_id(byte: u8) -> AccountId {
        AccountId::try_from_slice(&[byte; 32]).unwrap()
    }

    fn output_ref(index: u64) -> OutputRef {
        OutputRef::new(TransactionHash::from([index as u8; 32]), index)
    }

    fn dummy_output(lovelace: u64) -> TransactionOutput {
        TransactionOutput::new(
            Address::from_bech32(
                "addr1z8d70g7c58vznyye9guwagdza74x36f3uff0eyk2zwpcpx6c96rgsm7p0hmwrj8e28qny5yxwya63e8gjj8s2ugfglhsxedx9j",
            )
            .unwrap(),
            Value::from(Coin::from(lovelace)),
            None,
            None,
        )
    }

    fn account_utxo(output_ref: OutputRef, output: TransactionOutput) -> AlephAccountUtxo {
        AlephAccountUtxo {
            output_ref,
            output,
            state: AlephAccountState {
                magic: vec![1, 2, 3],
                allowlist: vec![],
                nonce: vec![0],
                main_key: vec![4; 32],
                co_key: None,
                cold_key_hash: [5; 28],
                store_root: [6; 32],
            },
        }
    }

    #[test]
    fn consumed_bound_account_is_unavailable_until_continuation_arrives() {
        let id = account_id(7);
        let old_ref = output_ref(0);
        let new_ref = output_ref(1);
        let new_output = dummy_output(2_100_000);
        let mut index = AccountIndex::default();

        index.mark_pending_update(id, old_ref, new_ref, new_output.clone());

        assert_eq!(index.current(id), None);

        index.observe_consumed(old_ref);
        assert_eq!(index.current(id), None);

        index.observe_created_or_updated(account_utxo(new_ref, new_output.clone()));

        let finalized = index.current(id).expect("continuation should re-bind account");
        assert_eq!(finalized.reference(), new_ref);
        assert_eq!(finalized.0, new_output);
    }

    #[test]
    fn unbound_account_outputs_do_not_become_executable() {
        let id = account_id(9);
        let out_ref = output_ref(3);
        let mut index = AccountIndex::default();

        index.observe_created_or_updated(account_utxo(out_ref, dummy_output(2_000_000)));

        assert_eq!(index.current(id), None);
    }

    #[test]
    fn external_binding_makes_unbound_output_current() {
        let id = account_id(10);
        let out_ref = output_ref(4);
        let output = dummy_output(2_000_000);
        let mut index = AccountIndex::default();

        index.observe_created_or_updated(account_utxo(out_ref, output.clone()));

        let finalized = index
            .bind_external_account_id(id, out_ref)
            .expect("external registry should bind observed account");

        assert_eq!(finalized.reference(), out_ref);
        assert_eq!(finalized.0, output);
        assert_eq!(index.current(id), Some(finalized));
    }
}
