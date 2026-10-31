//! Parsers for the host-level readings the shell takes (rule-20).

/// rule-20: pve's own root filesystem, from `df --output=pcent /` (or any
/// single-column `df` output ending in a percentage) — the last non-empty
/// line, so a header row (`Use%`) is skipped without needing to know
/// whether one was printed.
pub fn parse_df_pcent(out: &str) -> Option<u8> {
    out.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())?
        .trim_end_matches('%')
        .parse()
        .ok()
}

/// rule-20: `lvs --noheadings -o data_percent,metadata_percent <thin-pool>`
/// — one line, two space-separated percentages (lvs prints them as plain
/// decimals, e.g. `27.66  1.15`, no `%` sign). Returns `(data, meta)`
/// rounded to the nearest whole percent.
pub fn parse_thin_pool_percents(out: &str) -> Option<(u8, u8)> {
    let line = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut parts = line.split_whitespace();
    let data: f64 = parts.next()?.parse().ok()?;
    let meta: f64 = parts.next()?.parse().ok()?;
    Some((data.round() as u8, meta.round() as u8))
}

/// rule-20: `zpool list -H -o name,capacity` — `-H` makes it tab-separated,
/// one pool per line, capacity as e.g. `79%`. Every pool on the host, not
/// only the ones a stack happens to declare a mount on.
pub fn parse_zpool_capacities(out: &str) -> Vec<(String, u8)> {
    out.lines()
        .filter_map(|l| {
            let mut c = l.split('\t');
            let name = c.next()?.trim();
            let pct: u8 = c.next()?.trim().trim_end_matches('%').parse().ok()?;
            (!name.is_empty()).then(|| (name.to_string(), pct))
        })
        .collect()
}

/// rule-20: `journalctl --disk-usage`, e.g. "Archived and active journals \
/// take up 1.9G in the file system." — the number and unit just before "in
/// the file system", converted to MiB. None when the line cannot be read,
/// which is not the same as "empty" and is simply not reported.
pub fn parse_journal_disk_usage_mib(out: &str) -> Option<u64> {
    let line = out.lines().find(|l| l.contains("in the file system"))?;
    let token = line.split_whitespace().find(|t| {
        t.chars().next().is_some_and(|c| c.is_ascii_digit())
            && matches!(t.chars().last(), Some('K' | 'M' | 'G' | 'T'))
    })?;
    let (num, unit) = token.split_at(token.len() - 1);
    let value: f64 = num.parse().ok()?;
    let mib = match unit {
        "K" => value / 1024.0,
        "M" => value,
        "G" => value * 1024.0,
        "T" => value * 1024.0 * 1024.0,
        _ => return None,
    };
    Some(mib.round() as u64)
}

/// rule-20: the scalar value of a Prometheus instant-query response, e.g.
/// `{"status":"success","data":{"resultType":"vector","result":[{"metric":{},"value":[1,"12345"]}]}}`
/// — the number right after the first `"value":[<timestamp>,`. None for an
/// empty result (`"result":[]`, nothing scraped yet) or anything that does
/// not parse, which is not the same as zero and is simply not reported.
pub fn parse_prometheus_scalar(json: &str) -> Option<f64> {
    let after = json.split_once("\"value\":[")?.1;
    let after_ts = after.split_once(',')?.1;
    let quoted = after_ts.split_once('"')?.1;
    let (num, _) = quoted.split_once('"')?;
    num.parse().ok()
}

/// rule-20: `du -sm <dir>` — the first whitespace-separated token, in MiB.
pub fn parse_du_sm(out: &str) -> Option<u64> {
    out.lines().next()?.split_whitespace().next()?.parse().ok()
}

/// redesign-host-4: `/proc/uptime` — seconds since boot, the first number.
pub fn parse_proc_uptime(text: &str) -> Option<u64> {
    let secs: f64 = text.split_whitespace().next()?.parse().ok()?;
    (secs.is_finite() && secs >= 0.0).then_some(secs as u64)
}
