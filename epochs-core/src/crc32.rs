//! IEEE 802.3 CRC32 (pure Rust, no external crate).

const TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut c = i;
        let mut j = 0;
        while j < 8 {
            if c & 1 != 0 {
                c = 0xEDB8_8320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            j += 1;
        }
        table[i as usize] = c;
        i += 1;
    }
    table
};

/// Compute IEEE CRC32 over `data`.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF;
    for byte in data {
        let idx = ((crc ^ u32::from(*byte)) & 0xFF) as usize;
        crc = TABLE[idx] ^ (crc >> 8);
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
