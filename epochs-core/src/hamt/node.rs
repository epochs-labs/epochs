//! HAMT node payload codecs (no record type prefix).

use crate::codec::ByteDecoder;
use crate::error::Result;
use crate::hash::Hash;

/// HAMT node variants stored as CAS payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HamtNode {
    /// Internal bitmap node with child hash references.
    Bitmap {
        /// Presence bitmask for 32 branches.
        bitmap: u32,
        /// Child node hashes in ascending bit order.
        children: Vec<Hash>,
    },
    /// Terminal key-value leaf.
    Leaf {
        /// Key bytes.
        key: Vec<u8>,
        /// Value bytes.
        value: Vec<u8>,
    },
}

impl HamtNode {
    /// Encode node to canonical payload bytes (uses the thread-local encoder pool).
    pub fn encode(&self) -> Vec<u8> {
        crate::codec::encode_with(|enc| match self {
            Self::Bitmap { bitmap, children } => {
                enc.write_u32(*bitmap);
                enc.write_u8(children.len() as u8);
                for child in children {
                    enc.write_hash(child);
                }
            }
            Self::Leaf { key, value } => {
                enc.write_u16(key.len() as u16);
                enc.write_bytes(key);
                enc.write_u32(value.len() as u32);
                enc.write_bytes(value);
            }
        })
    }

    /// Decode node from payload bytes, using `record_type` to disambiguate layout.
    pub fn decode(record_type: crate::cas::RecordType, buf: &[u8]) -> Result<Self> {
        let mut dec = ByteDecoder::new(buf);
        match record_type {
            crate::cas::RecordType::HamtBitmap => {
                let bitmap = dec.read_u32()?;
                let child_count = dec.read_u8()? as usize;
                let mut children = Vec::with_capacity(child_count);
                for _ in 0..child_count {
                    children.push(dec.read_hash()?);
                }
                Ok(Self::Bitmap { bitmap, children })
            }
            crate::cas::RecordType::HamtLeaf => {
                let klen = dec.read_u16()? as usize;
                let key = dec.read_slice(klen)?.to_vec();
                let vlen = dec.read_u32()? as usize;
                let value = dec.read_slice(vlen)?.to_vec();
                Ok(Self::Leaf { key, value })
            }
            _ => Err(crate::error::EpochsError::Codec(
                "not a HAMT node record".into(),
            )),
        }
    }
}
