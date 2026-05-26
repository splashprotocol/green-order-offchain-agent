use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bloom_offchain_cardano::event_sink::tx_view::TxViewMut;
use bloom_offchain_cardano::orders::green::{AlephAccountUtxo, ALEPH_ACCOUNT_VALIDATOR};
use cardano_chain_sync::data::LedgerTxEvent;
use cardano_mempool_sync::data::MempoolUpdate;
use log::info;
use spectrum_cardano_lib::transaction::TransactionOutputExtension;
use spectrum_cardano_lib::OutputRef;
use spectrum_offchain::domain::Has;
use spectrum_offchain::event_sink::event_handler::EventHandler;
use spectrum_offchain_cardano::deployment::DeployedScriptInfo;

use crate::account_index::AccountIndex;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AccountEvent {
    Consumed(OutputRef),
    Produced(AlephAccountUtxo),
}

#[derive(Clone)]
pub struct AccountEventHandler<C> {
    index: Arc<Mutex<AccountIndex>>,
    ctx: C,
}

impl<C> AccountEventHandler<C> {
    pub fn new(index: Arc<Mutex<AccountIndex>>, ctx: C) -> Self {
        Self { index, ctx }
    }
}

pub fn scan_account_events<C>(tx: &TxViewMut, ctx: &C) -> Vec<AccountEvent>
where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>>,
{
    let mut events = Vec::with_capacity(tx.inputs.len() + tx.outputs.len());
    for input in &tx.inputs {
        events.push(AccountEvent::Consumed(OutputRef::from(input.clone())));
    }
    for (index, output) in &tx.outputs {
        let output_ref = OutputRef::new(tx.hash, *index as u64);
        if let Some(account) = AlephAccountUtxo::try_parse(output_ref, output.clone(), ctx) {
            info!(
                "Observed Aleph account output {} for main key {}",
                output_ref,
                hex::encode(&account.state.main_key)
            );
            events.push(AccountEvent::Produced(account));
        } else if output.script_hash()
            == Some(
                ctx.select::<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>>()
                    .script_hash,
            )
        {
            info!(
                "Ignored Aleph account script output {} because account datum could not be parsed",
                output_ref
            );
        }
    }
    events
}

pub fn apply_account_events(index: &mut AccountIndex, events: impl IntoIterator<Item = AccountEvent>) {
    let mut consumed_refs = Vec::new();
    let mut produced_accounts = Vec::new();
    for event in events {
        match event {
            AccountEvent::Consumed(output_ref) => consumed_refs.push(output_ref),
            AccountEvent::Produced(account) => produced_accounts.push(account),
        }
    }
    index.observe_transaction(consumed_refs, produced_accounts);
}

pub fn revert_account_events(index: &mut AccountIndex, events: impl IntoIterator<Item = AccountEvent>) {
    for event in events {
        match event {
            AccountEvent::Produced(account) => index.observe_removed_output(account.output_ref),
            AccountEvent::Consumed(output_ref) => index.observe_rollback_consumed(output_ref),
        }
    }
}

#[async_trait]
impl<C> EventHandler<LedgerTxEvent<TxViewMut>> for AccountEventHandler<C>
where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> + Clone + Send + Sync + 'static,
{
    async fn try_handle(&mut self, ev: LedgerTxEvent<TxViewMut>) -> Option<LedgerTxEvent<TxViewMut>> {
        match &ev {
            LedgerTxEvent::TxApplied { tx, .. } => {
                let events = scan_account_events(tx, &self.ctx);
                let mut index = self.index.lock().expect("account index lock poisoned");
                apply_account_events(&mut index, events);
            }
            LedgerTxEvent::TxUnapplied { tx, .. } => {
                let events = scan_account_events(tx, &self.ctx);
                let mut index = self.index.lock().expect("account index lock poisoned");
                revert_account_events(&mut index, events);
            }
        }
        Some(ev)
    }
}

#[async_trait]
impl<C> EventHandler<MempoolUpdate<TxViewMut>> for AccountEventHandler<C>
where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> + Clone + Send + Sync + 'static,
{
    async fn try_handle(&mut self, ev: MempoolUpdate<TxViewMut>) -> Option<MempoolUpdate<TxViewMut>> {
        // Account stores are durable state. Mempool accept/drop events are
        // transient and can remove the planned successor store before the same
        // transaction is observed from ledger, so account indexing is driven by
        // confirmed chain events only.
        Some(ev)
    }
}

#[cfg(test)]
mod tests {
    use bloom_offchain_cardano::event_sink::tx_view::TxViewMut;
    use bloom_offchain_cardano::orders::green::{AlephAccountAbi, AlephAccountState};
    use cml_chain::address::{Address, EnterpriseAddress};
    use cml_chain::assets::AssetBundle;
    use cml_chain::certs::Credential;
    use cml_chain::transaction::{ConwayFormatTxOut, DatumOption, TransactionInput, TransactionOutput};
    use cml_chain::Value;
    use cml_crypto::{ScriptHash, TransactionHash};
    use spectrum_cardano_lib::ex_units::ExUnits;
    use spectrum_cardano_lib::plutus_data::IntoPlutusData;
    use spectrum_cardano_lib::OutputRef;
    use spectrum_offchain::domain::Has;
    use spectrum_offchain_cardano::deployment::DeployedScriptInfo;
    use type_equalities::IsEqual;

    use super::{scan_account_events, AccountEvent, ALEPH_ACCOUNT_VALIDATOR};

    #[derive(Copy, Clone)]
    struct AccountCtx {
        account: DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>,
    }

    impl Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> for AccountCtx {
        fn select<U: IsEqual<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>>>(
            &self,
        ) -> DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }> {
            self.account
        }
    }

    fn account_state() -> AlephAccountState {
        AlephAccountState {
            abi: AlephAccountAbi::Current,
            magic: b"green-test".to_vec(),
            allowlist: vec![],
            nonce: vec![0],
            main_key: vec![2; 33],
            co_key: None,
            cold_key_hash: [13; 28],
            store_root: [14; 32],
        }
    }

    #[test]
    fn scanner_detects_consumed_refs_and_aleph_account_outputs() {
        let script_hash = ScriptHash::from([9; 28]);
        let ctx = AccountCtx {
            account: DeployedScriptInfo {
                script_hash,
                marginal_cost: ExUnits { mem: 0, steps: 0 },
            },
        };
        let tx_hash = TransactionHash::from([1; 32]);
        let input = TransactionInput::new(TransactionHash::from([2; 32]), 4);
        let account_output = TransactionOutput::new_conway_format_tx_out(ConwayFormatTxOut {
            address: Address::Enterprise(EnterpriseAddress::new(0, Credential::new_script(script_hash))),
            amount: Value::new(2_000_000, AssetBundle::new()),
            datum_option: Some(DatumOption::Datum {
                datum: account_state().into_pd(),
                len_encoding: Default::default(),
                tag_encoding: None,
                datum_tag_encoding: None,
                datum_bytes_encoding: Default::default(),
            }),
            script_reference: None,
            encodings: None,
        });
        let tx = TxViewMut {
            hash: tx_hash,
            inputs: vec![input.clone()],
            outputs: vec![(3, account_output)],
            metadata: None,
            mints: None,
            signers: vec![],
        };

        let events = scan_account_events(&tx, &ctx);

        assert!(matches!(
            events.as_slice(),
            [AccountEvent::Consumed(ref consumed), AccountEvent::Produced(ref produced)]
                if *consumed == OutputRef::from(input) && produced.output_ref == OutputRef::new(tx_hash, 3)
        ));
    }
}
