//! Versioned-KV workload: **deep history** (bounded live keys) is the default.
//!
//! Real VCS / agent DBs look like git: many commits, a working set of documents
//! that get *updated*. Checkout cost should track **#live keys**, not #commits.
//!
//! Optional `--shape wide` keeps the old stress test (unique key per commit).

use std::collections::BTreeSet;
use std::time::Instant;

use crate::engines::{BenchReport, CommitId, LatencyStats, Put, Shape, VcsBench};
use crate::mem::sample_memory_bytes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Smoke,
    Dev,
    Mid,
    Large,
    Heavy,
}

impl Tier {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "smoke" => Ok(Self::Smoke),
            "dev" => Ok(Self::Dev),
            "mid" => Ok(Self::Mid),
            "large" => Ok(Self::Large),
            "heavy" => Ok(Self::Heavy),
            other => Err(format!(
                "unknown tier '{other}' (smoke|dev|mid|large|heavy)"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Dev => "dev",
            Self::Mid => "mid",
            Self::Large => "large",
            Self::Heavy => "heavy",
        }
    }

    /// Default live key cardinality for **deep** shape.
    pub fn live_keys(self) -> u64 {
        match self {
            Self::Smoke => 1_000,
            Self::Dev => 10_000,
            Self::Mid => 10_000,
            Self::Large => 100_000,
            Self::Heavy => 100_000,
        }
    }

    /// Default commit count (history depth).
    pub fn commits(self) -> u64 {
        match self {
            Self::Smoke => 10_000,
            Self::Dev => 100_000,
            Self::Mid => 1_000_000,
            Self::Large => 5_000_000,
            Self::Heavy => 50_000_000,
        }
    }

    pub fn requires_force(self) -> bool {
        matches!(self, Self::Heavy)
    }
}

const W1_SAMPLE_CAP: u64 = 100_000;

/// Knobs for a single bench run (keeps `run_bench` argument count sane).
#[derive(Clone, Debug)]
pub struct BenchOpts {
    pub tier: Tier,
    pub shape: Shape,
    pub commits_override: Option<u64>,
    pub keys_override: Option<u64>,
    pub payload_bytes: usize,
    pub puts_per_commit: u32,
    pub history_depth: u32,
    pub branch_count: u64,
    pub checkout_samples: u64,
    pub progress_every: u64,
}

/// Run commit → branch → history → checkout against a VCS backend.
pub fn run_bench<S: VcsBench>(mut store: S, opts: &BenchOpts) -> Result<BenchReport, String> {
    let tier = opts.tier;
    let shape = opts.shape;
    let n = opts.commits_override.unwrap_or_else(|| tier.commits());
    if n == 0 {
        return Err("commits must be > 0".into());
    }
    let live_keys = match shape {
        Shape::Deep => opts
            .keys_override
            .unwrap_or_else(|| tier.live_keys())
            .max(1),
        Shape::Wide => opts.keys_override.unwrap_or(n).max(1),
    };
    let puts_n = opts.puts_per_commit.max(1) as u64;
    let payload_base = vec![b'x'; opts.payload_bytes.max(16)];

    let branches = opts.branch_count.max(1);
    let r2_samples = opts.checkout_samples.clamp(1, 20);
    let hist_samples = opts.checkout_samples.clamp(1, 200);
    let progress_every = opts.progress_every;
    let history_depth = opts.history_depth;

    // Sparse tip retention — never store all N ids.
    let mut keep_idx: BTreeSet<u64> = BTreeSet::new();
    keep_idx.insert(n - 1); // tip — always checkout tip for R2 story
    for b in 0..branches {
        keep_idx.insert((b * n / branches).min(n - 1));
    }
    for s in 0..r2_samples.saturating_sub(1) {
        // Mid-history samples (plus tip already kept).
        keep_idx.insert(((s + 1) * n) / (r2_samples + 1));
    }

    let w1_stride = (n / W1_SAMPLE_CAP).max(1);
    let mut w1 = LatencyStats::default();
    let mut kept: Vec<(u64, CommitId)> = Vec::with_capacity(keep_idx.len());
    let mut tip: Option<CommitId> = None;

    let mut key_bufs: Vec<Vec<u8>> = (0..puts_n).map(|_| Vec::with_capacity(32)).collect();
    let mut val_bufs: Vec<Vec<u8>> = (0..puts_n).map(|_| payload_base.clone()).collect();

    let load_start = Instant::now();
    for i in 0..n {
        let mut puts = Vec::with_capacity(puts_n as usize);
        for p in 0..puts_n {
            let key_id = match shape {
                Shape::Deep => (i * puts_n + p) % live_keys,
                Shape::Wide => i * puts_n + p,
            };
            let kb = &mut key_bufs[p as usize];
            kb.clear();
            use std::io::Write;
            let _ = write!(kb, "k/{key_id}");

            let vb = &mut val_bufs[p as usize];
            // Embed commit index so each update is a real write (not identical CAS).
            let tag = i.to_le_bytes();
            vb[..8].copy_from_slice(&tag);

            puts.push(Put {
                key: kb.clone(),
                value: vb.clone(),
            });
        }

        let t0 = Instant::now();
        let id = store.commit(None, &puts, "c")?;
        if i % w1_stride == 0 {
            w1.record(t0.elapsed());
        }
        tip = Some(id.clone());
        if keep_idx.contains(&i) {
            kept.push((i, id));
        }

        if progress_every > 0 && (i + 1) % progress_every == 0 {
            eprint!(
                "\r  [{}] {} keys={} commits {}/{} …",
                store.name(),
                shape.as_str(),
                live_keys,
                i + 1,
                n
            );
        }
    }
    if progress_every > 0 {
        eprintln!();
    }
    let load_secs = load_start.elapsed().as_secs_f64();
    let tip = tip.ok_or("no commits")?;
    w1.set_count_override(n as usize);

    let id_at = |idx: u64| -> Result<&CommitId, String> {
        kept.iter()
            .find(|(i, _)| *i == idx)
            .map(|(_, id)| id)
            .ok_or_else(|| format!("missing sampled commit {idx}"))
    };

    let mut w2 = LatencyStats::default();
    for b in 0..branches {
        let name = format!("b{b}");
        let idx = (b * n / branches).min(n - 1);
        let src = id_at(idx)?;
        let t0 = Instant::now();
        store.branch(&name, src)?;
        w2.record(t0.elapsed());
    }

    let mut r1 = LatencyStats::default();
    for _ in 0..hist_samples {
        let t0 = Instant::now();
        let chain = store.history(&tip, history_depth)?;
        r1.record(t0.elapsed());
        if chain.is_empty() {
            return Err("empty history".into());
        }
    }

    // R2: always include tip; plus mid-history samples.
    let mut r2 = LatencyStats::default();
    let mut r2_idxs: Vec<u64> = keep_idx.iter().copied().collect();
    r2_idxs.sort_unstable();
    // Prefer tip last measurement clarity — measure up to r2_samples including tip.
    if r2_idxs.len() > r2_samples as usize {
        // keep tip + evenly spaced earlier
        let tip_i = n - 1;
        let mut chosen = vec![tip_i];
        let others: Vec<u64> = r2_idxs.into_iter().filter(|i| *i != tip_i).collect();
        let need = r2_samples as usize - 1;
        for s in 0..need {
            if others.is_empty() {
                break;
            }
            chosen.push(others[s * others.len() / need]);
        }
        r2_idxs = chosen;
    }
    for idx in r2_idxs {
        let id = id_at(idx)?;
        let t0 = Instant::now();
        let state = store.checkout(id)?;
        r2.record(t0.elapsed());
        let expect = match shape {
            Shape::Deep => live_keys.min((idx + 1).saturating_mul(puts_n)),
            Shape::Wide => (idx + 1).saturating_mul(puts_n),
        };
        if state.len() as u64 != expect {
            return Err(format!(
                "checkout at commit {idx}: got {} keys, expected {expect}",
                state.len()
            ));
        }
    }

    let disk_bytes = store.disk_bytes()?;
    let memory_bytes = sample_memory_bytes();

    Ok(BenchReport {
        engine: store.name().to_string(),
        tier: tier.name().to_string(),
        shape,
        live_keys,
        commits: n,
        w1_commit: w1,
        w2_branch: w2,
        r1_history: r1,
        r2_checkout: r2,
        disk_bytes,
        memory_bytes,
        load_secs,
    })
}
