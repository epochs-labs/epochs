//! Pretty-print / CSV for VCS bench reports.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use crate::engines::BenchReport;

fn fmt_dur(d: Duration) -> String {
    let us = d.as_secs_f64() * 1_000_000.0;
    if us < 1000.0 {
        format!("{us:.1}µs")
    } else if us < 1_000_000.0 {
        format!("{:.2}ms", us / 1000.0)
    } else {
        format!("{:.2}s", us / 1_000_000.0)
    }
}

fn fmt_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let x = n as f64;
    if x >= GB {
        format!("{:.2} GiB", x / GB)
    } else if x >= MB {
        format!("{:.2} MiB", x / MB)
    } else if x >= KB {
        format!("{:.1} KiB", x / KB)
    } else {
        format!("{n} B")
    }
}

fn tps(count: usize, secs: f64) -> f64 {
    if secs <= 0.0 {
        0.0
    } else {
        count as f64 / secs
    }
}

pub fn print_reports(reports: &[BenchReport]) {
    println!();
    println!("=== epochs-bench (deep history / versioned KV) ===");
    println!();
    println!(
        "{:<9} {:<5} {:>7} {:>8} {:>10} {:>9} {:>9} {:>9} {:>9} {:>10} {:>10}",
        "engine",
        "shape",
        "keys",
        "commits",
        "commit/s",
        "W1 p50",
        "W2 p50",
        "R1 p50",
        "R2 p50",
        "disk",
        "rss"
    );
    println!("{}", "-".repeat(112));

    for r in reports {
        let cps = tps(r.w1_commit.count(), r.load_secs);
        let rss = r.memory_bytes.map(fmt_bytes).unwrap_or_else(|| "—".into());
        println!(
            "{:<9} {:<5} {:>7} {:>8} {:>10.0} {:>9} {:>9} {:>9} {:>9} {:>10} {:>10}",
            r.engine,
            r.shape.as_str(),
            r.live_keys,
            r.commits,
            cps,
            fmt_dur(r.w1_commit.p50()),
            fmt_dur(r.w2_branch.p50()),
            fmt_dur(r.r1_history.p50()),
            fmt_dur(r.r2_checkout.p50()),
            fmt_bytes(r.disk_bytes),
            rss
        );
        println!(
            "          {:>5} {:>7} {:>8} {:>10} {:>9} {:>9} {:>9} {:>9}",
            "",
            "",
            "",
            "(p99 →)",
            fmt_dur(r.w1_commit.p99()),
            fmt_dur(r.w2_branch.p99()),
            fmt_dur(r.r1_history.p99()),
            fmt_dur(r.r2_checkout.p99())
        );
    }
    println!();
    println!("shape=deep → fixed live keys, updates over history (git-like)");
    println!("W1=commit  W2=branch  R1=history  R2=checkout (tip+samples)");
    println!("SQL peers: branches + commits(parent) + commit_ops (delta replay).");
}

pub fn write_csv(path: &Path, reports: &[BenchReport]) -> Result<(), String> {
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;

    if f.metadata().map(|m| m.len()).unwrap_or(1) == 0 {
        writeln!(
            f,
            "engine,tier,shape,live_keys,commits,commit_per_s,w1_p50_us,w2_p50_us,r1_p50_us,r2_p50_us,disk_bytes,rss_bytes"
        )
        .map_err(|e| e.to_string())?;
    }

    for r in reports {
        let cps = tps(r.w1_commit.count(), r.load_secs);
        writeln!(
            f,
            "{},{},{},{},{},{:.1},{},{},{},{},{},{}",
            r.engine,
            r.tier,
            r.shape.as_str(),
            r.live_keys,
            r.commits,
            cps,
            r.w1_commit.p50().as_micros(),
            r.w2_branch.p50().as_micros(),
            r.r1_history.p50().as_micros(),
            r.r2_checkout.p50().as_micros(),
            r.disk_bytes,
            r.memory_bytes.map(|m| m.to_string()).unwrap_or_default()
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
