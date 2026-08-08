//! `.epl` record format definitions.

use crate::codec::{ByteDecoder, ByteEncoder};
use crate::crc32;
use crate::error::{EpochsError, Result, StorageError};
use crate::hash::Hash;

/// Magic bytes for `.epl` records: `EPCL`.
pub const RECORD_MAGIC: [u8; 4] = [0x45, 0x50, 0x43, 0x4C];

/// Magic bytes for segment headers: `EPSG`.
pub const SEGMENT_MAGIC: [u8; 4] = [0x45, 0x50, 0x53, 0x47];

/// Size of a segment header in bytes.
pub const SEGMENT_HEADER_LEN: u64 = 16;

/// Object type stored in the append-only log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RecordType {
    /// Commit snapshot.
    Commit = 1,
    /// HAMT bitmap node.
    HamtBitmap = 2,
    /// HAMT leaf node.
    HamtLeaf = 3,
}

impl RecordType {
    /// Parse from a raw byte.
    pub fn from_u8(v: u8) -> Result<Self> {
        match v {
            1 => Ok(Self::Commit),
            2 => Ok(Self::HamtBitmap),
            3 => Ok(Self::HamtLeaf),
            _ => Err(EpochsError::Storage(StorageError::Corrupt(format!(
                "unknown record type: {v}"
            )))),
        }
    }
}

/// A decoded `.epl` record (payload only; header verified separately).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Record type.
    pub record_type: RecordType,
    /// Content hash of the payload.
    pub content_hash: Hash,
    /// Canonical payload bytes.
    pub payload: Vec<u8>,
}

/// Encode a full on-disk record (header + payload + CRC).
pub fn encode_record(record_type: RecordType, payload: &[u8], content_hash: Hash) -> Vec<u8> {
    crate::codec::encode_with(|enc| {
        enc.write_bytes(&RECORD_MAGIC);
        enc.write_u8(record_type as u8);
        enc.write_u32(payload.len() as u32);
        enc.write_hash(&content_hash);
        enc.write_bytes(payload);

        let crc = crc32::crc32(&enc.buf[4..]);
        enc.write_u32(crc);
    })
}

/// Decode and verify a full on-disk record from `buf`.
pub fn decode_record(buf: &[u8]) -> Result<Record> {
    if buf.len() < 45 {
        return Err(EpochsError::Storage(StorageError::Corrupt(
            "record too short".into(),
        )));
    }

    let mut dec = ByteDecoder::new(buf);
    let magic = dec.read_slice(4)?;
    if magic != RECORD_MAGIC {
        return Err(EpochsError::Storage(StorageError::Corrupt(
            "invalid record magic".into(),
        )));
    }

    let record_type = RecordType::from_u8(dec.read_u8()?)?;
    let payload_len = dec.read_u32()? as usize;
    let content_hash = dec.read_hash()?;
    let payload = dec.read_slice(payload_len)?.to_vec();
    let stored_crc = dec.read_u32()?;

    let computed_crc = crc32::crc32(&buf[4..4 + 37 + payload_len]);
    if stored_crc != computed_crc {
        return Err(EpochsError::Storage(StorageError::CrcMismatch));
    }

    let computed_hash = Hash::of_bytes(&payload);
    if computed_hash != content_hash {
        return Err(EpochsError::Storage(StorageError::HashMismatch {
            expected: content_hash,
            computed: computed_hash,
        }));
    }

    Ok(Record {
        record_type,
        content_hash,
        payload,
    })
}

/// Encode a 16-byte segment header.
pub fn encode_segment_header(segment_id: u32, created_at: u64) -> [u8; 16] {
    let mut enc = ByteEncoder::new();
    enc.write_bytes(&SEGMENT_MAGIC);
    enc.write_u32(segment_id);
    enc.write_u64(created_at);
    let mut out = [0u8; 16];
    out.copy_from_slice(&enc.buf);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn record_roundtrip() {
        let payload = b"hello world".to_vec();
        let hash = Hash::of_bytes(&payload);
        let encoded = encode_record(RecordType::HamtLeaf, &payload, hash);
        let decoded = decode_record(&encoded).unwrap();
        assert_eq!(decoded.payload, payload);
        assert_eq!(decoded.content_hash, hash);
    }

    #[test]
    fn record_rejects_crc_tamper() {
        let payload = b"hello".to_vec();
        let hash = Hash::of_bytes(&payload);
        let mut encoded = encode_record(RecordType::Commit, &payload, hash);
        let last = encoded.len() - 1;
        encoded[last] ^= 0xFF;
        assert!(decode_record(&encoded).is_err());
    }
}
