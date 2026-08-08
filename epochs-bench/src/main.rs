//! epochs-bench — versioned KV / commit-DAG scale harness.

mod engines;
mod mem;
mod report;
mod workload;

use std::path::PathBuf;
use std::process;

use clap::{Parser, ValueEnum};

use engines::{EpochsEngine, MysqlEngine, PostgresEngine, Shape, SqliteEngine};
use workload::{run_bench, BenchOpts, Tier};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum EngineChoice {
    Epochs,
    Sqlite,
    Postgres,
    Mysql,
    /// Local embedded only (epochs + sqlite).
    Embedded,
    /// Dockerized SQL peers (postgres + mysql).
    Docker,
    All,
}

#[derive(Parser, Debug)]
#[command(
    name = "epochs-bench",
    about = "Versioned KV benchmarks: deep history (bounded keys) vs SQL delta replay"
)]
struct Cli {
    /// Tier: smoke / dev / mid / large / heavy (see --help for sizes).
    #[arg(long, default_value = "smoke")]
    tier: String,

    /// Workload shape: deep (default, git-like) or wide (unique key/commit).
    #[arg(long, default_value = "deep")]
    shape: String,

    /// Override commit count.
    #[arg(long)]
    commits: Option<u64>,

    /// Override live key cardinality (deep) or ignored unless set (wide).
    #[arg(long)]
    keys: Option<u64>,

    /// Puts per commit (default 1).
    #[arg(long, default_value_t = 1)]
    puts: u32,

    #[arg(long, value_enum, default_value_t = EngineChoice::Embedded)]
    engine: EngineChoice,

    #[arg(long, default_value = "target/epochs-bench-data")]
    data_dir: PathBuf,

    #[arg(long, default_value_t = 256)]
    payload_bytes: usize,

    #[arg(long, default_value_t = 64)]
    history_depth: u32,

    #[arg(long, default_value_t = 20)]
    branches: u64,

    #[arg(long, default_value_t = 10)]
    samples: u64,

    #[arg(long, default_value_t = 5_000)]
    progress_every: u64,

    #[arg(long, default_value_t = false)]
    force: bool,

    #[arg(long)]
    csv: Option<PathBuf>,

    #[arg(long, default_value = "postgres://bench:bench@127.0.0.1:54329/bench")]
    postgres_url: String,

    #[arg(long, default_value = "mysql://bench:bench@127.0.0.1:33069/bench")]
    mysql_url: String,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let tier = Tier::parse(&cli.tier)?;
    let shape = Shape::parse(&cli.shape)?;

    if tier.requires_force() && !cli.force {
        return Err(format!(
            "tier '{}' ≈ {} commits × {} keys — pass --force",
            tier.name(),
            tier.commits(),
            tier.live_keys()
        ));
    }
    if let Some(n) = cli.commits {
        if n >= 50_000_000 && !cli.force {
            return Err(format!("--commits {n} is huge — pass --force"));
        }
    }

    let engines: Vec<&str> = match cli.engine {
        EngineChoice::Epochs => vec!["epochs"],
        EngineChoice::Sqlite => vec!["sqlite"],
        EngineChoice::Postgres => vec!["postgres"],
        EngineChoice::Mysql => vec!["mysql"],
        EngineChoice::Embedded => vec!["epochs", "sqlite"],
        EngineChoice::Docker => vec!["postgres", "mysql"],
        EngineChoice::All => vec!["epochs", "sqlite", "postgres", "mysql"],
    };

    let opts = BenchOpts {
        tier,
        shape,
        commits_override: cli.commits,
        keys_override: cli.keys,
        payload_bytes: cli.payload_bytes,
        puts_per_commit: cli.puts,
        history_depth: cli.history_depth,
        branch_count: cli.branches,
        checkout_samples: cli.samples,
        progress_every: cli.progress_every,
    };

    let mut reports = Vec::new();
    for name in engines {
        eprintln!(
            "→ running {name} / {} shape={} keys≈{} commits≈{}",
            tier.name(),
            shape.as_str(),
            cli.keys.unwrap_or_else(|| match shape {
                Shape::Deep => tier.live_keys(),
                Shape::Wide => cli.commits.unwrap_or_else(|| tier.commits()),
            }),
            cli.commits.unwrap_or_else(|| tier.commits())
        );
        let report = match name {
            "epochs" => {
                let store = EpochsEngine::open(&cli.data_dir.join("epochs"))?;
                run_bench(store, &opts)?
            }
            "sqlite" => {
                let store = SqliteEngine::open(&cli.data_dir.join("sqlite"))?;
                run_bench(store, &opts)?
            }
            "postgres" => {
                let store = PostgresEngine::open(&cli.postgres_url)?;
                run_bench(store, &opts)?
            }
            "mysql" => {
                let store = MysqlEngine::open(&cli.mysql_url)?;
                run_bench(store, &opts)?
            }
            _ => unreachable!(),
        };
        reports.push(report);
    }

    report::print_reports(&reports);
    if let Some(csv) = &cli.csv {
        report::write_csv(csv, &reports)?;
        eprintln!("wrote CSV → {}", csv.display());
    }
    Ok(())
}
