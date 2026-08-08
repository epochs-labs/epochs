//! Custom little-endian binary codecs.

mod decoder;
mod encoder;
mod pool;

pub use decoder::ByteDecoder;
pub use encoder::ByteEncoder;
pub use pool::{encode_with, put_encoder, take_encoder};
