use std::collections::BTreeMap;

use cml_chain::plutus::utils::ConstrPlutusDataEncoding;
use cml_chain::plutus::{ConstrPlutusData, PlutusData};
use cml_core::serialization::{LenEncoding, Serialize};
use spectrum_cardano_lib::plutus_data::IntoPlutusData;

pub const EMPTY_ROOT: [u8; 32] = [0; 32];

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ProofStep {
    Branch {
        skip: usize,
        neighbors: Vec<u8>,
    },
    Fork {
        skip: usize,
        neighbor: Neighbor,
    },
    Leaf {
        skip: usize,
        key: [u8; 32],
        value: [u8; 32],
    },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Neighbor {
    pub nibble: u8,
    pub prefix: Vec<u8>,
    pub root: [u8; 32],
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MpfError {
    InvalidBranchNeighbors { actual: usize },
    InvalidProof,
    ExistingKey,
    MissingKey,
    PathExhausted,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Trie {
    leaves: BTreeMap<Vec<u8>, Vec<u8>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct Leaf {
    key: Vec<u8>,
    path: [u8; 32],
    value: Vec<u8>,
    value_hash: [u8; 32],
}

impl Trie {
    pub fn empty() -> Self {
        Self {
            leaves: BTreeMap::new(),
        }
    }

    pub fn from_leaves(leaves: BTreeMap<Vec<u8>, Vec<u8>>) -> Self {
        Self { leaves }
    }

    pub fn root(&self) -> [u8; 32] {
        root_for(&self.mpf_leaves(), 0)
    }

    pub fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<[u8; 32], MpfError> {
        if self.leaves.contains_key(&key) {
            return Err(MpfError::ExistingKey);
        }
        self.leaves.insert(key, value);
        Ok(self.root())
    }

    pub fn update(&mut self, key: &[u8], value: Vec<u8>) -> Result<[u8; 32], MpfError> {
        let leaf = self.leaves.get_mut(key).ok_or(MpfError::MissingKey)?;
        *leaf = value;
        Ok(self.root())
    }

    pub fn delete(&mut self, key: &[u8]) -> Result<[u8; 32], MpfError> {
        self.leaves.remove(key).ok_or(MpfError::MissingKey)?;
        Ok(self.root())
    }

    pub fn proof_for_missing(&self, key: &[u8]) -> Result<Vec<ProofStep>, MpfError> {
        if self.leaves.contains_key(key) {
            return Err(MpfError::ExistingKey);
        }
        proof_for_path(&self.mpf_leaves(), &hash(key), 0)
    }

    pub fn proof_for_present(&self, key: &[u8]) -> Result<Vec<ProofStep>, MpfError> {
        if !self.leaves.contains_key(key) {
            return Err(MpfError::MissingKey);
        }
        proof_for_path(&self.mpf_leaves(), &hash(key), 0)
    }

    fn mpf_leaves(&self) -> Vec<Leaf> {
        self.leaves
            .iter()
            .map(|(key, value)| Leaf {
                key: key.clone(),
                path: hash(key),
                value: value.clone(),
                value_hash: hash(value),
            })
            .collect()
    }
}

pub fn has(root: [u8; 32], key: &[u8], value: &[u8], proof: &[ProofStep]) -> bool {
    including(&hash(key), &hash(value), 0, proof) == Ok(root)
}

pub fn miss(root: [u8; 32], key: &[u8], proof: &[ProofStep]) -> bool {
    excluding(&hash(key), 0, proof) == Ok(root)
}

pub fn insert(root: [u8; 32], key: &[u8], value: &[u8], proof: &[ProofStep]) -> Result<[u8; 32], MpfError> {
    let path = hash(key);
    if excluding(&path, 0, proof)? != root {
        return Err(MpfError::InvalidProof);
    }
    including(&path, &hash(value), 0, proof)
}

pub fn update(
    root: [u8; 32],
    key: &[u8],
    proof: &[ProofStep],
    old_value: &[u8],
    new_value: &[u8],
) -> Result<[u8; 32], MpfError> {
    let path = hash(key);
    if including(&path, &hash(old_value), 0, proof)? != root {
        return Err(MpfError::InvalidProof);
    }
    including(&path, &hash(new_value), 0, proof)
}

pub fn delete(root: [u8; 32], key: &[u8], value: &[u8], proof: &[ProofStep]) -> Result<[u8; 32], MpfError> {
    let path = hash(key);
    if including(&path, &hash(value), 0, proof)? != root {
        return Err(MpfError::InvalidProof);
    }
    excluding(&path, 0, proof)
}

pub fn proof_to_cbor(proof: &[ProofStep]) -> Vec<u8> {
    proof_to_plutus_data(proof).to_cbor_bytes()
}

pub fn proof_to_plutus_data(proof: &[ProofStep]) -> PlutusData {
    PlutusData::List {
        list: proof.iter().map(step_to_plutus_data).collect::<Vec<_>>(),
        list_encoding: LenEncoding::Indefinite,
    }
}

fn step_to_plutus_data(step: &ProofStep) -> PlutusData {
    match step {
        ProofStep::Branch { skip, neighbors } => {
            aiken_constr(0, vec![(*skip as u64).into_pd(), neighbors.clone().into_pd()])
        }
        ProofStep::Fork { skip, neighbor } => aiken_constr(
            1,
            vec![
                (*skip as u64).into_pd(),
                aiken_constr(
                    0,
                    vec![
                        u64::from(neighbor.nibble).into_pd(),
                        neighbor.prefix.clone().into_pd(),
                        neighbor.root.to_vec().into_pd(),
                    ],
                ),
            ],
        ),
        ProofStep::Leaf { skip, key, value } => aiken_constr(
            2,
            vec![
                (*skip as u64).into_pd(),
                key.to_vec().into_pd(),
                value.to_vec().into_pd(),
            ],
        ),
    }
}

fn aiken_constr(alternative: u64, fields: Vec<PlutusData>) -> PlutusData {
    PlutusData::new_constr_plutus_data(ConstrPlutusData {
        alternative,
        fields,
        encodings: Some(ConstrPlutusDataEncoding {
            len_encoding: LenEncoding::Indefinite,
            prefer_compact: true,
            tag_encoding: None,
            alternative_encoding: None,
            fields_encoding: LenEncoding::Indefinite,
        }),
    })
}

fn including(
    path: &[u8; 32],
    value_hash: &[u8; 32],
    cursor: usize,
    proof: &[ProofStep],
) -> Result<[u8; 32], MpfError> {
    ensure_cursor(cursor)?;
    match proof.split_first() {
        None => Ok(combine(&suffix(path, cursor)?, value_hash)),
        Some((ProofStep::Branch { skip, neighbors }, steps)) => {
            validate_neighbors(neighbors)?;
            let next_cursor = cursor.checked_add(*skip).ok_or(MpfError::PathExhausted)?;
            ensure_cursor(next_cursor)?;
            let root = including(path, value_hash, next_cursor + 1, steps)?;
            branch_root(path, cursor, next_cursor, &root, neighbors)
        }
        Some((ProofStep::Fork { skip, neighbor }, steps)) => {
            let next_cursor = cursor.checked_add(*skip).ok_or(MpfError::PathExhausted)?;
            ensure_cursor(next_cursor)?;
            let root = including(path, value_hash, next_cursor + 1, steps)?;
            fork_root(path, cursor, next_cursor, &root, neighbor)
        }
        Some((ProofStep::Leaf { skip, key, value }, steps)) => {
            let next_cursor = cursor.checked_add(*skip).ok_or(MpfError::PathExhausted)?;
            ensure_cursor(next_cursor)?;
            let root = including(path, value_hash, next_cursor + 1, steps)?;
            let neighbor = Neighbor {
                prefix: suffix(key, next_cursor + 1)?,
                nibble: nibble(key, next_cursor)? as u8,
                root: *value,
            };
            fork_root(path, cursor, next_cursor, &root, &neighbor)
        }
    }
}

fn excluding(path: &[u8; 32], cursor: usize, proof: &[ProofStep]) -> Result<[u8; 32], MpfError> {
    ensure_cursor(cursor)?;
    match proof.split_first() {
        None => Ok(EMPTY_ROOT),
        Some((ProofStep::Branch { skip, neighbors }, steps)) => {
            validate_neighbors(neighbors)?;
            let next_cursor = cursor.checked_add(*skip).ok_or(MpfError::PathExhausted)?;
            ensure_cursor(next_cursor)?;
            let root = excluding(path, next_cursor + 1, steps)?;
            branch_root(path, cursor, next_cursor, &root, neighbors)
        }
        Some((ProofStep::Fork { skip, neighbor }, [])) => {
            if neighbor.nibble > 15 {
                return Err(MpfError::InvalidProof);
            }
            let mut prefix = Vec::new();
            if *skip > 0 {
                prefix.extend_from_slice(&nibbles(path, cursor, cursor + *skip)?);
            }
            prefix.push(neighbor.nibble);
            prefix.extend_from_slice(&neighbor.prefix);
            Ok(combine(&prefix, &neighbor.root))
        }
        Some((ProofStep::Fork { skip, neighbor }, steps)) => {
            let next_cursor = cursor.checked_add(*skip).ok_or(MpfError::PathExhausted)?;
            ensure_cursor(next_cursor)?;
            let root = excluding(path, next_cursor + 1, steps)?;
            fork_root(path, cursor, next_cursor, &root, neighbor)
        }
        Some((ProofStep::Leaf { key, value, .. }, [])) => Ok(combine(&suffix(key, cursor)?, value)),
        Some((ProofStep::Leaf { skip, key, value }, steps)) => {
            let next_cursor = cursor.checked_add(*skip).ok_or(MpfError::PathExhausted)?;
            ensure_cursor(next_cursor)?;
            let root = excluding(path, next_cursor + 1, steps)?;
            let neighbor = Neighbor {
                prefix: suffix(key, next_cursor + 1)?,
                nibble: nibble(key, next_cursor)? as u8,
                root: *value,
            };
            fork_root(path, cursor, next_cursor, &root, &neighbor)
        }
    }
}

fn proof_for_path(leaves: &[Leaf], path: &[u8; 32], cursor: usize) -> Result<Vec<ProofStep>, MpfError> {
    match leaves.len() {
        0 => Ok(vec![]),
        1 => {
            let leaf = &leaves[0];
            if &leaf.path == path {
                return Ok(vec![]);
            }
            let split = first_divergence(path, &leaf.path, cursor)?;
            Ok(vec![ProofStep::Leaf {
                skip: split - cursor,
                key: leaf.path,
                value: leaf.value_hash,
            }])
        }
        _ => {
            let split = common_split(leaves, cursor)?;
            let branch = nibble(path, split)?;
            let children = child_groups(leaves, split)?;
            let mut proof = vec![proof_step_for_branch(path, cursor, split, branch, &children)?];
            if !children[branch].is_empty() {
                proof.extend(proof_for_path(&children[branch], path, split + 1)?);
            }
            Ok(proof)
        }
    }
}

fn proof_step_for_branch(
    path: &[u8; 32],
    cursor: usize,
    split: usize,
    branch: usize,
    children: &[Vec<Leaf>; 16],
) -> Result<ProofStep, MpfError> {
    let neighbors = children
        .iter()
        .enumerate()
        .filter(|(ix, leaves)| *ix != branch && !leaves.is_empty())
        .collect::<Vec<_>>();
    if neighbors.len() == 1 {
        let (nibble, leaves) = neighbors[0];
        return sparse_neighbor_step(split - cursor, split + 1, nibble, leaves);
    }

    let child_roots = child_roots(children, split + 1);
    Ok(ProofStep::Branch {
        skip: split - cursor,
        neighbors: branch_neighbors(nibble(path, split)?, &child_roots),
    })
}

fn sparse_neighbor_step(
    skip: usize,
    cursor: usize,
    nibble: usize,
    leaves: &[Leaf],
) -> Result<ProofStep, MpfError> {
    if leaves.len() == 1 {
        return Ok(ProofStep::Leaf {
            skip,
            key: leaves[0].path,
            value: leaves[0].value_hash,
        });
    }

    let split = common_split(leaves, cursor)?;
    let children = child_groups(leaves, split)?;
    let child_roots = child_roots(&children, split + 1);
    Ok(ProofStep::Fork {
        skip,
        neighbor: Neighbor {
            nibble: nibble as u8,
            prefix: nibbles(&leaves[0].path, cursor, split)?,
            root: merkle_16_roots(&child_roots),
        },
    })
}

fn root_for(leaves: &[Leaf], cursor: usize) -> [u8; 32] {
    match leaves.len() {
        0 => EMPTY_ROOT,
        1 => combine(
            &suffix(&leaves[0].path, cursor).expect("valid MPF leaf cursor"),
            &leaves[0].value_hash,
        ),
        _ => {
            let split =
                common_split(leaves, cursor).expect("multiple MPF leaves must diverge before path end");
            let children = child_groups(leaves, split).expect("valid MPF split");
            let child_roots = child_roots(&children, split + 1);
            let subtree = merkle_16_roots(&child_roots);
            combine(
                &nibbles(&leaves[0].path, cursor, split).expect("valid MPF prefix"),
                &subtree,
            )
        }
    }
}

fn child_groups(leaves: &[Leaf], split: usize) -> Result<[Vec<Leaf>; 16], MpfError> {
    let mut children: [Vec<Leaf>; 16] = std::array::from_fn(|_| Vec::new());
    for leaf in leaves {
        children[nibble(&leaf.path, split)?].push(leaf.clone());
    }
    Ok(children)
}

fn child_roots(children: &[Vec<Leaf>; 16], cursor: usize) -> [[u8; 32]; 16] {
    std::array::from_fn(|ix| root_for(&children[ix], cursor))
}

fn common_split(leaves: &[Leaf], cursor: usize) -> Result<usize, MpfError> {
    let first = leaves.first().ok_or(MpfError::InvalidProof)?;
    for ix in cursor..64 {
        let nib = nibble(&first.path, ix)?;
        if leaves.iter().any(|leaf| nibble(&leaf.path, ix) != Ok(nib)) {
            return Ok(ix);
        }
    }
    Err(MpfError::PathExhausted)
}

fn first_divergence(lhs: &[u8; 32], rhs: &[u8; 32], cursor: usize) -> Result<usize, MpfError> {
    (cursor..64)
        .find(|ix| nibble(lhs, *ix) != nibble(rhs, *ix))
        .ok_or(MpfError::PathExhausted)
}

fn branch_root(
    path: &[u8; 32],
    cursor: usize,
    next_cursor: usize,
    root: &[u8; 32],
    neighbors: &[u8],
) -> Result<[u8; 32], MpfError> {
    validate_neighbors(neighbors)?;
    let branch = nibble(path, next_cursor)?;
    let prefix = nibbles(path, cursor, next_cursor)?;
    Ok(combine(&prefix, &merkle_16(branch, root, neighbors)?))
}

fn fork_root(
    path: &[u8; 32],
    cursor: usize,
    next_cursor: usize,
    root: &[u8; 32],
    neighbor: &Neighbor,
) -> Result<[u8; 32], MpfError> {
    let branch = nibble(path, next_cursor)?;
    if branch == usize::from(neighbor.nibble) || neighbor.nibble > 15 {
        return Err(MpfError::InvalidProof);
    }
    let prefix = nibbles(path, cursor, next_cursor)?;
    let neighbor_root = combine(&neighbor.prefix, &neighbor.root);
    Ok(combine(
        &prefix,
        &sparse_merkle_16(branch, root, usize::from(neighbor.nibble), &neighbor_root),
    ))
}

fn validate_neighbors(neighbors: &[u8]) -> Result<(), MpfError> {
    if neighbors.len() != 128 {
        return Err(MpfError::InvalidBranchNeighbors {
            actual: neighbors.len(),
        });
    }
    Ok(())
}

fn merkle_16(branch: usize, root: &[u8; 32], neighbors: &[u8]) -> Result<[u8; 32], MpfError> {
    validate_neighbors(neighbors)?;
    let neighbor_8: [u8; 32] = neighbors[0..32].try_into().unwrap();
    let neighbor_4: [u8; 32] = neighbors[32..64].try_into().unwrap();
    let neighbor_2: [u8; 32] = neighbors[64..96].try_into().unwrap();
    let neighbor_1: [u8; 32] = neighbors[96..128].try_into().unwrap();
    let pair = if branch % 2 == 0 {
        combine(root, &neighbor_1)
    } else {
        combine(&neighbor_1, root)
    };
    let group_4 = if branch % 4 < 2 {
        combine(&pair, &neighbor_2)
    } else {
        combine(&neighbor_2, &pair)
    };
    let group_8 = if branch % 8 < 4 {
        combine(&group_4, &neighbor_4)
    } else {
        combine(&neighbor_4, &group_4)
    };
    if branch < 8 {
        Ok(combine(&group_8, &neighbor_8))
    } else {
        Ok(combine(&neighbor_8, &group_8))
    }
}

fn sparse_merkle_16(me: usize, me_hash: &[u8; 32], neighbor: usize, neighbor_hash: &[u8; 32]) -> [u8; 32] {
    let mut roots = [EMPTY_ROOT; 16];
    roots[me] = *me_hash;
    roots[neighbor] = *neighbor_hash;
    merkle_16_roots(&roots)
}

fn branch_neighbors(branch: usize, roots: &[[u8; 32]; 16]) -> Vec<u8> {
    let neighbor_8 = if branch < 8 {
        merkle_8_roots(&roots[8..16])
    } else {
        merkle_8_roots(&roots[0..8])
    };
    let group_8_base = branch / 8 * 8;
    let neighbor_4_base = group_8_base + if branch % 8 < 4 { 4 } else { 0 };
    let neighbor_4 = merkle_4_roots(&roots[neighbor_4_base..neighbor_4_base + 4]);
    let group_4_base = branch / 4 * 4;
    let neighbor_2_base = group_4_base + if branch % 4 < 2 { 2 } else { 0 };
    let neighbor_2 = merkle_2_roots(&roots[neighbor_2_base..neighbor_2_base + 2]);
    let neighbor_1 = roots[branch ^ 1];
    [neighbor_8, neighbor_4, neighbor_2, neighbor_1].concat()
}

fn merkle_16_roots(roots: &[[u8; 32]; 16]) -> [u8; 32] {
    combine(&merkle_8_roots(&roots[0..8]), &merkle_8_roots(&roots[8..16]))
}

fn merkle_8_roots(roots: &[[u8; 32]]) -> [u8; 32] {
    combine(&merkle_4_roots(&roots[0..4]), &merkle_4_roots(&roots[4..8]))
}

fn merkle_4_roots(roots: &[[u8; 32]]) -> [u8; 32] {
    combine(&merkle_2_roots(&roots[0..2]), &merkle_2_roots(&roots[2..4]))
}

fn merkle_2_roots(roots: &[[u8; 32]]) -> [u8; 32] {
    combine(&roots[0], &roots[1])
}

fn suffix(path: &[u8; 32], cursor: usize) -> Result<Vec<u8>, MpfError> {
    ensure_cursor(cursor)?;
    if cursor % 2 == 0 {
        let mut suffix = Vec::with_capacity(1 + 32 - cursor / 2);
        suffix.push(0xff);
        suffix.extend_from_slice(&path[cursor / 2..]);
        Ok(suffix)
    } else {
        let mut suffix = Vec::with_capacity(2 + 32 - (cursor + 1) / 2);
        suffix.push(0);
        suffix.push(nibble(path, cursor)? as u8);
        suffix.extend_from_slice(&path[(cursor + 1) / 2..]);
        Ok(suffix)
    }
}

fn nibbles(path: &[u8; 32], start: usize, end: usize) -> Result<Vec<u8>, MpfError> {
    if start > end || end > 64 {
        return Err(MpfError::PathExhausted);
    }
    (start..end).map(|ix| nibble(path, ix).map(|n| n as u8)).collect()
}

fn nibble(path: &[u8; 32], index: usize) -> Result<usize, MpfError> {
    if index >= 64 {
        return Err(MpfError::PathExhausted);
    }
    let byte = path[index / 2];
    if index % 2 == 0 {
        Ok(usize::from(byte / 16))
    } else {
        Ok(usize::from(byte % 16))
    }
}

fn ensure_cursor(cursor: usize) -> Result<(), MpfError> {
    if cursor > 64 {
        Err(MpfError::PathExhausted)
    } else {
        Ok(())
    }
}

fn combine(left: &[u8], right: &[u8]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(left.len() + right.len());
    bytes.extend_from_slice(left);
    bytes.extend_from_slice(right);
    hash(bytes)
}

fn hash(bytes: impl AsRef<[u8]>) -> [u8; 32] {
    cml_crypto::blake2b256(bytes.as_ref())
}

#[cfg(test)]
pub(crate) fn hex32(hex_value: &str) -> [u8; 32] {
    hex::decode(hex_value).unwrap().try_into().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_root_is_aiken_empty_root() {
        assert_eq!(EMPTY_ROOT, [0u8; 32]);
    }

    #[test]
    fn rejects_malformed_branch_neighbor_lengths() {
        let proof = vec![ProofStep::Branch {
            skip: 0,
            neighbors: vec![0; 127],
        }];
        assert!(matches!(
            insert(EMPTY_ROOT, b"k", b"v", &proof),
            Err(MpfError::InvalidBranchNeighbors { .. })
        ));
    }

    #[test]
    fn bitcoin_845602_insert_matches_aiken_fixture() {
        let old_root = hex32("225a4599b804ba53745538c83bfa699ecf8077201b61484c91171f5910a4a8f9");
        let new_root = hex32("507c03bc4a25fd1cac2b03592befa4225c5f3488022affa0ab059ca350de2353");
        let key = hex::decode("0000000000000000000261a131bf48cc5a19658ade8cfede99dc1c3933300d60").unwrap();
        let value = hex::decode("26f711634eb26999169bb927f629870938bb4b6b4d1a078b44a6b4ec54f9e8df").unwrap();
        let proof = proof_bitcoin_845602();

        assert!(miss(old_root, &key, &proof));
        assert_eq!(insert(old_root, &key, &value, &proof).unwrap(), new_root);
    }

    #[test]
    fn upstream_fixture_roots_match_rust_transitions() {
        for fixture in [
            include_str!("../resources/mpf-fixtures/upstream/bitcoin-845602.json"),
            include_str!("../resources/mpf-fixtures/upstream/small-branch.json"),
            include_str!("../resources/mpf-fixtures/upstream/leaf-exclusion.json"),
            include_str!("../resources/mpf-fixtures/upstream/fork-compression.json"),
            include_str!("../resources/mpf-fixtures/upstream/delete-collapse.json"),
        ] {
            assert_upstream_fixture(fixture);
        }
    }

    #[test]
    fn empty_trie_insert_generates_valid_exclusion_proof() {
        let mut trie = Trie::empty();
        let proof = trie.proof_for_missing(b"k").unwrap();
        let new_root = insert(trie.root(), b"k", b"v", &proof).unwrap();

        trie.insert(b"k".to_vec(), b"v".to_vec()).unwrap();
        assert_eq!(trie.root(), new_root);
        assert!(has(
            trie.root(),
            b"k",
            b"v",
            &trie.proof_for_present(b"k").unwrap()
        ));
    }

    #[test]
    fn second_insert_generated_proof_verifies_against_current_root() {
        let mut trie = Trie::empty();
        trie.insert(b"k1".to_vec(), b"v1".to_vec()).unwrap();

        let proof = trie.proof_for_missing(b"k2").unwrap();
        let new_root = insert(trie.root(), b"k2", b"v2", &proof).unwrap();

        trie.insert(b"k2".to_vec(), b"v2".to_vec()).unwrap();
        assert_eq!(trie.root(), new_root);
    }

    #[test]
    fn generated_update_proof_transitions_root() {
        let mut trie = Trie::empty();
        trie.insert(b"k1".to_vec(), b"v1".to_vec()).unwrap();
        let old_root = trie.root();
        let proof = trie.proof_for_present(b"k1").unwrap();

        let expected = update(old_root, b"k1", &proof, b"v1", b"v2").unwrap();
        trie.update(b"k1", b"v2".to_vec()).unwrap();

        assert_eq!(trie.root(), expected);
    }

    #[test]
    fn generated_delete_proof_transitions_root() {
        let mut trie = Trie::empty();
        trie.insert(b"k1".to_vec(), b"v1".to_vec()).unwrap();
        trie.insert(b"k2".to_vec(), b"v2".to_vec()).unwrap();
        let old_root = trie.root();
        let proof = trie.proof_for_present(b"k1").unwrap();

        let expected = delete(old_root, b"k1", b"v1", &proof).unwrap();
        trie.delete(b"k1").unwrap();

        assert_eq!(trie.root(), expected);
    }

    #[test]
    fn generated_fork_proof_transitions_root() {
        let mut trie = Trie::empty();
        for ix in [0u8, 66, 86] {
            trie.insert(vec![ix], vec![ix.wrapping_add(1)]).unwrap();
        }

        let proof = trie.proof_for_present(&[0]).unwrap();
        assert!(proof.iter().any(|step| matches!(step, ProofStep::Fork { .. })));
        let old_root = trie.root();
        assert!(has(old_root, &[0], &[1], &proof));
        let new_root = update(old_root, &[0], &proof, &[1], b"updated").unwrap();
        trie.update(&[0], b"updated".to_vec()).unwrap();

        assert_eq!(trie.root(), new_root);
    }

    #[test]
    fn delete_singleton_returns_empty_root() {
        let mut trie = Trie::empty();
        trie.insert(b"k1".to_vec(), b"v1".to_vec()).unwrap();
        let proof = trie.proof_for_present(b"k1").unwrap();
        let old_root = trie.root();

        assert_eq!(delete(old_root, b"k1", b"v1", &proof).unwrap(), EMPTY_ROOT);
    }

    #[test]
    fn rejects_wrong_old_root() {
        let proof = vec![];
        assert_eq!(insert([1; 32], b"k", b"v", &proof), Err(MpfError::InvalidProof));
    }

    #[test]
    fn trie_rejects_duplicate_insert_and_missing_mutations() {
        let mut trie = Trie::empty();
        trie.insert(b"k".to_vec(), b"v".to_vec()).unwrap();

        assert_eq!(
            trie.insert(b"k".to_vec(), b"v2".to_vec()),
            Err(MpfError::ExistingKey)
        );
        assert_eq!(trie.update(b"missing", b"v".to_vec()), Err(MpfError::MissingKey));
        assert_eq!(trie.delete(b"missing"), Err(MpfError::MissingKey));
    }

    #[test]
    fn rejects_malformed_skip_and_fork_nibble() {
        assert_eq!(
            insert(
                EMPTY_ROOT,
                b"k",
                b"v",
                &[ProofStep::Branch {
                    skip: 65,
                    neighbors: vec![0; 128],
                }],
            ),
            Err(MpfError::PathExhausted)
        );

        assert_eq!(
            miss(
                EMPTY_ROOT,
                b"k",
                &[ProofStep::Fork {
                    skip: 0,
                    neighbor: Neighbor {
                        nibble: 16,
                        prefix: vec![],
                        root: EMPTY_ROOT,
                    },
                }],
            ),
            false
        );
        assert_eq!(
            excluding(
                &hash(b"k"),
                0,
                &[ProofStep::Fork {
                    skip: 0,
                    neighbor: Neighbor {
                        nibble: 16,
                        prefix: vec![],
                        root: EMPTY_ROOT,
                    },
                }],
            ),
            Err(MpfError::InvalidProof)
        );
    }

    #[test]
    fn has_rejects_wrong_value() {
        let mut trie = Trie::empty();
        trie.insert(b"k".to_vec(), b"v".to_vec()).unwrap();
        let proof = trie.proof_for_present(b"k").unwrap();

        assert!(!has(trie.root(), b"k", b"wrong", &proof));
    }

    fn proof_bitcoin_845602() -> Vec<ProofStep> {
        [
            "bc13df27a19f8caf0bf922c900424025282a892ba8577095fd35256c9d553ca120b8645121ebc9057f7b28fa4c0032b1f49e616dfb8dbd88e4bffd7c0844d29b011b1af0993ac88158342583053094590c66847acd7890c86f6de0fde0f7ae2479eafca17f9659f252fa13ee353c879373a65ca371093525cf359fae1704cf4a",
            "255753863960985679b4e752d4b133322ff567d210ffbb10ee56e51177db057460b547fe42c6f44dfef8b3ecee35dfd4aa105d28b94778a3f1bb8211cf2679d7434b40848aebdd6565b59efdc781ffb5ca8a9f2b29f95a47d0bf01a09c38fa39359515ddb9d2d37a26bccb022968ef4c8e29a95c7c82edcbe561332ff79a51af",
            "9d95e34e6f74b59d4ea69943d2759c01fe9f986ff0c03c9e25ab561b23a413b77792fa78d9fbcb98922a4eed2df0ed70a2852ae8dbac8cff54b9024f229e66629136cfa60a569c464503a8b7779cb4a632ae052521750212848d1cc0ebed406e1ba4876c4fd168988c8fe9e226ed283f4d5f17134e811c3b5322bc9c494a598b",
            "b93c3b90e647f90beb9608aecf714e3fbafdb7f852cfebdbd8ff435df84a4116d10ccdbe4ea303efbf0f42f45d8dc4698c3890595be97e4b0f39001bde3f2ad95b8f6f450b1e85d00dacbd732b0c5bc3e8c92fc13d43028777decb669060558821db21a9b01ba5ddf6932708cd96d45d41a1a4211412a46fe41870968389ec96",
            "f89f9d06b48ecc0e1ea2e6a43a9047e1ff02ecf9f79b357091ffc0a7104bbb260908746f8e61ecc60dfe26b8d03bcc2f1318a2a95fa895e4d1aadbb917f9f2936b900c75ffe49081c265df9c7c329b9036a0efb46d5bac595a1dcb7c200e7d590000000000000000000000000000000000000000000000000000000000000000",
        ]
        .into_iter()
        .map(|neighbors| ProofStep::Branch {
            skip: 0,
            neighbors: hex::decode(neighbors).unwrap(),
        })
            .collect()
    }

    fn assert_upstream_fixture(json: &str) {
        let fixture: serde_json::Value = serde_json::from_str(json).unwrap();
        let proof = parse_proof(fixture["proof"].as_array().unwrap());
        let key = decode_field(&fixture, "key");
        match fixture["operation"].as_str().unwrap() {
            "has" => {
                let root = decode_root(&fixture, "root");
                let value = decode_field(&fixture, "value");
                assert!(has(root, &key, &value, &proof), "{}", fixture["name"]);
            }
            "insert" => {
                let old_root = decode_root(&fixture, "old_root");
                let new_root = decode_root(&fixture, "new_root");
                let value = decode_field(&fixture, "value");
                assert!(miss(old_root, &key, &proof), "{}", fixture["name"]);
                assert_eq!(
                    insert(old_root, &key, &value, &proof).unwrap(),
                    new_root,
                    "{}",
                    fixture["name"]
                );
            }
            "update" => {
                let old_root = decode_root(&fixture, "old_root");
                let new_root = decode_root(&fixture, "new_root");
                let old_value = decode_field(&fixture, "old_value");
                let new_value = decode_field(&fixture, "new_value");
                assert_eq!(
                    update(old_root, &key, &proof, &old_value, &new_value).unwrap(),
                    new_root,
                    "{}",
                    fixture["name"]
                );
            }
            "delete" => {
                let old_root = decode_root(&fixture, "old_root");
                let new_root = decode_root(&fixture, "new_root");
                let value = decode_field(&fixture, "value");
                assert_eq!(
                    delete(old_root, &key, &value, &proof).unwrap(),
                    new_root,
                    "{}",
                    fixture["name"]
                );
            }
            operation => panic!("unknown fixture operation {operation}"),
        }
    }

    fn parse_proof(steps: &[serde_json::Value]) -> Vec<ProofStep> {
        steps.iter().map(parse_step).collect()
    }

    fn parse_step(step: &serde_json::Value) -> ProofStep {
        let skip = step["skip"].as_u64().unwrap() as usize;
        match step["type"].as_str().unwrap() {
            "branch" => ProofStep::Branch {
                skip,
                neighbors: hex::decode(step["neighbors"].as_str().unwrap()).unwrap(),
            },
            "fork" => {
                let neighbor = &step["neighbor"];
                ProofStep::Fork {
                    skip,
                    neighbor: Neighbor {
                        nibble: neighbor["nibble"].as_u64().unwrap() as u8,
                        prefix: hex::decode(neighbor["prefix"].as_str().unwrap()).unwrap(),
                        root: hex32(neighbor["root"].as_str().unwrap()),
                    },
                }
            }
            "leaf" => {
                let neighbor = step.get("neighbor").unwrap_or(step);
                ProofStep::Leaf {
                    skip,
                    key: hex32(neighbor["key"].as_str().unwrap()),
                    value: hex32(neighbor["value"].as_str().unwrap()),
                }
            }
            step_type => panic!("unknown fixture proof step {step_type}"),
        }
    }

    fn decode_root(fixture: &serde_json::Value, field: &str) -> [u8; 32] {
        hex32(fixture[field].as_str().unwrap())
    }

    fn decode_field(fixture: &serde_json::Value, field: &str) -> Vec<u8> {
        hex::decode(fixture[field].as_str().unwrap()).unwrap()
    }
}
