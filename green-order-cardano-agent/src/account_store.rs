use std::collections::BTreeMap;

use bloom_offchain_cardano::orders::green::{
    AlephIntention, GreenOrderId, GreenStorePlanningError, PlannedStoreCompletion, PlannedStoreDelta,
    StoreSnapshotId,
};

use crate::mpf;

pub const ALEPH_MPF_EMPTY_ROOT: [u8; 32] = mpf::EMPTY_ROOT;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AccountStore {
    root: [u8; 32],
    leaves: BTreeMap<Vec<u8>, StoredIntentLeaf>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoredIntentLeaf {
    pub canonical_order_id: GreenOrderId,
    pub intent: AlephIntention,
    pub digest: [u8; 32],
    pub status: StoredIntentStatus,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StoredIntentStatus {
    Pending,
    Completed,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoredIntentLeafSnapshot {
    pub key: Vec<u8>,
    pub canonical_order_id: GreenOrderId,
    pub intent: AlephIntention,
    pub digest: [u8; 32],
    pub status: StoredIntentStatus,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MpfDelta {
    pub old_root: [u8; 32],
    pub new_root: [u8; 32],
    pub proof: Vec<u8>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MpfCompletion {
    pub root: [u8; 32],
    pub proof: Vec<u8>,
}

impl AccountStore {
    pub fn empty() -> Self {
        Self {
            root: ALEPH_MPF_EMPTY_ROOT,
            leaves: BTreeMap::new(),
        }
    }

    pub fn from_observed_root(root: [u8; 32]) -> Option<Self> {
        if root == ALEPH_MPF_EMPTY_ROOT {
            Some(Self::empty())
        } else {
            None
        }
    }

    pub fn from_snapshot(
        root: [u8; 32],
        leaves: Vec<StoredIntentLeafSnapshot>,
    ) -> Result<Self, GreenStorePlanningError> {
        let leaves = leaves
            .into_iter()
            .map(|leaf| {
                (
                    leaf.key,
                    StoredIntentLeaf {
                        canonical_order_id: leaf.canonical_order_id,
                        intent: leaf.intent,
                        digest: leaf.digest,
                        status: leaf.status,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let calculated_root = trie_from_leaves(&leaves).root();
        if calculated_root != root {
            return Err(GreenStorePlanningError::RootMismatch {
                expected: calculated_root,
                actual: root,
            });
        }
        Ok(Self { root, leaves })
    }

    pub fn root(&self) -> [u8; 32] {
        self.root
    }

    pub fn snapshot_leaves(&self) -> Vec<StoredIntentLeafSnapshot> {
        self.leaves
            .iter()
            .map(|(key, leaf)| StoredIntentLeafSnapshot {
                key: key.clone(),
                canonical_order_id: leaf.canonical_order_id,
                intent: leaf.intent.clone(),
                digest: leaf.digest,
                status: leaf.status,
            })
            .collect()
    }

    pub fn insert_remaining(
        &mut self,
        key: Vec<u8>,
        canonical_order_id: GreenOrderId,
        intent: AlephIntention,
    ) -> Result<MpfDelta, GreenStorePlanningError> {
        if self.leaves.contains_key(&key) {
            return Err(GreenStorePlanningError::ExistingPendingIntent);
        }
        let old_root = self.root;
        let digest = intent.digest();
        let trie = trie_from_leaves(&self.leaves);
        let proof = trie.proof_for_missing(&key)?;
        let new_root = mpf::insert(old_root, &key, &digest, &proof)?;
        self.leaves.insert(
            key,
            StoredIntentLeaf {
                canonical_order_id,
                intent,
                digest,
                status: StoredIntentStatus::Pending,
            },
        );
        self.root = new_root;
        Ok(MpfDelta {
            old_root,
            new_root,
            proof: mpf::proof_to_cbor(&proof),
        })
    }

    pub fn update_remaining(
        &mut self,
        key: Vec<u8>,
        old_digest: [u8; 32],
        new_intent: AlephIntention,
    ) -> Result<MpfDelta, GreenStorePlanningError> {
        let old_root = self.root;
        let trie = trie_from_leaves(&self.leaves);
        let proof = trie.proof_for_present(&key)?;
        let leaf = self
            .leaves
            .get_mut(&key)
            .ok_or(GreenStorePlanningError::MissingPendingIntent)?;
        if leaf.status != StoredIntentStatus::Pending {
            return Err(GreenStorePlanningError::MissingPendingIntent);
        }
        if leaf.digest != old_digest {
            return Err(GreenStorePlanningError::DigestMismatch);
        }
        let new_digest = new_intent.digest();
        let new_root = mpf::update(old_root, &key, &proof, &old_digest, &new_digest)?;
        leaf.intent = new_intent;
        leaf.digest = new_digest;
        self.root = new_root;
        Ok(MpfDelta {
            old_root,
            new_root: self.root,
            proof: mpf::proof_to_cbor(&proof),
        })
    }

    pub fn mark_completed(
        &mut self,
        key: Vec<u8>,
        old_digest: [u8; 32],
    ) -> Result<MpfCompletion, GreenStorePlanningError> {
        let trie = trie_from_leaves(&self.leaves);
        let proof = trie.proof_for_present(&key)?;
        let leaf = self
            .leaves
            .get_mut(&key)
            .ok_or(GreenStorePlanningError::MissingPendingIntent)?;
        if leaf.status != StoredIntentStatus::Pending {
            return Err(GreenStorePlanningError::MissingPendingIntent);
        }
        if leaf.digest != old_digest {
            return Err(GreenStorePlanningError::DigestMismatch);
        }
        if !mpf::has(self.root, &key, &old_digest, &proof) {
            return Err(GreenStorePlanningError::RootMismatch {
                expected: trie.root(),
                actual: self.root,
            });
        }
        leaf.status = StoredIntentStatus::Completed;
        Ok(MpfCompletion {
            root: self.root,
            proof: mpf::proof_to_cbor(&proof),
        })
    }

    pub fn pending_leaf(&self, key: &[u8]) -> Option<&StoredIntentLeaf> {
        self.leaves
            .get(key)
            .filter(|leaf| leaf.status == StoredIntentStatus::Pending)
    }

    pub fn pending_leaves(&self) -> impl Iterator<Item = (&Vec<u8>, &StoredIntentLeaf)> {
        self.leaves
            .iter()
            .filter(|(_, leaf)| leaf.status == StoredIntentStatus::Pending)
    }

    pub fn path_proof(&self, key: &[u8]) -> Result<Vec<u8>, GreenStorePlanningError> {
        let leaf = self
            .leaves
            .get(key)
            .ok_or(GreenStorePlanningError::MissingPendingIntent)?;
        if leaf.status != StoredIntentStatus::Pending {
            return Err(GreenStorePlanningError::MissingPendingIntent);
        }
        Ok(mpf::proof_to_cbor(
            &trie_from_leaves(&self.leaves).proof_for_present(key)?,
        ))
    }
}

impl From<mpf::MpfError> for GreenStorePlanningError {
    fn from(_: mpf::MpfError) -> Self {
        GreenStorePlanningError::Unsupported
    }
}

pub fn planned_delta(delta: MpfDelta, snapshot_id: StoreSnapshotId) -> PlannedStoreDelta {
    PlannedStoreDelta {
        old_root: delta.old_root,
        new_root: delta.new_root,
        proof: delta.proof,
        predicted_snapshot_id: snapshot_id,
    }
}

pub fn planned_completion(completion: MpfCompletion, snapshot_id: StoreSnapshotId) -> PlannedStoreCompletion {
    PlannedStoreCompletion {
        root: completion.root,
        proof: completion.proof,
        predicted_snapshot_id: snapshot_id,
    }
}

fn trie_from_leaves(leaves: &BTreeMap<Vec<u8>, StoredIntentLeaf>) -> mpf::Trie {
    mpf::Trie::from_leaves(
        leaves
            .iter()
            .map(|(key, leaf)| (key.clone(), leaf.digest.to_vec()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use bloom_offchain_cardano::orders::green::{AccountId, AlephIntention, GreenOrderId};
    use spectrum_cardano_lib::AssetClass;

    use super::{AccountStore, StoredIntentStatus, ALEPH_MPF_EMPTY_ROOT};

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

    fn order_id(seed: u8) -> GreenOrderId {
        GreenOrderId::new(
            AccountId::try_from_slice(&[seed; 32]).unwrap(),
            0,
            42,
            [seed.saturating_add(1); 32],
        )
    }

    #[test]
    fn empty_store_root_matches_aleph_null_hash() {
        let store = AccountStore::empty();

        assert_eq!(store.root(), ALEPH_MPF_EMPTY_ROOT);
    }

    #[test]
    fn insert_remaining_intent_changes_root_and_produces_proof() {
        let mut store = AccountStore::empty();
        let key = vec![1, 2, 3];
        let intent = intent(1_000);
        let digest = intent.digest();

        let canonical_order_id = order_id(1);
        let delta = store
            .insert_remaining(key.clone(), canonical_order_id, intent.clone())
            .unwrap();

        assert_eq!(delta.old_root, ALEPH_MPF_EMPTY_ROOT);
        assert_ne!(delta.new_root, delta.old_root);
        assert!(!delta.proof.is_empty());
        let leaf = store.pending_leaf(&key).unwrap();
        assert_eq!(leaf.intent, intent);
        assert_eq!(leaf.canonical_order_id, canonical_order_id);
        assert_eq!(leaf.digest, digest);
        assert_eq!(leaf.status, StoredIntentStatus::Pending);
    }

    #[test]
    fn update_requires_existing_digest() {
        let mut store = AccountStore::empty();
        let key = vec![1, 2, 3];
        let old_intent = intent(1_000);
        store
            .insert_remaining(key.clone(), order_id(2), old_intent.clone())
            .unwrap();

        let err = store.update_remaining(key, [9; 32], intent(500)).unwrap_err();

        assert_eq!(
            err,
            bloom_offchain_cardano::orders::green::GreenStorePlanningError::DigestMismatch
        );
    }

    #[test]
    fn failed_update_does_not_mutate_leaf() {
        let mut store = AccountStore::empty();
        let key = vec![1, 2, 3];
        let old_intent = intent(1_000);
        let old_digest = old_intent.digest();
        store
            .insert_remaining(key.clone(), order_id(6), old_intent.clone())
            .unwrap();
        store.root = [9; 32];

        let err = store
            .update_remaining(key.clone(), old_digest, intent(500))
            .unwrap_err();

        assert_eq!(
            err,
            bloom_offchain_cardano::orders::green::GreenStorePlanningError::Unsupported
        );
        let leaf = store.pending_leaf(&key).unwrap();
        assert_eq!(leaf.intent, old_intent);
        assert_eq!(leaf.digest, old_digest);
    }

    #[test]
    fn path_full_fill_preserves_root_but_marks_leaf_completed() {
        let mut store = AccountStore::empty();
        let key = vec![1, 2, 3];
        let intent = intent(1_000);
        let digest = intent.digest();
        store.insert_remaining(key.clone(), order_id(3), intent).unwrap();
        let old_root = store.root();

        store.mark_completed(key.clone(), digest).unwrap();

        assert_eq!(store.root(), old_root);
        assert!(store.pending_leaf(&key).is_none());
    }

    #[test]
    fn inserts_into_non_empty_store_and_keeps_completed_leaves() {
        let mut store = AccountStore::empty();
        let first_key = vec![1, 2, 3];
        let second_key = vec![4, 5, 6];
        let first = intent(1_000);
        let first_digest = first.digest();
        store
            .insert_remaining(first_key.clone(), order_id(4), first)
            .unwrap();
        let singleton_root = store.root();
        store.mark_completed(first_key.clone(), first_digest).unwrap();

        let second_delta = store
            .insert_remaining(second_key.clone(), order_id(5), intent(2_000))
            .unwrap();

        assert_eq!(second_delta.old_root, singleton_root);
        assert_ne!(second_delta.new_root, singleton_root);
        assert!(store.pending_leaf(&first_key).is_none());
        assert!(store.pending_leaf(&second_key).is_some());
    }

    #[test]
    fn account_store_root_matches_mpf_trie_for_pending_and_completed_leaves() {
        let mut store = AccountStore::empty();
        store
            .insert_remaining(vec![1, 2, 3], order_id(1), intent(1_000))
            .unwrap();
        store
            .insert_remaining(vec![4, 5, 6], order_id(2), intent(2_000))
            .unwrap();

        let mut trie = crate::mpf::Trie::empty();
        for leaf in store.snapshot_leaves() {
            trie.insert(leaf.key, leaf.digest.to_vec()).unwrap();
        }

        assert_eq!(store.root(), trie.root());
    }
}
