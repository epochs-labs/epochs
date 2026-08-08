# Proly Trees (Phase 2 Design)

Proly Trees combine B-Tree range-query capability with content-defined chunking.
This document freezes the v2 extension points so the v1 HAMT + CAS layout remains compatible.

## Goals

- Range scans over tabular agent conversation logs (turn index, timestamps)
- Content-defined chunk boundaries (Gear hash rolling fingerprint)
- Identical sub-tree hashes across branches that share rows (structural deduplication)

## Record Type 4: Proly Chunk Node

```
GEAR_STATE u64 LE          // rolling hash state at chunk start
CHUNK_LEN u32 LE
CHUNK_HASH [32]            // BLAKE3(chunk bytes)
CHILD_LEFT [32]            // optional child hash (ZERO if absent)
CHILD_RIGHT [32]           // optional child hash (ZERO if absent)
ROW_COUNT u32 LE
```

## Commit Payload v2

Commit payload v1 begins with `VERSION u8 = 1`. v2 adds optional Proly root:

```
VERSION u8 = 2
PARENT_COUNT u8
...
ROOT_HAMT [32]
ROOT_PROLY [32]             // ZERO if unused
TIMESTAMP u64 LE
MSG_LEN u16 LE
MSG_BYTES
```

v1 commits keep `ROOT_PROLY` absent; decoders use VERSION to select layout.

## Implementation Notes

- Gear hash: pure Rust, 64-bit accumulator, boundary when `(acc & (1 << 13)) == 0`
- Proly sits beside HAMT: HAMT for key-value agent memory, Proly for append-only turn logs
- Diff across branches walks matching chunk hashes before deep comparison
