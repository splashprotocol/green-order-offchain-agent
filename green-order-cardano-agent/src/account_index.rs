use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bloom_offchain::execution_engine::bundled::Bundled;
use bloom_offchain_cardano::orders::green::{
    AccountId, AlephAccountUtxo, AlephIntention, GreenAuth, GreenIntention, GreenOrder, GreenOrderId,
    GreenStorePlanningError, StoreSnapshotId,
};
use cml_chain::transaction::TransactionOutput;
use spectrum_cardano_lib::output::FinalizedTxOut;
use spectrum_cardano_lib::OutputRef;

use crate::account_store::{planned_completion, planned_delta, AccountStore, StoredIntentLeafSnapshot};

#[derive(Debug, Default)]
pub struct AccountIndex {
    by_account_id: HashMap<AccountId, IndexedAccount>,
    by_output_ref: HashMap<OutputRef, AccountId>,
    predicted_by_output_ref: HashMap<OutputRef, (AccountId, TransactionOutput)>,
    unbound_by_output_ref: HashMap<OutputRef, AlephAccountUtxo>,
    persisted_by_output_ref: HashMap<OutputRef, (AccountId, AccountStore)>,
    pending_by_account_id: HashSet<AccountId>,
    rollback_by_spent_ref: HashMap<OutputRef, (AccountId, IndexedAccount)>,
    pending_store_by_snapshot_id: HashMap<StoreSnapshotId, (AccountId, OutputRef, AccountStore)>,
    pending_snapshot_by_output_ref: HashMap<OutputRef, StoreSnapshotId>,
    persistence_path: Option<PathBuf>,
    next_snapshot_id: u64,
}

#[derive(Debug, Clone)]
pub struct IndexedAccount {
    pub utxo: FinalizedTxOut,
    pub store: AccountStore,
}

impl AccountIndex {
    pub fn with_persistence_path(path: PathBuf) -> Self {
        let mut index = Self {
            persistence_path: Some(path.clone()),
            ..Default::default()
        };
        match load_persisted_stores(&path) {
            Ok(stores) => {
                index.persisted_by_output_ref = stores
                    .into_iter()
                    .map(|store| (store.output_ref, (store.account_id, store.store)))
                    .collect();
            }
            Err(err) => log::warn!("Failed to load green account store snapshots: {}", err),
        }
        index
    }

    pub fn current(&self, account_id: AccountId) -> Option<FinalizedTxOut> {
        self.by_account_id
            .get(&account_id)
            .map(|account| account.utxo.clone())
    }

    pub fn current_store_root(&self, account_id: AccountId) -> Option<[u8; 32]> {
        self.by_account_id
            .get(&account_id)
            .map(|account| account.store.root())
    }

    pub fn pending_snapshot(&self, snapshot_id: StoreSnapshotId) -> Option<&AccountStore> {
        self.pending_store_by_snapshot_id
            .get(&snapshot_id)
            .map(|(_, _, store)| store)
    }

    pub fn pending_continuations(&self) -> Vec<Bundled<GreenOrder, FinalizedTxOut>> {
        self.by_account_id
            .iter()
            .flat_map(|(account_id, account)| {
                account
                    .store
                    .pending_leaves()
                    .filter_map(|(key, leaf)| {
                        let proof = account.store.path_proof(key).ok()?;
                        Some(Bundled(
                            GreenOrder {
                                id: leaf.canonical_order_id,
                                account_id: *account_id,
                                current_remainder: leaf.intent.leaving_amount,
                                accumulated_output: 0,
                                intention: GreenIntention {
                                    input_asset: leaf.intent.leaving_asset,
                                    output_asset: leaf.intent.arriving_asset,
                                    leaving_amount: leaf.intent.leaving_amount,
                                    expected_arriving_amount: leaf.intent.expected_arriving_amount,
                                    fee_lovelace: leaf.intent.fee_lovelace,
                                    target_nonce_slot: leaf.intent.target_nonce_index,
                                    target_nonce_value: leaf.intent.target_nonce_value,
                                    operator_key_hash: leaf.intent.operator,
                                },
                                auth: GreenAuth::Path { proof },
                            },
                            account.utxo.clone(),
                        ))
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn plan_sig_insert(
        &mut self,
        account_id: AccountId,
        old_ref: OutputRef,
        canonical_order_id: GreenOrderId,
        key: Vec<u8>,
        updated_intent: AlephIntention,
    ) -> Result<bloom_offchain_cardano::orders::green::PlannedStoreDelta, GreenStorePlanningError> {
        let account = self
            .by_account_id
            .get(&account_id)
            .ok_or(GreenStorePlanningError::MissingAccount)?;
        if account.utxo.reference() != old_ref {
            return Err(GreenStorePlanningError::MissingAccount);
        }
        let mut predicted = account.store.clone();
        let delta = predicted.insert_remaining(key, canonical_order_id, updated_intent)?;
        let snapshot_id = self.reserve_predicted_store_snapshot(account_id, old_ref, predicted);
        Ok(planned_delta(delta, snapshot_id))
    }

    pub fn plan_path_update(
        &mut self,
        account_id: AccountId,
        old_ref: OutputRef,
        key: Vec<u8>,
        old_digest: [u8; 32],
        updated_intent: AlephIntention,
    ) -> Result<bloom_offchain_cardano::orders::green::PlannedStoreDelta, GreenStorePlanningError> {
        let account = self
            .by_account_id
            .get(&account_id)
            .ok_or(GreenStorePlanningError::MissingAccount)?;
        if account.utxo.reference() != old_ref {
            return Err(GreenStorePlanningError::MissingAccount);
        }
        let mut predicted = account.store.clone();
        let delta = predicted.update_remaining(key, old_digest, updated_intent)?;
        let snapshot_id = self.reserve_predicted_store_snapshot(account_id, old_ref, predicted);
        Ok(planned_delta(delta, snapshot_id))
    }

    pub fn plan_path_completion(
        &mut self,
        account_id: AccountId,
        old_ref: OutputRef,
        key: Vec<u8>,
        old_digest: [u8; 32],
    ) -> Result<bloom_offchain_cardano::orders::green::PlannedStoreCompletion, GreenStorePlanningError> {
        let account = self
            .by_account_id
            .get(&account_id)
            .ok_or(GreenStorePlanningError::MissingAccount)?;
        if account.utxo.reference() != old_ref {
            return Err(GreenStorePlanningError::MissingAccount);
        }
        let mut predicted = account.store.clone();
        let completion = predicted.mark_completed(key, old_digest)?;
        let snapshot_id = self.reserve_predicted_store_snapshot(account_id, old_ref, predicted);
        Ok(planned_completion(completion, snapshot_id))
    }

    pub fn reserve_predicted_store_snapshot(
        &mut self,
        account_id: AccountId,
        old_ref: OutputRef,
        predicted_store: AccountStore,
    ) -> StoreSnapshotId {
        let snapshot_id = StoreSnapshotId(self.next_snapshot_id);
        self.next_snapshot_id += 1;
        self.pending_store_by_snapshot_id
            .insert(snapshot_id, (account_id, old_ref, predicted_store));
        snapshot_id
    }

    pub fn attach_predicted_output_ref(
        &mut self,
        snapshot_id: StoreSnapshotId,
        predicted_output_ref: OutputRef,
    ) -> Result<(), GreenStorePlanningError> {
        if !self.pending_store_by_snapshot_id.contains_key(&snapshot_id) {
            return Err(GreenStorePlanningError::MissingStore);
        }
        self.pending_snapshot_by_output_ref
            .insert(predicted_output_ref, snapshot_id);
        Ok(())
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
        if let Some((account_id, indexed)) = self.rollback_by_spent_ref.remove(&output_ref) {
            self.bind_indexed(account_id, indexed);
        }
        self.pending_store_by_snapshot_id
            .retain(|_, (_, old_ref, _)| *old_ref != output_ref);
        self.pending_snapshot_by_output_ref
            .retain(|_, snapshot_id| self.pending_store_by_snapshot_id.contains_key(snapshot_id));
    }

    pub fn observe_removed_output(&mut self, output_ref: OutputRef) {
        if let Some(account_id) = self.by_output_ref.remove(&output_ref) {
            self.by_account_id.remove(&account_id);
            self.pending_by_account_id.insert(account_id);
        }
        self.predicted_by_output_ref.remove(&output_ref);
        self.unbound_by_output_ref.remove(&output_ref);
    }

    pub fn observe_transaction(
        &mut self,
        consumed_refs: impl IntoIterator<Item = OutputRef>,
        produced_accounts: impl IntoIterator<Item = AlephAccountUtxo>,
    ) {
        let consumed_refs = consumed_refs.into_iter().collect::<HashSet<_>>();
        for output_ref in &consumed_refs {
            self.observe_consumed(*output_ref);
        }

        for account in produced_accounts {
            if self
                .bind_confirmed_store_for_consumed_refs(&consumed_refs, account.clone())
                .is_ok()
            {
                continue;
            }
            self.observe_created_or_updated(account);
        }
    }

    pub fn observe_created_or_updated(&mut self, account: AlephAccountUtxo) {
        if let Some((account_id, _)) = self.predicted_by_output_ref.remove(&account.output_ref) {
            if self
                .bind_confirmed_predicted_store(account.output_ref, account.clone())
                .is_err()
            {
                self.bind_observed(account_id, account);
            }
            return;
        }

        if self
            .pending_snapshot_by_output_ref
            .contains_key(&account.output_ref)
        {
            let _ = self.bind_confirmed_predicted_store(account.output_ref, account);
            return;
        }

        if let Some((account_id, store)) = self.persisted_by_output_ref.get(&account.output_ref).cloned() {
            if store.root() == account.state.store_root {
                self.persisted_by_output_ref.remove(&account.output_ref);
                self.bind_indexed(
                    account_id,
                    IndexedAccount {
                        utxo: FinalizedTxOut::new(account.output, account.output_ref),
                        store,
                    },
                );
                return;
            }
        }

        self.unbound_by_output_ref.insert(account.output_ref, account);
    }

    pub fn bind_external_account_id(
        &mut self,
        account_id: AccountId,
        output_ref: OutputRef,
    ) -> Option<FinalizedTxOut> {
        let account = self.unbound_by_output_ref.remove(&output_ref)?;
        self.bind_observed(account_id, account);
        self.current(account_id)
    }

    pub fn bind_confirmed_predicted_store(
        &mut self,
        predicted_output_ref: OutputRef,
        account: AlephAccountUtxo,
    ) -> Result<(), GreenStorePlanningError> {
        let snapshot_id = self
            .pending_snapshot_by_output_ref
            .get(&predicted_output_ref)
            .copied()
            .ok_or(GreenStorePlanningError::MissingStore)?;
        let (_, _, store) = self
            .pending_store_by_snapshot_id
            .get(&snapshot_id)
            .ok_or(GreenStorePlanningError::MissingStore)?;
        if account.state.store_root != store.root() {
            return Err(GreenStorePlanningError::RootMismatch {
                expected: store.root(),
                actual: account.state.store_root,
            });
        }
        let (account_id, _, store) = self
            .pending_store_by_snapshot_id
            .remove(&snapshot_id)
            .ok_or(GreenStorePlanningError::MissingStore)?;
        self.pending_snapshot_by_output_ref.remove(&predicted_output_ref);
        self.bind_indexed(
            account_id,
            IndexedAccount {
                utxo: FinalizedTxOut::new(account.output, account.output_ref),
                store,
            },
        );
        Ok(())
    }

    fn bind_confirmed_store_for_consumed_refs(
        &mut self,
        consumed_refs: &HashSet<OutputRef>,
        account: AlephAccountUtxo,
    ) -> Result<(), GreenStorePlanningError> {
        let matching = self
            .pending_store_by_snapshot_id
            .iter()
            .filter(|(_, (_, old_ref, store))| {
                consumed_refs.contains(old_ref) && store.root() == account.state.store_root
            })
            .map(|(snapshot_id, _)| *snapshot_id)
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err(GreenStorePlanningError::MissingStore);
        }
        let snapshot_id = matching[0];
        let (account_id, _, store) = self
            .pending_store_by_snapshot_id
            .remove(&snapshot_id)
            .ok_or(GreenStorePlanningError::MissingStore)?;
        self.pending_snapshot_by_output_ref
            .retain(|_, pending_snapshot_id| *pending_snapshot_id != snapshot_id);
        self.bind_indexed(
            account_id,
            IndexedAccount {
                utxo: FinalizedTxOut::new(account.output, account.output_ref),
                store,
            },
        );
        Ok(())
    }

    fn bind_observed(&mut self, account_id: AccountId, account: AlephAccountUtxo) {
        let Some(store) = AccountStore::from_observed_root(account.state.store_root) else {
            self.unbound_by_output_ref.insert(account.output_ref, account);
            return;
        };
        self.bind_indexed(
            account_id,
            IndexedAccount {
                utxo: FinalizedTxOut::new(account.output, account.output_ref),
                store,
            },
        );
    }

    fn bind_indexed(&mut self, account_id: AccountId, indexed: IndexedAccount) {
        let output_ref = indexed.utxo.reference();
        if let Some(previous) = self.by_account_id.remove(&account_id) {
            self.by_output_ref.remove(&previous.utxo.reference());
        }
        self.by_output_ref.insert(output_ref, account_id);
        self.by_account_id.insert(account_id, indexed);
        self.pending_by_account_id.remove(&account_id);
        self.unbound_by_output_ref.remove(&output_ref);
        self.persist_account_stores();
    }

    fn remove_bound_output(&mut self, output_ref: OutputRef) -> Option<AccountId> {
        self.by_output_ref.remove(&output_ref)
    }

    fn persist_account_stores(&self) {
        let Some(path) = &self.persistence_path else {
            return;
        };
        if let Err(err) = persist_account_stores(
            path,
            self.by_account_id
                .iter()
                .map(|(account_id, account)| PersistedAccountStore::from_indexed(*account_id, account))
                .chain(
                    self.persisted_by_output_ref
                        .iter()
                        .map(|(output_ref, (account_id, store))| {
                            PersistedAccountStore::from_parts(*account_id, *output_ref, store)
                        }),
                ),
        ) {
            log::warn!("Failed to persist green account store snapshots: {}", err);
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedAccountStore {
    account_id: AccountId,
    output_ref: OutputRef,
    root: [u8; 32],
    leaves: Vec<StoredIntentLeafSnapshot>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PersistedAccountStores {
    stores: Vec<PersistedAccountStore>,
}

impl PersistedAccountStore {
    fn from_indexed(account_id: AccountId, indexed: &IndexedAccount) -> Self {
        Self::from_parts(account_id, indexed.utxo.reference(), &indexed.store)
    }

    fn from_parts(account_id: AccountId, output_ref: OutputRef, store: &AccountStore) -> Self {
        Self {
            account_id,
            output_ref,
            root: store.root(),
            leaves: store.snapshot_leaves(),
        }
    }

    fn into_store(self) -> Result<LoadedAccountStore, GreenStorePlanningError> {
        Ok(LoadedAccountStore {
            account_id: self.account_id,
            output_ref: self.output_ref,
            store: AccountStore::from_snapshot(self.root, self.leaves)?,
        })
    }
}

struct LoadedAccountStore {
    account_id: AccountId,
    output_ref: OutputRef,
    store: AccountStore,
}

fn load_persisted_stores(path: &PathBuf) -> Result<Vec<LoadedAccountStore>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read(path).map_err(|err| err.to_string())?;
    let persisted: PersistedAccountStores = serde_json::from_slice(&raw).map_err(|err| err.to_string())?;
    persisted
        .stores
        .into_iter()
        .map(|store| store.into_store().map_err(|err| format!("{:?}", err)))
        .collect()
}

fn persist_account_stores(
    path: &PathBuf,
    stores: impl IntoIterator<Item = PersistedAccountStore>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let persisted = PersistedAccountStores {
        stores: stores.into_iter().collect(),
    };
    let raw = serde_json::to_vec_pretty(&persisted).map_err(|err| err.to_string())?;
    std::fs::write(path, raw).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use bloom_offchain_cardano::orders::green::{
        AccountId, AlephAccountState, AlephAccountUtxo, AlephIntention, GreenOrderId,
    };
    use cml_chain::address::Address;
    use cml_chain::assets::Coin;
    use cml_chain::transaction::TransactionOutput;
    use cml_chain::Value;
    use cml_crypto::TransactionHash;
    use spectrum_cardano_lib::output::FinalizedTxOut;
    use spectrum_cardano_lib::AssetClass;
    use spectrum_cardano_lib::OutputRef;

    use super::{AccountIndex, IndexedAccount};

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

    fn account_utxo_with_root(
        output_ref: OutputRef,
        output: TransactionOutput,
        store_root: [u8; 32],
    ) -> AlephAccountUtxo {
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
                store_root,
            },
        }
    }

    fn account_utxo(output_ref: OutputRef, output: TransactionOutput) -> AlephAccountUtxo {
        account_utxo_with_root(output_ref, output, [0; 32])
    }

    fn intent(leaving_amount: u64) -> AlephIntention {
        AlephIntention {
            target_nonce_index: 0,
            target_nonce_value: 42,
            leaving_asset: AssetClass::Native,
            leaving_amount,
            arriving_asset: AssetClass::Native,
            expected_arriving_amount: leaving_amount / 2,
            fee_lovelace: 10,
            operator: [7; 28],
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
    fn non_empty_unplanned_root_stays_unbound() {
        let id = account_id(11);
        let out_ref = output_ref(5);
        let mut index = AccountIndex::default();

        index.observe_created_or_updated(account_utxo_with_root(out_ref, dummy_output(2_000_000), [6; 32]));

        assert_eq!(index.bind_external_account_id(id, out_ref), None);
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

    #[test]
    fn mismatched_predicted_store_can_be_retried() {
        let id = account_id(12);
        let old_ref = output_ref(6);
        let predicted_ref = output_ref(7);
        let mut index = AccountIndex::default();
        let snapshot_id =
            index.reserve_predicted_store_snapshot(id, old_ref, crate::account_store::AccountStore::empty());
        index
            .attach_predicted_output_ref(snapshot_id, predicted_ref)
            .unwrap();

        let mismatched = account_utxo_with_root(predicted_ref, dummy_output(2_000_000), [9; 32]);
        assert!(index
            .bind_confirmed_predicted_store(predicted_ref, mismatched)
            .is_err());
        assert!(index.pending_snapshot(snapshot_id).is_some());

        let matched = account_utxo(predicted_ref, dummy_output(2_000_000));
        index
            .bind_confirmed_predicted_store(predicted_ref, matched)
            .unwrap();
        assert!(index.pending_snapshot(snapshot_id).is_none());
        assert!(index.current(id).is_some());
    }

    #[test]
    fn transaction_binds_unique_pending_snapshot_by_consumed_ref_and_root() {
        let id = account_id(13);
        let old_ref = output_ref(8);
        let new_ref = output_ref(9);
        let mut index = AccountIndex::default();
        let snapshot_id =
            index.reserve_predicted_store_snapshot(id, old_ref, crate::account_store::AccountStore::empty());

        index.observe_transaction([old_ref], [account_utxo(new_ref, dummy_output(2_000_000))]);

        assert!(index.pending_snapshot(snapshot_id).is_none());
        assert_eq!(
            index.current(id).map(|account| account.reference()),
            Some(new_ref)
        );
    }

    #[test]
    fn transaction_does_not_bind_ambiguous_pending_snapshot() {
        let id_1 = account_id(14);
        let id_2 = account_id(15);
        let old_ref_1 = output_ref(10);
        let old_ref_2 = output_ref(11);
        let new_ref = output_ref(12);
        let mut index = AccountIndex::default();
        let snapshot_id_1 = index.reserve_predicted_store_snapshot(
            id_1,
            old_ref_1,
            crate::account_store::AccountStore::empty(),
        );
        let snapshot_id_2 = index.reserve_predicted_store_snapshot(
            id_2,
            old_ref_2,
            crate::account_store::AccountStore::empty(),
        );

        index.observe_transaction(
            [old_ref_1, old_ref_2],
            [account_utxo(new_ref, dummy_output(2_000_000))],
        );

        assert!(index.pending_snapshot(snapshot_id_1).is_some());
        assert!(index.pending_snapshot(snapshot_id_2).is_some());
        assert_eq!(index.current(id_1), None);
        assert_eq!(index.current(id_2), None);
    }

    #[test]
    fn persisted_snapshot_rebinds_only_after_matching_output_is_observed() {
        let id = account_id(16);
        let output_ref = output_ref(13);
        let output = dummy_output(2_000_000);
        let path = std::env::temp_dir().join(format!(
            "green-account-store-test-{}-{}.json",
            std::process::id(),
            output_ref.index()
        ));
        let _ = std::fs::remove_file(&path);

        {
            let mut index = AccountIndex::with_persistence_path(path.clone());
            index.bind_indexed(
                id,
                IndexedAccount {
                    utxo: FinalizedTxOut::new(output.clone(), output_ref),
                    store: crate::account_store::AccountStore::empty(),
                },
            );
        }

        let mut reloaded = AccountIndex::with_persistence_path(path.clone());
        assert_eq!(reloaded.current(id), None);
        reloaded.observe_created_or_updated(account_utxo(output_ref, output.clone()));

        assert_eq!(
            reloaded.current(id).map(|account| account.reference()),
            Some(output_ref)
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scanner_continuation_uses_canonical_order_id_from_execution_path() {
        let id = account_id(17);
        let old_ref = output_ref(14);
        let new_ref = output_ref(15);
        let old_output = dummy_output(2_000_000);
        let new_output = dummy_output(2_100_000);
        let canonical_order_id = GreenOrderId::new(id, 0, 42, [99; 32]);
        let updated_intent = intent(500);
        let mut index = AccountIndex::default();
        index.bind_indexed(
            id,
            IndexedAccount {
                utxo: FinalizedTxOut::new(old_output, old_ref),
                store: crate::account_store::AccountStore::empty(),
            },
        );

        let planned = index
            .plan_sig_insert(
                id,
                old_ref,
                canonical_order_id,
                updated_intent.intent_key(),
                updated_intent,
            )
            .unwrap();
        index.observe_transaction(
            [old_ref],
            [account_utxo_with_root(new_ref, new_output, planned.new_root)],
        );

        let continuations = index.pending_continuations();

        assert_eq!(continuations.len(), 1);
        assert_eq!(continuations[0].0.id, canonical_order_id);
    }

    #[test]
    fn rebinding_one_persisted_snapshot_keeps_other_unobserved_snapshots_durable() {
        let id_1 = account_id(18);
        let id_2 = account_id(19);
        let ref_1 = output_ref(16);
        let ref_2 = output_ref(17);
        let out_1 = dummy_output(2_000_000);
        let out_2 = dummy_output(2_100_000);
        let path = std::env::temp_dir().join(format!(
            "green-account-store-test-{}-multi.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        {
            let mut index = AccountIndex::with_persistence_path(path.clone());
            index.bind_indexed(
                id_1,
                IndexedAccount {
                    utxo: FinalizedTxOut::new(out_1.clone(), ref_1),
                    store: crate::account_store::AccountStore::empty(),
                },
            );
            index.bind_indexed(
                id_2,
                IndexedAccount {
                    utxo: FinalizedTxOut::new(out_2.clone(), ref_2),
                    store: crate::account_store::AccountStore::empty(),
                },
            );
        }

        {
            let mut reloaded = AccountIndex::with_persistence_path(path.clone());
            reloaded.observe_created_or_updated(account_utxo(ref_1, out_1.clone()));
        }

        let mut reloaded_again = AccountIndex::with_persistence_path(path.clone());
        reloaded_again.observe_created_or_updated(account_utxo(ref_2, out_2.clone()));

        assert_eq!(
            reloaded_again.current(id_2).map(|account| account.reference()),
            Some(ref_2)
        );
        let _ = std::fs::remove_file(path);
    }
}
