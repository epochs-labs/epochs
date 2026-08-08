//! Thread-local [`ByteEncoder`] pool to cut encode-path allocations.

use std::cell::RefCell;

use crate::codec::encoder::ByteEncoder;

thread_local! {
    static ENCODER_POOL: RefCell<Vec<ByteEncoder>> = const { RefCell::new(Vec::new()) };
}

const POOL_CAP: usize = 32;
const ENC_BUF_HINT: usize = 256;

/// Take a pooled encoder (cleared, ready to write).
pub fn take_encoder() -> ByteEncoder {
    ENCODER_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        let mut enc = pool
            .pop()
            .unwrap_or_else(|| ByteEncoder::with_capacity(ENC_BUF_HINT));
        enc.clear();
        enc
    })
}

/// Return an encoder to the pool (buffer retained for reuse).
pub fn put_encoder(mut enc: ByteEncoder) {
    enc.clear();
    ENCODER_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < POOL_CAP {
            pool.push(enc);
        }
    });
}

/// Run `f` with a pooled encoder and return the finished bytes.
pub fn encode_with(f: impl FnOnce(&mut ByteEncoder)) -> Vec<u8> {
    let mut enc = take_encoder();
    f(&mut enc);
    let out = std::mem::take(&mut enc.buf);
    put_encoder(enc);
    out
}
