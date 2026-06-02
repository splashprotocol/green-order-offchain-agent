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

#[derive(Debug, Copy, Clone, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub current: bool,
    pub persisted: bool,
    pub pending: bool,
    pub unbound: bool,
    pub predicted: bool,
}

#[derive(Debug, Clone)]
pub struct AccountSummary {
    pub account_id: AccountId,
    pub current_output_ref: Option<OutputRef>,
    pub current_store_root: Option<[u8; 32]>,
    pub pending: bool,
    pub persisted_outputs: usize,
}

#[derive(Debug, Copy, Clone)]
pub struct AccountIndexSummary {
    pub current_accounts: usize,
    pub pending_accounts: usize,
    pub unbound_outputs: usize,
    pub predicted_outputs: usize,
    pub persisted_outputs: usize,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum AccountBindingError {
    AlreadyBound,
    OutputAlreadyBound,
    OutputNotObserved,
    NonEmptyUnplannedRoot,
}

impl AccountIndex {
    pub fn with_persistence_path(path: PathBuf) -> Self {
        let mut index = Self {
            persistence_path: Some(path.clone()),
            ..Default::default()
        };
        match load_persisted_stores(&path) {
            Ok(state) => {
                index.persisted_by_output_ref = state
                    .stores
                    .into_iter()
                    .map(|store| (store.output_ref, (store.account_id, store.store)))
                    .collect();
                index.pending_store_by_snapshot_id = state
                    .pending
                    .into_iter()
                    .map(|(snapshot_id, account_id, old_ref, store)| {
                        (snapshot_id, (account_id, old_ref, store))
                    })
                    .collect();
                index.next_snapshot_id = state.next_snapshot_id.max(
                    index
                        .pending_store_by_snapshot_id
                        .keys()
                        .map(|snapshot_id| snapshot_id.0.saturating_add(1))
                        .max()
                        .unwrap_or(0),
                );
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

    pub fn current_or_pending_base(&self, account_id: AccountId) -> Option<FinalizedTxOut> {
        self.current(account_id).or_else(|| {
            if !self.pending_by_account_id.contains(&account_id) {
                return None;
            }
            self.pending_store_by_snapshot_id
                .values()
                .filter(|(pending_account_id, _, _)| *pending_account_id == account_id)
                .filter_map(|(_, old_ref, _)| self.rollback_by_spent_ref.get(old_ref))
                .map(|(_, account)| account.utxo.clone())
                .next()
        })
    }

    pub fn current_store_root(&self, account_id: AccountId) -> Option<[u8; 32]> {
        self.by_account_id
            .get(&account_id)
            .map(|account| account.store.root())
    }

    pub fn account_summary(&self, account_id: AccountId) -> Option<AccountSummary> {
        let current = self.by_account_id.get(&account_id);
        let pending = self.pending_by_account_id.contains(&account_id);
        let persisted_outputs = self
            .persisted_by_output_ref
            .values()
            .filter(|(persisted_id, _)| *persisted_id == account_id)
            .count();
        if current.is_none() && !pending && persisted_outputs == 0 {
            return None;
        }
        Some(AccountSummary {
            account_id,
            current_output_ref: current.map(|account| account.utxo.reference()),
            current_store_root: current.map(|account| account.store.root()),
            pending,
            persisted_outputs,
        })
    }

    pub fn summary(&self) -> AccountIndexSummary {
        AccountIndexSummary {
            current_accounts: self.by_account_id.len(),
            pending_accounts: self.pending_by_account_id.len(),
            unbound_outputs: self.unbound_by_output_ref.len(),
            predicted_outputs: self.predicted_by_output_ref.len(),
            persisted_outputs: self.persisted_by_output_ref.len(),
        }
    }

    pub fn account_status(&self, account_id: AccountId, output_ref: OutputRef) -> AccountStatus {
        AccountStatus {
            current: self
                .by_account_id
                .get(&account_id)
                .is_some_and(|account| account.utxo.reference() == output_ref),
            persisted: self
                .persisted_by_output_ref
                .get(&output_ref)
                .is_some_and(|(persisted_id, _)| *persisted_id == account_id),
            pending: self.pending_by_account_id.contains(&account_id),
            unbound: self.unbound_by_output_ref.contains_key(&output_ref),
            predicted: self
                .predicted_by_output_ref
                .get(&output_ref)
                .is_some_and(|(predicted_id, _)| *predicted_id == account_id),
        }
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
        let base_store = self
            .store_for_planning(account_id, old_ref)
            .ok_or(GreenStorePlanningError::MissingAccount)?;
        let mut predicted = base_store.clone();
        let delta = predicted.insert_remaining(key, canonical_order_id, updated_intent)?;
        self.remove_pending_snapshots_for(account_id, old_ref);
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
        let base_store = self
            .store_for_planning(account_id, old_ref)
            .ok_or(GreenStorePlanningError::MissingAccount)?;
        let mut predicted = base_store.clone();
        let delta = predicted.update_remaining(key, old_digest, updated_intent)?;
        self.remove_pending_snapshots_for(account_id, old_ref);
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
        let base_store = self
            .store_for_planning(account_id, old_ref)
            .ok_or(GreenStorePlanningError::MissingAccount)?;
        let mut predicted = base_store.clone();
        let completion = predicted.mark_completed(key, old_digest)?;
        self.remove_pending_snapshots_for(account_id, old_ref);
        let snapshot_id = self.reserve_predicted_store_snapshot(account_id, old_ref, predicted);
        Ok(planned_completion(completion, snapshot_id))
    }

    fn store_for_planning(&self, account_id: AccountId, old_ref: OutputRef) -> Option<&AccountStore> {
        if let Some(account) = self.by_account_id.get(&account_id) {
            if account.utxo.reference() != old_ref {
                return None;
            }
            return Some(&account.store);
        }
        if !self.pending_by_account_id.contains(&account_id)
            || !self.rollback_by_spent_ref.contains_key(&old_ref)
        {
            return None;
        }
        let has_matching_pending =
            self.pending_store_by_snapshot_id
                .values()
                .any(|(pending_account_id, pending_old_ref, _)| {
                    *pending_account_id == account_id && *pending_old_ref == old_ref
                });
        if !has_matching_pending {
            return None;
        }
        self.rollback_by_spent_ref
            .get(&old_ref)
            .map(|(_, account)| &account.store)
    }

    fn remove_pending_snapshots_for(&mut self, account_id: AccountId, old_ref: OutputRef) {
        let removed = self
            .pending_store_by_snapshot_id
            .iter()
            .filter(|(_, (pending_account_id, pending_old_ref, _))| {
                *pending_account_id == account_id && *pending_old_ref == old_ref
            })
            .map(|(snapshot_id, _)| *snapshot_id)
            .collect::<HashSet<_>>();
        if removed.is_empty() {
            return;
        }
        self.pending_store_by_snapshot_id
            .retain(|snapshot_id, _| !removed.contains(snapshot_id));
        self.pending_snapshot_by_output_ref
            .retain(|_, snapshot_id| !removed.contains(snapshot_id));
    }

    pub fn reserve_predicted_store_snapshot(
        &mut self,
        account_id: AccountId,
        old_ref: OutputRef,
        predicted_store: AccountStore,
    ) -> StoreSnapshotId {
        let snapshot_id = StoreSnapshotId(self.next_snapshot_id);
        self.next_snapshot_id += 1;
        self.remove_bound_output(old_ref);
        if let Some(output) = self.by_account_id.remove(&account_id) {
            self.rollback_by_spent_ref.insert(old_ref, (account_id, output));
        }
        self.pending_by_account_id.insert(account_id);
        self.pending_store_by_snapshot_id
            .insert(snapshot_id, (account_id, old_ref, predicted_store));
        self.persist_account_stores();
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
        self.persist_account_stores();
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
        self.persist_account_stores();
    }

    pub fn observe_rollback_consumed(&mut self, output_ref: OutputRef) {
        if let Some((account_id, indexed)) = self.rollback_by_spent_ref.remove(&output_ref) {
            self.bind_indexed(account_id, indexed);
        }
        self.pending_store_by_snapshot_id
            .retain(|_, (_, old_ref, _)| *old_ref != output_ref);
        self.pending_snapshot_by_output_ref
            .retain(|_, snapshot_id| self.pending_store_by_snapshot_id.contains_key(snapshot_id));
        self.persist_account_stores();
    }

    pub fn observe_removed_output(&mut self, output_ref: OutputRef) {
        if let Some(account_id) = self.by_output_ref.remove(&output_ref) {
            self.by_account_id.remove(&account_id);
            self.pending_by_account_id.insert(account_id);
        }
        self.predicted_by_output_ref.remove(&output_ref);
        self.unbound_by_output_ref.remove(&output_ref);
        self.persist_account_stores();
    }

    pub fn observe_transaction(
        &mut self,
        consumed_refs: impl IntoIterator<Item = OutputRef>,
        produced_accounts: impl IntoIterator<Item = AlephAccountUtxo>,
    ) {
        let consumed_refs = consumed_refs.into_iter().collect::<HashSet<_>>();
        let consumed_account_ids = consumed_refs
            .iter()
            .filter_map(|output_ref| self.by_output_ref.get(output_ref).copied())
            .collect::<HashSet<_>>();
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
            if self.bind_empty_successor_for_consumed_account(&consumed_account_ids, account.clone()) {
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

    pub fn bind_external_account_id_once(
        &mut self,
        account_id: AccountId,
        output_ref: OutputRef,
    ) -> Result<FinalizedTxOut, AccountBindingError> {
        if self.by_account_id.contains_key(&account_id)
            || self.pending_by_account_id.contains(&account_id)
            || self
                .persisted_by_output_ref
                .values()
                .any(|(persisted_id, _)| *persisted_id == account_id)
        {
            return Err(AccountBindingError::AlreadyBound);
        }
        if self.by_output_ref.contains_key(&output_ref)
            || self.persisted_by_output_ref.contains_key(&output_ref)
        {
            return Err(AccountBindingError::OutputAlreadyBound);
        }

        let account = self
            .unbound_by_output_ref
            .remove(&output_ref)
            .ok_or(AccountBindingError::OutputNotObserved)?;
        let Some(store) = AccountStore::from_observed_root(account.state.store_root) else {
            self.unbound_by_output_ref.insert(output_ref, account);
            return Err(AccountBindingError::NonEmptyUnplannedRoot);
        };
        self.bind_indexed(
            account_id,
            IndexedAccount {
                utxo: FinalizedTxOut::new(account.output, account.output_ref),
                store,
            },
        );
        self.current(account_id)
            .ok_or(AccountBindingError::OutputNotObserved)
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
        self.persist_account_stores();
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
            .map(|(snapshot_id, (account_id, old_ref, store))| {
                (*snapshot_id, *account_id, *old_ref, store.clone())
            })
            .collect::<Vec<_>>();
        let Some((_, account_id, _, store)) = matching.first().cloned() else {
            return Err(GreenStorePlanningError::MissingStore);
        };
        if matching
            .iter()
            .any(|(_, matching_account_id, _, matching_store)| {
                *matching_account_id != account_id || *matching_store != store
            })
        {
            return Err(GreenStorePlanningError::MissingStore);
        }
        let matching_snapshot_ids = matching
            .iter()
            .map(|(matching_snapshot_id, _, _, _)| *matching_snapshot_id)
            .collect::<HashSet<_>>();
        self.pending_store_by_snapshot_id
            .retain(|pending_snapshot_id, _| !matching_snapshot_ids.contains(pending_snapshot_id));
        self.pending_snapshot_by_output_ref
            .retain(|_, pending_snapshot_id| !matching_snapshot_ids.contains(pending_snapshot_id));
        self.bind_indexed(
            account_id,
            IndexedAccount {
                utxo: FinalizedTxOut::new(account.output, account.output_ref),
                store,
            },
        );
        self.persist_account_stores();
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

    fn bind_empty_successor_for_consumed_account(
        &mut self,
        consumed_account_ids: &HashSet<AccountId>,
        account: AlephAccountUtxo,
    ) -> bool {
        if consumed_account_ids.len() != 1 {
            return false;
        }
        let Some(store) = AccountStore::from_observed_root(account.state.store_root) else {
            return false;
        };
        let account_id = *consumed_account_ids.iter().next().expect("len checked");
        if !self.pending_by_account_id.contains(&account_id) {
            return false;
        }
        self.bind_indexed(
            account_id,
            IndexedAccount {
                utxo: FinalizedTxOut::new(account.output, account.output_ref),
                store,
            },
        );
        true
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
            self.pending_store_by_snapshot_id
                .iter()
                .map(|(snapshot_id, (account_id, old_ref, store))| {
                    PersistedPendingStore::from_pending(*snapshot_id, *account_id, *old_ref, store)
                }),
            self.next_snapshot_id,
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
    #[serde(default)]
    pending: Vec<PersistedPendingStore>,
    #[serde(default)]
    next_snapshot_id: u64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PersistedPendingStore {
    snapshot_id: StoreSnapshotId,
    account_id: AccountId,
    old_ref: OutputRef,
    root: [u8; 32],
    leaves: Vec<StoredIntentLeafSnapshot>,
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

impl PersistedPendingStore {
    fn from_pending(
        snapshot_id: StoreSnapshotId,
        account_id: AccountId,
        old_ref: OutputRef,
        store: &AccountStore,
    ) -> Self {
        Self {
            snapshot_id,
            account_id,
            old_ref,
            root: store.root(),
            leaves: store.snapshot_leaves(),
        }
    }

    fn into_pending(
        self,
    ) -> Result<(StoreSnapshotId, AccountId, OutputRef, AccountStore), GreenStorePlanningError> {
        Ok((
            self.snapshot_id,
            self.account_id,
            self.old_ref,
            AccountStore::from_snapshot(self.root, self.leaves)?,
        ))
    }
}

struct LoadedAccountStore {
    account_id: AccountId,
    output_ref: OutputRef,
    store: AccountStore,
}

struct LoadedPersistedState {
    stores: Vec<LoadedAccountStore>,
    pending: Vec<(StoreSnapshotId, AccountId, OutputRef, AccountStore)>,
    next_snapshot_id: u64,
}

fn load_persisted_stores(path: &PathBuf) -> Result<LoadedPersistedState, String> {
    if !path.exists() {
        return Ok(LoadedPersistedState {
            stores: Vec::new(),
            pending: Vec::new(),
            next_snapshot_id: 0,
        });
    }
    let raw = std::fs::read(path).map_err(|err| err.to_string())?;
    let persisted: PersistedAccountStores = serde_json::from_slice(&raw).map_err(|err| err.to_string())?;
    let stores = persisted
        .stores
        .into_iter()
        .map(|store| store.into_store().map_err(|err| format!("{:?}", err)))
        .collect::<Result<Vec<_>, _>>()?;
    let pending = persisted
        .pending
        .into_iter()
        .map(|store| store.into_pending().map_err(|err| format!("{:?}", err)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LoadedPersistedState {
        stores,
        pending,
        next_snapshot_id: persisted.next_snapshot_id,
    })
}

fn persist_account_stores(
    path: &PathBuf,
    stores: impl IntoIterator<Item = PersistedAccountStore>,
    pending: impl IntoIterator<Item = PersistedPendingStore>,
    next_snapshot_id: u64,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let persisted = PersistedAccountStores {
        stores: stores.into_iter().collect(),
        pending: pending.into_iter().collect(),
        next_snapshot_id,
    };
    let raw = serde_json::to_vec_pretty(&persisted).map_err(|err| err.to_string())?;
    std::fs::write(path, raw).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use bloom_offchain_cardano::orders::green::{
        AccountId, AlephAccountAbi, AlephAccountState, AlephAccountUtxo, AlephIntention, GreenOrderId,
    };
    use cml_chain::address::Address;
    use cml_chain::assets::Coin;
    use cml_chain::transaction::TransactionOutput;
    use cml_chain::Value;
    use cml_crypto::TransactionHash;
    use spectrum_cardano_lib::output::FinalizedTxOut;
    use spectrum_cardano_lib::AssetClass;
    use spectrum_cardano_lib::OutputRef;

    use super::{AccountBindingError, AccountIndex, IndexedAccount};

    fn account_id(byte: u8) -> AccountId {
        AccountId::try_from_slice(&[byte; 32]).unwrap()
    }

    fn order_id(byte: u8) -> GreenOrderId {
        GreenOrderId::new(account_id(byte), 0, 42, [byte.saturating_add(1); 32])
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
                abi: AlephAccountAbi::Current,
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
            abi: AlephAccountAbi::Current,
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
    fn transaction_rebinds_empty_successor_for_consumed_bound_account() {
        let id = account_id(31);
        let old_ref = output_ref(33);
        let new_ref = output_ref(34);
        let old_output = dummy_output(2_000_000);
        let new_output = dummy_output(2_100_000);
        let mut index = AccountIndex::default();

        index.observe_created_or_updated(account_utxo(old_ref, old_output));
        index.bind_external_account_id_once(id, old_ref).unwrap();

        index.observe_transaction([old_ref], [account_utxo(new_ref, new_output.clone())]);

        let finalized = index.current(id).expect("empty successor should re-bind account");
        assert_eq!(finalized.reference(), new_ref);
        assert_eq!(finalized.0, new_output);
        let status = index.account_status(id, new_ref);
        assert!(status.current);
        assert!(!status.pending);
        assert!(!status.unbound);
    }

    #[test]
    fn reserved_predicted_store_locks_current_account_until_successor_arrives() {
        let id = account_id(32);
        let old_ref = output_ref(35);
        let new_ref = output_ref(36);
        let pending_intent = intent(1_000);
        let pending_key = pending_intent.intent_key();
        let pending_digest = pending_intent.digest();
        let mut successor_intent = intent(500);
        successor_intent.target_nonce_value = pending_intent.target_nonce_value;
        let mut index = AccountIndex::default();
        let mut store = crate::account_store::AccountStore::empty();
        store
            .insert_remaining(pending_key.clone(), order_id(32), pending_intent.clone())
            .unwrap();
        let mut predicted_store = store.clone();
        predicted_store
            .update_remaining(pending_key.clone(), pending_digest, successor_intent.clone())
            .unwrap();
        let predicted_root = predicted_store.root();

        index.bind_indexed(
            id,
            IndexedAccount {
                utxo: FinalizedTxOut::new(dummy_output(2_000_000), old_ref),
                store,
            },
        );
        assert_eq!(index.pending_continuations().len(), 1);

        index
            .plan_path_update(id, old_ref, pending_key, pending_digest, successor_intent)
            .unwrap();

        assert_eq!(index.current(id), None);
        assert_eq!(
            index
                .current_or_pending_base(id)
                .map(|account| account.reference()),
            Some(old_ref)
        );
        assert!(index.pending_continuations().is_empty());

        index.observe_transaction(
            [old_ref],
            [account_utxo_with_root(
                new_ref,
                dummy_output(2_000_000),
                predicted_root,
            )],
        );

        assert_eq!(
            index.current(id).map(|account| account.reference()),
            Some(new_ref)
        );
        assert_eq!(index.pending_continuations().len(), 1);
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
    fn external_binding_rejects_second_bind_for_same_account_id() {
        let id = account_id(20);
        let ref_1 = output_ref(20);
        let ref_2 = output_ref(21);
        let mut index = AccountIndex::default();
        index.observe_created_or_updated(account_utxo(ref_1, dummy_output(2_000_000)));
        index.observe_created_or_updated(account_utxo(ref_2, dummy_output(2_000_000)));

        assert!(index.bind_external_account_id_once(id, ref_1).is_ok());
        assert_eq!(
            index.bind_external_account_id_once(id, ref_2),
            Err(AccountBindingError::AlreadyBound)
        );
        assert_eq!(index.current(id).map(|account| account.reference()), Some(ref_1));
    }

    #[test]
    fn external_binding_rejects_output_already_bound_to_other_account_id() {
        let id_1 = account_id(21);
        let id_2 = account_id(22);
        let out_ref = output_ref(22);
        let mut index = AccountIndex::default();
        index.observe_created_or_updated(account_utxo(out_ref, dummy_output(2_000_000)));

        assert!(index.bind_external_account_id_once(id_1, out_ref).is_ok());
        assert_eq!(
            index.bind_external_account_id_once(id_2, out_ref),
            Err(AccountBindingError::OutputAlreadyBound)
        );
    }

    #[test]
    fn external_binding_rejects_unknown_output_ref() {
        let id = account_id(23);
        let mut index = AccountIndex::default();

        assert_eq!(
            index.bind_external_account_id_once(id, output_ref(23)),
            Err(AccountBindingError::OutputNotObserved)
        );
    }

    #[test]
    fn one_time_binding_survives_restart_via_persisted_store() {
        let id = account_id(24);
        let out_ref = output_ref(24);
        let output = dummy_output(2_000_000);
        let path = std::env::temp_dir().join(format!(
            "green-account-binding-test-{}-once.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        {
            let mut index = AccountIndex::with_persistence_path(path.clone());
            index.observe_created_or_updated(account_utxo(out_ref, output.clone()));
            assert!(index.bind_external_account_id_once(id, out_ref).is_ok());
        }

        let mut reloaded = AccountIndex::with_persistence_path(path.clone());
        assert_eq!(reloaded.current(id), None);
        reloaded.observe_created_or_updated(account_utxo(out_ref, output));

        assert_eq!(
            reloaded.current(id).map(|account| account.reference()),
            Some(out_ref)
        );
        assert_eq!(
            reloaded.bind_external_account_id_once(id, out_ref),
            Err(AccountBindingError::AlreadyBound)
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persisted_output_ref_cannot_be_rebound_to_different_account_before_lazy_restore() {
        let id_1 = account_id(26);
        let id_2 = account_id(27);
        let out_ref = output_ref(27);
        let output = dummy_output(2_000_000);
        let path = std::env::temp_dir().join(format!(
            "green-account-binding-test-{}-persisted-output-ref.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        {
            let mut index = AccountIndex::with_persistence_path(path.clone());
            index.observe_created_or_updated(account_utxo(out_ref, output.clone()));
            assert!(index.bind_external_account_id_once(id_1, out_ref).is_ok());
        }

        let mut reloaded = AccountIndex::with_persistence_path(path.clone());
        assert_eq!(reloaded.current(id_1), None);

        assert_eq!(
            reloaded.bind_external_account_id_once(id_2, out_ref),
            Err(AccountBindingError::OutputAlreadyBound)
        );
        reloaded.observe_created_or_updated(account_utxo(out_ref, output));
        assert_eq!(
            reloaded.current(id_1).map(|account| account.reference()),
            Some(out_ref)
        );
        let _ = std::fs::remove_file(path);
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
    fn transaction_binds_duplicate_identical_pending_snapshots_from_rebuilds() {
        let id = account_id(20);
        let old_ref = output_ref(18);
        let new_ref = output_ref(19);
        let mut planned_store = crate::account_store::AccountStore::empty();
        let remaining = intent(500);
        planned_store
            .insert_remaining(remaining.intent_key(), order_id(20), remaining)
            .unwrap();
        let root = planned_store.root();
        let mut index = AccountIndex::default();
        let first_snapshot = index.reserve_predicted_store_snapshot(id, old_ref, planned_store.clone());
        let second_snapshot = index.reserve_predicted_store_snapshot(id, old_ref, planned_store);

        index.observe_transaction(
            [old_ref],
            [account_utxo_with_root(new_ref, dummy_output(2_000_000), root)],
        );

        assert!(index.pending_snapshot(first_snapshot).is_none());
        assert!(index.pending_snapshot(second_snapshot).is_none());
        assert_eq!(
            index.current(id).map(|account| account.reference()),
            Some(new_ref)
        );
        assert_eq!(index.pending_continuations().len(), 1);
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
    fn account_status_distinguishes_persisted_snapshot_from_current_binding() {
        let id = account_id(28);
        let output_ref = output_ref(28);
        let output = dummy_output(2_000_000);
        let path = std::env::temp_dir().join(format!(
            "green-account-store-test-{}-status.json",
            std::process::id()
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
        let persisted = reloaded.account_status(id, output_ref);
        assert!(!persisted.current);
        assert!(persisted.persisted);
        assert!(!persisted.pending);
        assert!(!persisted.unbound);

        reloaded.observe_created_or_updated(account_utxo(output_ref, output));
        let current = reloaded.account_status(id, output_ref);
        assert!(current.current);
        assert!(!current.persisted);
        assert!(!current.pending);
        assert!(!current.unbound);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn account_summary_reports_current_account_reference_and_store_root() {
        let id = account_id(91);
        let output_ref = output_ref(91);
        let output = dummy_output(2_000_000);
        let mut index = AccountIndex::default();
        index.observe_created_or_updated(account_utxo_with_root(output_ref, output, [0; 32]));
        index.bind_external_account_id(id, output_ref).unwrap();

        let summary = index.account_summary(id).unwrap();

        assert_eq!(summary.account_id, id);
        assert_eq!(summary.current_output_ref, Some(output_ref));
        assert_eq!(summary.current_store_root, Some([0; 32]));
        assert!(!summary.pending);
        assert_eq!(summary.persisted_outputs, 0);
    }

    #[test]
    fn account_summary_reports_pending_account_without_current_output() {
        let id = account_id(92);
        let old_ref = output_ref(92);
        let new_ref = output_ref(93);
        let mut index = AccountIndex::default();

        index.mark_pending_update(id, old_ref, new_ref, dummy_output(2_100_000));

        let summary = index.account_summary(id).unwrap();

        assert_eq!(summary.account_id, id);
        assert_eq!(summary.current_output_ref, None);
        assert_eq!(summary.current_store_root, None);
        assert!(summary.pending);
        assert_eq!(summary.persisted_outputs, 0);
    }

    #[test]
    fn account_summary_reports_persisted_account_before_observation() {
        let id = account_id(93);
        let output_ref = output_ref(94);
        let output = dummy_output(2_000_000);
        let path = std::env::temp_dir().join(format!(
            "green-account-store-test-{}-summary.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        {
            let mut index = AccountIndex::with_persistence_path(path.clone());
            index.bind_indexed(
                id,
                IndexedAccount {
                    utxo: FinalizedTxOut::new(output, output_ref),
                    store: crate::account_store::AccountStore::empty(),
                },
            );
        }

        let reloaded = AccountIndex::with_persistence_path(path.clone());
        let summary = reloaded.account_summary(id).unwrap();

        assert_eq!(summary.account_id, id);
        assert_eq!(summary.current_output_ref, None);
        assert_eq!(summary.current_store_root, None);
        assert!(!summary.pending);
        assert_eq!(summary.persisted_outputs, 1);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn account_summary_returns_none_for_unknown_account() {
        assert!(AccountIndex::default().account_summary(account_id(94)).is_none());
    }

    #[test]
    fn account_index_summary_counts_empty_state() {
        let summary = AccountIndex::default().summary();

        assert_eq!(summary.current_accounts, 0);
        assert_eq!(summary.pending_accounts, 0);
        assert_eq!(summary.unbound_outputs, 0);
        assert_eq!(summary.predicted_outputs, 0);
        assert_eq!(summary.persisted_outputs, 0);
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
    fn pending_store_snapshot_survives_restart_and_binds_confirmed_successor() {
        let id = account_id(29);
        let old_ref = output_ref(29);
        let new_ref = output_ref(30);
        let old_output = dummy_output(2_000_000);
        let new_output = dummy_output(2_100_000);
        let canonical_order_id = GreenOrderId::new(id, 0, 42, [29; 32]);
        let updated_intent = intent(500);
        let path = std::env::temp_dir().join(format!(
            "green-account-store-test-{}-pending-restart.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let planned_root = {
            let mut index = AccountIndex::with_persistence_path(path.clone());
            index.bind_indexed(
                id,
                IndexedAccount {
                    utxo: FinalizedTxOut::new(old_output, old_ref),
                    store: crate::account_store::AccountStore::empty(),
                },
            );
            index
                .plan_sig_insert(
                    id,
                    old_ref,
                    canonical_order_id,
                    updated_intent.intent_key(),
                    updated_intent,
                )
                .unwrap()
                .new_root
        };

        let mut reloaded = AccountIndex::with_persistence_path(path.clone());
        reloaded.observe_transaction(
            [old_ref],
            [account_utxo_with_root(new_ref, new_output, planned_root)],
        );

        assert_eq!(
            reloaded.current(id).map(|account| account.reference()),
            Some(new_ref)
        );
        assert_eq!(reloaded.pending_continuations().len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persisted_non_empty_store_reloads_with_continuation_after_observation() {
        let id = account_id(30);
        let old_ref = output_ref(31);
        let new_ref = output_ref(32);
        let old_output = dummy_output(2_000_000);
        let new_output = dummy_output(2_100_000);
        let canonical_order_id = GreenOrderId::new(id, 0, 42, [30; 32]);
        let updated_intent = intent(500);
        let path = std::env::temp_dir().join(format!(
            "green-account-store-test-{}-persisted-non-empty.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let planned_root = {
            let mut index = AccountIndex::with_persistence_path(path.clone());
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
                [account_utxo_with_root(
                    new_ref,
                    new_output.clone(),
                    planned.new_root,
                )],
            );
            planned.new_root
        };

        let mut reloaded = AccountIndex::with_persistence_path(path.clone());
        assert_eq!(reloaded.current(id), None);

        reloaded.observe_created_or_updated(account_utxo_with_root(new_ref, new_output, planned_root));

        assert_eq!(
            reloaded.current(id).map(|account| account.reference()),
            Some(new_ref)
        );
        let continuations = reloaded.pending_continuations();
        assert_eq!(continuations.len(), 1);
        assert_eq!(continuations[0].0.id, canonical_order_id);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn once_bound_account_tracks_planned_later_account_output_without_rebinding() {
        let id = account_id(25);
        let old_ref = output_ref(25);
        let new_ref = output_ref(26);
        let old_output = dummy_output(2_000_000);
        let new_output = dummy_output(2_100_000);
        let canonical_order_id = GreenOrderId::new(id, 0, 42, [25; 32]);
        let updated_intent = intent(500);
        let mut index = AccountIndex::default();

        index.observe_created_or_updated(account_utxo(old_ref, old_output));
        index.bind_external_account_id_once(id, old_ref).unwrap();

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

        assert_eq!(
            index.current(id).map(|account| account.reference()),
            Some(new_ref)
        );
        assert_eq!(
            index.bind_external_account_id_once(id, new_ref),
            Err(AccountBindingError::AlreadyBound)
        );
    }

    #[test]
    fn consumed_persisted_store_binds_successor_output_after_restart() {
        let id = account_id(21);
        let old_ref = output_ref(33);
        let new_ref = output_ref(34);
        let old_output = dummy_output(2_000_000);
        let new_output = dummy_output(2_100_000);
        let canonical_order_id = GreenOrderId::new(id, 0, 42, [21; 32]);
        let updated_intent = intent(500);
        let path = std::env::temp_dir().join(format!(
            "green-account-store-test-{}-consumed-persisted.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let planned_root = {
            let mut index = AccountIndex::with_persistence_path(path.clone());
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
            planned.new_root
        };

        let mut reloaded = AccountIndex::with_persistence_path(path.clone());
        reloaded.observe_transaction(
            [old_ref],
            [account_utxo_with_root(new_ref, new_output, planned_root)],
        );

        assert_eq!(
            reloaded.current(id).map(|account| account.reference()),
            Some(new_ref)
        );
        let continuations = reloaded.pending_continuations();
        assert_eq!(continuations.len(), 1);
        assert_eq!(continuations[0].0.id, canonical_order_id);
        let _ = std::fs::remove_file(path);
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
