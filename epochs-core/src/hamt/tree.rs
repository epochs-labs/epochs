//! Persistent HAMT operations with path copying.

use crate::cas::{CasBackend, RecordType};
use crate::error::Result;
use crate::hamt::node::HamtNode;
use crate::hash::Hash;

/// Diff operation between two HAMT roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffOp {
    /// Key added in the new root.
    Added,
    /// Key removed in the new root.
    Removed,
    /// Key value changed.
    Changed,
}

/// Persistent hash-array mapped trie over a CAS backend.
pub struct PersistentHamt;

impl PersistentHamt {
    /// Insert or update `key` with `value`, returning the new root hash.
    pub fn insert(
        cas: &mut impl CasBackend,
        root: Option<Hash>,
        key: &[u8],
        value: &[u8],
    ) -> Result<Hash> {
        let key_hash = Hash::of_bytes(key);
        Self::insert_internal(cas, root, key, value, &key_hash, 0)
    }

    /// Look up `key` under `root`.
    pub fn get(
        cas: &mut impl CasBackend,
        root: Option<Hash>,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let Some(root_hash) = root else {
            return Ok(None);
        };
        let key_hash = Hash::of_bytes(key);
        Self::get_internal(cas, root_hash, key, &key_hash, 0)
    }

    /// Return the CAS hash of the leaf node storing `key`, if present.
    pub fn leaf_hash(
        cas: &mut impl CasBackend,
        root: Option<Hash>,
        key: &[u8],
    ) -> Result<Option<Hash>> {
        let Some(root_hash) = root else {
            return Ok(None);
        };
        let key_hash = Hash::of_bytes(key);
        Self::leaf_hash_internal(cas, root_hash, key, &key_hash, 0)
    }

    /// Collect all key-value pairs under `root` (depth-first).
    pub fn entries(
        cas: &mut impl CasBackend,
        root: Option<Hash>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let Some(root_hash) = root else {
            return Ok(vec![]);
        };
        let mut out = Vec::new();
        Self::collect_entries(cas, root_hash, &mut out)?;
        Ok(out)
    }

    /// Remove `key` from the tree, returning the new root (or `None` if empty).
    pub fn remove(
        cas: &mut impl CasBackend,
        root: Option<Hash>,
        key: &[u8],
    ) -> Result<Option<Hash>> {
        let Some(root_hash) = root else {
            return Ok(None);
        };
        let key_hash = Hash::of_bytes(key);
        Self::remove_internal(cas, root_hash, key, &key_hash, 0)
    }

    fn remove_internal(
        cas: &mut impl CasBackend,
        node_hash: Hash,
        key: &[u8],
        key_hash: &Hash,
        depth: usize,
    ) -> Result<Option<Hash>> {
        match Self::load_node(cas, node_hash)? {
            HamtNode::Leaf { key: k, .. } => {
                if k == key {
                    Ok(None)
                } else {
                    Ok(Some(node_hash))
                }
            }
            HamtNode::Bitmap { bitmap, children } => {
                let branch = Self::extract_5bits(key_hash, depth);
                let bit_mask = 1u32 << branch;
                if (bitmap & bit_mask) == 0 {
                    return Ok(Some(node_hash));
                }
                let idx = Self::child_index(bitmap, branch);
                let child = children[idx];
                match Self::remove_internal(cas, child, key, key_hash, depth + 1)? {
                    None => {
                        let mut new_children = children;
                        new_children.remove(idx);
                        let new_bitmap = bitmap & !bit_mask;
                        if new_children.is_empty() {
                            Ok(None)
                        } else if new_children.len() == 1 {
                            // Keep bitmap wrapper for stable structure (simpler than collapsing)
                            Self::persist_bitmap(cas, new_bitmap, new_children).map(Some)
                        } else {
                            Self::persist_bitmap(cas, new_bitmap, new_children).map(Some)
                        }
                    }
                    Some(new_child) => {
                        let mut new_children = children;
                        new_children[idx] = new_child;
                        Self::persist_bitmap(cas, bitmap, new_children).map(Some)
                    }
                }
            }
        }
    }

    fn collect_entries(
        cas: &mut impl CasBackend,
        node_hash: Hash,
        out: &mut Vec<(Vec<u8>, Vec<u8>)>,
    ) -> Result<()> {
        match Self::load_node(cas, node_hash)? {
            HamtNode::Leaf { key, value } => {
                out.push((key, value));
            }
            HamtNode::Bitmap { children, .. } => {
                for child in children {
                    Self::collect_entries(cas, child, out)?;
                }
            }
        }
        Ok(())
    }

    fn extract_5bits(key_hash: &Hash, depth: usize) -> u8 {
        let bit_idx = depth * 5;
        let byte_idx = bit_idx / 8;
        let bit_offset = bit_idx % 8;

        if byte_idx >= 31 {
            return (key_hash.as_bytes()[31] >> (bit_offset % 8)) & 0x1F;
        }

        let b0 = key_hash.as_bytes()[byte_idx] as u16;
        let b1 = key_hash.as_bytes()[byte_idx + 1] as u16;
        let combined = (b0 | (b1 << 8)) >> bit_offset;
        (combined & 0x1F) as u8
    }

    fn child_index(bitmap: u32, branch: u8) -> usize {
        let mask = 1u32 << branch;
        (bitmap & (mask - 1)).count_ones() as usize
    }

    fn make_leaf(key: &[u8], value: &[u8]) -> HamtNode {
        HamtNode::Leaf {
            key: key.to_vec(),
            value: value.to_vec(),
        }
    }

    fn persist_leaf(cas: &mut impl CasBackend, key: &[u8], value: &[u8]) -> Result<Hash> {
        let node = Self::make_leaf(key, value);
        cas.put(RecordType::HamtLeaf, &node.encode())
    }

    fn persist_bitmap(cas: &mut impl CasBackend, bitmap: u32, children: Vec<Hash>) -> Result<Hash> {
        let node = HamtNode::Bitmap { bitmap, children };
        cas.put(RecordType::HamtBitmap, &node.encode())
    }

    fn load_node(cas: &mut impl CasBackend, hash: Hash) -> Result<HamtNode> {
        let (record_type, payload) = cas.get_record(&hash)?;
        HamtNode::decode(record_type, &payload)
    }

    fn insert_internal(
        cas: &mut impl CasBackend,
        node_hash: Option<Hash>,
        key: &[u8],
        value: &[u8],
        key_hash: &Hash,
        depth: usize,
    ) -> Result<Hash> {
        match node_hash {
            None => Self::persist_leaf(cas, key, value),
            Some(hash) => {
                let node = Self::load_node(cas, hash)?;
                match node {
                    HamtNode::Leaf {
                        key: existing_key,
                        value: existing_val,
                    } => {
                        if existing_key == key {
                            return Self::persist_leaf(cas, key, value);
                        }
                        let existing_key_hash = Hash::of_bytes(&existing_key);
                        Self::expand_leaf_collision(
                            cas,
                            &existing_key,
                            &existing_val,
                            &existing_key_hash,
                            key,
                            value,
                            key_hash,
                            depth,
                        )
                    }
                    HamtNode::Bitmap { bitmap, children } => {
                        let branch = Self::extract_5bits(key_hash, depth);
                        let bit_mask = 1u32 << branch;
                        let exists = (bitmap & bit_mask) != 0;

                        if exists {
                            let idx = Self::child_index(bitmap, branch);
                            let child_hash = children[idx];
                            let new_child = Self::insert_internal(
                                cas,
                                Some(child_hash),
                                key,
                                value,
                                key_hash,
                                depth + 1,
                            )?;
                            let mut new_children = children;
                            new_children[idx] = new_child;
                            Self::persist_bitmap(cas, bitmap, new_children)
                        } else {
                            let new_leaf = Self::persist_leaf(cas, key, value)?;
                            let mut new_children = children;
                            let idx = Self::child_index(bitmap, branch);
                            new_children.insert(idx, new_leaf);
                            Self::persist_bitmap(cas, bitmap | bit_mask, new_children)
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn expand_leaf_collision(
        cas: &mut impl CasBackend,
        existing_key: &[u8],
        existing_val: &[u8],
        existing_key_hash: &Hash,
        new_key: &[u8],
        new_val: &[u8],
        new_key_hash: &Hash,
        depth: usize,
    ) -> Result<Hash> {
        let chunk_old = Self::extract_5bits(existing_key_hash, depth);
        let chunk_new = Self::extract_5bits(new_key_hash, depth);

        if chunk_old == chunk_new {
            let old_leaf_hash = Self::persist_leaf(cas, existing_key, existing_val)?;
            return Self::insert_internal(
                cas,
                Some(old_leaf_hash),
                new_key,
                new_val,
                new_key_hash,
                depth + 1,
            );
        }

        let old_leaf_hash = Self::persist_leaf(cas, existing_key, existing_val)?;
        let new_leaf_hash = Self::persist_leaf(cas, new_key, new_val)?;

        let bitmap = (1u32 << chunk_old) | (1u32 << chunk_new);
        let children = if chunk_old < chunk_new {
            vec![old_leaf_hash, new_leaf_hash]
        } else {
            vec![new_leaf_hash, old_leaf_hash]
        };
        Self::persist_bitmap(cas, bitmap, children)
    }

    fn get_internal(
        cas: &mut impl CasBackend,
        node_hash: Hash,
        key: &[u8],
        key_hash: &Hash,
        depth: usize,
    ) -> Result<Option<Vec<u8>>> {
        match Self::load_node(cas, node_hash)? {
            HamtNode::Leaf { key: k, value: v } => {
                if k == key {
                    Ok(Some(v))
                } else {
                    Ok(None)
                }
            }
            HamtNode::Bitmap { bitmap, children } => {
                let branch = Self::extract_5bits(key_hash, depth);
                let bit_mask = 1u32 << branch;
                if (bitmap & bit_mask) == 0 {
                    return Ok(None);
                }
                let idx = Self::child_index(bitmap, branch);
                Self::get_internal(cas, children[idx], key, key_hash, depth + 1)
            }
        }
    }

    fn leaf_hash_internal(
        cas: &mut impl CasBackend,
        node_hash: Hash,
        key: &[u8],
        key_hash: &Hash,
        depth: usize,
    ) -> Result<Option<Hash>> {
        match Self::load_node(cas, node_hash)? {
            HamtNode::Leaf { key: k, .. } => {
                if k == key {
                    Ok(Some(node_hash))
                } else {
                    Ok(None)
                }
            }
            HamtNode::Bitmap { bitmap, children } => {
                let branch = Self::extract_5bits(key_hash, depth);
                let bit_mask = 1u32 << branch;
                if (bitmap & bit_mask) == 0 {
                    return Ok(None);
                }
                let idx = Self::child_index(bitmap, branch);
                Self::leaf_hash_internal(cas, children[idx], key, key_hash, depth + 1)
            }
        }
    }
}
