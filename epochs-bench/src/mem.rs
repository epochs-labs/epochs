//! Process / cgroup memory sampling helpers.

/// Best-effort memory for the system under test (bytes).
///
/// Inside Docker we prefer **cgroup** usage so SQL engines report server+client
/// together (same budget the compose `mem_limit` enforces). Falls back to process RSS.
pub fn sample_memory_bytes() -> Option<u64> {
    cgroup_memory_bytes().or_else(process_rss_bytes)
}

fn cgroup_memory_bytes() -> Option<u64> {
    // cgroup v2
    if let Ok(s) = std::fs::read_to_string("/sys/fs/cgroup/memory.current") {
        if let Ok(n) = s.trim().parse::<u64>() {
            return Some(n);
        }
    }
    // cgroup v1
    if let Ok(s) = std::fs::read_to_string("/sys/fs/cgroup/memory/memory.usage_in_bytes") {
        if let Ok(n) = s.trim().parse::<u64>() {
            return Some(n);
        }
    }
    None
}

fn process_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
                return Some(kb * 1024);
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p"])
            .arg(std::process::id().to_string())
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout);
        let kb: u64 = s.trim().parse().ok()?;
        Some(kb * 1024)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}
