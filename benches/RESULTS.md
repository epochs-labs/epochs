# Results — deep history (bounded keys)

## Fair Docker smoke (1 000 keys × 10 000 commits, 2 CPU / 2 GiB)

`./benches/run.sh smoke`

| engine | commit/s | W1 p50 | R1 p50 | R2 p50 | disk | mem |
|--------|----------|--------|--------|--------|------|-----|
| **epochs** | **28 543** | **25µs** | **35µs** | **4.6ms** | 23.5 MiB | 41 MiB |
| sqlite | 12 026 | 32µs | 113µs | 24.4ms | 3.3 MiB | 22 MiB |
| postgres | 295 | 2.6ms | 7.7ms | 486ms | 4.5 MiB | ~1.5 GiB |
| mysql* | 415 | 2.2ms | 0.9ms | 347ms | 4.2 MiB | ~1.5 GiB |

\* MariaDB (MySQL protocol).

## Scale story (1 000 keys, growing history — host embedded)

| commits | epochs R2 | sqlite R2 | epochs R1 | sqlite R1 |
|---------|-----------|-----------|-----------|-----------|
| 10 000 | 15.9 ms | 28.6 ms | 36 µs | 133 µs |
| 50 000 | **3.6 ms** | 142 ms | 34 µs | 136 µs |
| 100 000 | **3.0 ms** | 292 ms | 36 µs | 130 µs |
| 250 000 | **4.1 ms** | **771 ms** | 36 µs | 132 µs |

Tip checkout stays ~ms for epochs (HAMT of 1k keys); SQLite delta-replay grows with history depth.

Charts: [`benches/charts/`](charts/) · `python3 benches/charts/render.py --csv benches/out/ladder.csv --out benches/charts`
