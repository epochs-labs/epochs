# epochs-bench — versioned KV / commit DAG

Default workload is **deep history**: a **fixed live key set** updated over many
commits (git-like). That is what agent/VCS databases look like in practice —
not “one brand-new key per commit forever.”

## What we compare

| Metric | Why |
|--------|-----|
| **W1 commit/s + p50/p99** | Write / ingest path as history grows |
| **R1 history p50** | Fixed-depth ancestor walk — should stay **flat** in N |
| **R2 tip checkout p50** | epochs: HAMT of **#keys**; SQL: **replay all deltas** → grows with N |
| **W2 branch** | O(1) tip pointer |
| **Disk / cgroup mem** | Retention cost; RSS should stay bounded |

Optional `--shape wide` = unique key per commit (stress only).

## Fair environment

All engines: **2 CPUs + 2 GiB**, sequential, SQL server+client in one cgroup.

```bash
./benches/run.sh smoke                 # 1k keys × 10k commits, all engines
./benches/run-ladder.sh --quick        # smoke + dev
./benches/run-ladder.sh                # + mid (+ large embedded)
SHAPE=deep ./benches/run.sh mid
```

```bash
python3 benches/charts/render.py --csv benches/out/ladder.csv --out benches/charts
```

## Scale charts (deep shape)

![Commit throughput](charts/commit_throughput.svg)

![W1 latency](charts/w1_latency.svg)

![R1 history — flat](charts/r1_history.svg)

![R2 checkout — tip](charts/r2_checkout.svg)

![Disk](charts/disk.svg)

![Memory](charts/memory.svg)

## Tiers (deep defaults)

| Tier | Live keys | Commits | Notes |
|------|-----------|---------|--------|
| `smoke` | 1 000 | 10 000 | CI |
| `dev` | 10 000 | 100 000 | short ladder |
| `mid` | 10 000 | 1 000 000 | SQL tip-replay story |
| `large` | 100 000 | 5 000 000 | embedded |
| `heavy` | 100 000 | 50 000 000 | `--force` |

Overrides: `--keys`, `--commits`, `--shape deep|wide`.

## SQL peer schema

```text
branches(name PK, tip_id)
commits(id, parent_id, message, ts)
commit_ops(commit_id, key, value)   -- checkout = replay chain
```

## Matched knobs

- epochs: `fsync_every=512`, CAS LRU 1024, commit LRU 256, mmap ≤2
- sqlite: WAL + NORMAL, ~8 MiB cache
- postgres: `shared_buffers=64MB`
- mysql/MariaDB: `innodb_buffer_pool_size=64M`

See [RESULTS.md](RESULTS.md).
