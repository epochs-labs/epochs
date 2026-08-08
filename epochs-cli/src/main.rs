//! Developer CLI for epochs — init, commit, branch, checkout, migrate, and query.
//!
//! Typical usage from a project root:
//!
//! ```text
//! epochs init
//! epochs migrate .
//! epochs query 'MATCH (c:Commit) SELECT c.hash;'
//! ```
//!
//! The repository lives at `<project>/.epochs`. Pass a project directory (default
//! `.`) or an explicit `.epochs` path; migrations are read from
//! `<repo>/migrations/`.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use epochql::{migrate, Engine};
use epochs_core::{DagStore, DiskStore, Hash};

/// Epochs — version-controlled DAG database for agent state and beyond.
#[derive(Parser, Debug)]
#[command(name = "epochs", version, about, long_about = None)]
struct Cli {
    /// Project directory (looks for `.epochs/` inside). Default: current directory.
    #[arg(short = 'C', long = "path", global = true, default_value = ".")]
    path: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize a new local epochs repository (`<path>/.epochs`).
    Init {
        /// Default branch name.
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Create a new commit on the current branch.
    Commit {
        /// Commit message.
        #[arg(short, long)]
        message: Option<String>,
        /// Key-value pairs in `key=value` form (repeatable).
        #[arg(short, long)]
        set: Vec<String>,
    },
    /// Create a new branch from a target (branch name, HEAD, or commit hash).
    Branch {
        /// Branch name to create.
        name: String,
        /// Source target (defaults to HEAD branch tip).
        #[arg(long)]
        from: Option<String>,
    },
    /// Resolve a branch or commit hash to its commit record.
    Checkout {
        /// Branch name or commit hash.
        target: String,
    },
    /// Apply pending `.eql` files from `<repo>/migrations/`.
    Migrate {
        /// Project directory or `.epochs` path (default: global `--path` / `.`).
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Execute EpochQL against the repository.
    Query {
        /// EpochQL source string (statement or script).
        query: String,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { branch } => {
            let repo = repo_path_for_init(&cli.path);
            cmd_init(&repo, &branch)
        }
        Commands::Commit { message, set } => {
            let repo = resolve_repo(&cli.path)?;
            cmd_commit(&repo, message, &set)
        }
        Commands::Branch { name, from } => {
            let repo = resolve_repo(&cli.path)?;
            cmd_branch(&repo, &name, from.as_deref())
        }
        Commands::Checkout { target } => {
            let repo = resolve_repo(&cli.path)?;
            cmd_checkout(&repo, &target)
        }
        Commands::Migrate { dir } => {
            let project = if dir.as_os_str() != "." {
                dir
            } else {
                cli.path
            };
            let repo = resolve_repo(&project)?;
            cmd_migrate(&repo)
        }
        Commands::Query { query } => {
            let repo = resolve_repo(&cli.path)?;
            cmd_query(&repo, &query)
        }
    }
}

/// Where `init` should create the repository.
fn repo_path_for_init(project: &Path) -> PathBuf {
    if is_epochs_repo(project) || looks_like_epochs_dirname(project) {
        project.to_path_buf()
    } else {
        project.join(".epochs")
    }
}

/// Resolve a project directory or explicit repo path to the `.epochs` store.
fn resolve_repo(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if is_epochs_repo(path) {
        return Ok(path.to_path_buf());
    }

    let nested = path.join(".epochs");
    if is_epochs_repo(&nested) {
        return Ok(nested);
    }

    // Helpful errors when migrations exist but the store does not (or vice versa).
    let mig_at_root = path.join("migrations");
    let mig_nested = nested.join("migrations");
    if mig_at_root.is_dir() || mig_nested.is_dir() {
        return Err(format!(
            "found migrations but no epochs repository under {} (run: epochs init -C {})",
            path.display(),
            path.display()
        )
        .into());
    }

    Err(format!(
        "no epochs repository at {} or {} (run: epochs init)",
        path.display(),
        nested.display()
    )
    .into())
}

fn is_epochs_repo(path: &Path) -> bool {
    path.join("data").is_dir()
}

fn looks_like_epochs_dirname(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == ".epochs")
}

fn cmd_init(data_dir: &Path, branch: &str) -> Result<(), Box<dyn std::error::Error>> {
    if data_dir.exists() {
        return Err(format!("repository already exists: {}", data_dir.display()).into());
    }

    let (store, root) = DiskStore::init(data_dir, branch, "Initial commit")?;
    std::fs::create_dir_all(data_dir.join("migrations"))?;
    std::fs::write(
        data_dir.join("migrations/001_init.eql"),
        r#"-- Example schema migration. Uncomment and run: epochs migrate .
-- CREATE COLLECTION items KEY id STRING;
-- CREATE INDEX ON items (id);
"#,
    )?;

    println!("Initialized epochs repository at {}", data_dir.display());
    println!("  branch: {branch}");
    println!("  root:   {root}");
    println!("  migrations: {}/migrations/", data_dir.display());
    let _ = store;
    Ok(())
}

fn cmd_commit(
    data_dir: &Path,
    message: Option<String>,
    set: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = DiskStore::open(data_dir)?;
    let msg = message.unwrap_or_else(|| "commit".into());

    if set.is_empty() {
        let head = store
            .head()?
            .ok_or_else(|| epochs_core::EpochsError::InvalidTarget("HEAD not set".into()))?;
        let parent = head.target;
        let parent_commit = store.get_commit(&parent)?;
        let root = if parent_commit.root_hamt == Hash::ZERO {
            None
        } else {
            Some(parent_commit.root_hamt)
        };
        let new_hash = store.commit(vec![parent], root, &[], &msg)?;
        store.update_branch(&head.name, new_hash)?;
        println!("commit {new_hash}");
        println!("  branch: {}", head.name);
        return Ok(());
    }

    // Route through EpochQL so schema indexes stay in sync when present.
    let mut pairs = Vec::new();
    for pair in set {
        let (key, value) = parse_kv(pair)?;
        pairs.push(format!(
            "{}: \"{}\"",
            key,
            value.replace('\\', "\\\\").replace('"', "\\\"")
        ));
    }
    let eql = format!(
        "COMMIT {{ {} }} MESSAGE \"{}\";",
        pairs.join(", "),
        msg.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let mut engine = Engine::new(&mut store);
    let results = engine.execute(&eql)?;
    for result in results {
        println!("{result}");
    }
    Ok(())
}

fn cmd_branch(
    data_dir: &Path,
    name: &str,
    from: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = DiskStore::open(data_dir)?;
    let from = from.unwrap_or("HEAD");
    let target = if from.eq_ignore_ascii_case("HEAD") {
        store
            .head()?
            .ok_or_else(|| epochs_core::EpochsError::InvalidTarget("HEAD not set".into()))?
            .target
    } else if let Ok(branch) = store.get_branch(from) {
        branch.target
    } else {
        store.resolve_hash_ref(from)?
    };

    store.create_branch(name, target)?;
    println!("created branch '{name}' at {target}");
    Ok(())
}

fn cmd_checkout(data_dir: &Path, target: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = DiskStore::open(data_dir)?;
    let commit = store.checkout(target)?;
    println!("commit {}", commit.id());
    println!("  root_hamt: {}", commit.root_hamt);
    println!("  message:   {}", commit.message);
    Ok(())
}

fn cmd_migrate(data_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let migrations = data_dir.join("migrations");
    if !migrations.is_dir() {
        return Err(format!(
            "no migrations directory at {} (expected .eql files there)",
            migrations.display()
        )
        .into());
    }

    let report = migrate(data_dir)?;
    if report.applied.is_empty() {
        println!(
            "No pending migrations ({} already applied).",
            report.skipped.len()
        );
    } else {
        println!("Applied {} migration(s):", report.applied.len());
        for name in &report.applied {
            println!("  {name}");
        }
    }
    println!(
        "Schema: {} collection(s), {} index(es)",
        report.schema.collections.len(),
        report.schema.all_indexes().len()
    );
    Ok(())
}

fn cmd_query(data_dir: &Path, query: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = DiskStore::open(data_dir)?;
    let mut engine = Engine::new(&mut store);
    let results = engine.execute(query)?;
    for (i, result) in results.iter().enumerate() {
        if results.len() > 1 {
            println!("--- result {} ---", i + 1);
        }
        println!("{result}");
    }
    Ok(())
}

fn parse_kv(pair: &str) -> epochs_core::Result<(&str, &str)> {
    match pair.split_once('=') {
        Some((k, v)) if !k.is_empty() => Ok((k, v)),
        _ => Err(epochs_core::EpochsError::Codec(format!(
            "invalid key=value pair: {pair}"
        ))),
    }
}
