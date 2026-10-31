//! The memory floor a deploy refuses to start below.

/// Below this share of available memory a deploy does not start
/// (corr-kuma-memory, 2026-09-27).
pub const MIN_MEMORY_AVAILABLE: f64 = 0.15;

/// `MemAvailable / MemTotal` from a container's /proc/meminfo (lxcfs shows
/// the container's own limit). None when either line is missing.
pub fn memory_available_fraction(meminfo: &str) -> Option<f64> {
    let field = |name: &str| {
        meminfo.lines().find_map(|l| {
            let rest = l.strip_prefix(name)?.trim_start_matches(':').trim();
            rest.split_whitespace().next()?.parse::<f64>().ok()
        })
    };
    let total = field("MemTotal")?;
    let avail = field("MemAvailable")?;
    (total > 0.0).then(|| avail / total)
}
