//! CLI smoke: init → commit → query against a temp project.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_epochs"))
}

fn temp_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("epochs_cli_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn run(args: &[&str], cwd: &std::path::Path) -> std::process::Output {
    Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("spawn epochs")
}

#[test]
fn init_commit_query() {
    let dir = temp_project("smoke");

    let out = run(&["init"], &dir);
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join(".epochs").is_dir());

    let out = run(&["commit", "-m", "hello", "--set", "status=ok"], &dir);
    assert!(
        out.status.success(),
        "commit failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = run(
        &["query", "MATCH (c:Commit) SELECT c.hash, c.message;"],
        &dir,
    );
    assert!(
        out.status.success(),
        "query failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hello") || stdout.contains("hash"));

    let _ = std::fs::remove_dir_all(&dir);
}
