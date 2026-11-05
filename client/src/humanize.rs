//! Times and sizes as the client prints them for a person.

/// A past moment's distance as "N s/min/h ago" (minutes and seconds, hours
/// past 60 min; the same shape as the UI catalog's).
pub fn ago(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s} s ago"),
        s if s < 3600 => format!("{} min ago", s / 60),
        s => format!("{} h {} min ago", s / 3600, (s % 3600) / 60),
    }
}

/// Bytes with a binary unit, one decimal past bytes.
pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let (mut v, mut i) = (b as f64, 0);
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// `YYYY-MM-DD HH:MM` in this machine's timezone, asked of the system's
/// `date` so no timezone database is bundled. Where `date` is missing
/// (Windows) it falls back to UTC, marked as such.
pub fn local_datetime(unix: u64) -> String {
    let out = std::process::Command::new("date")
        .args(["-d", &format!("@{unix}"), "+%Y-%m-%d %H:%M"])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => {
            let s = unix % 86_400;
            format!(
                "{} {:02}:{:02} UTC",
                homelab_core::state::ymd(unix),
                s / 3600,
                (s % 3600) / 60
            )
        }
    }
}
