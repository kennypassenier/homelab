//! Parsers for which disk backs pve's volume group, and how big it is
//! (fix-222, redesign-host-4).

/// fix-222 (Kenny, 2026-10-02: "root disk van HOST is 48% gebruikt, is dat
/// de 1TB SSD die erin zit?"): `lvs --noheadings --units g -o lv_size
/// <vg>/<lv>` — one line, e.g. `  50.00g` or `  <50.00g` (lvm prints `<`
/// when the figure is an estimate, as a thin pool's often is). GiB.
pub fn parse_lv_size_gb(out: &str) -> Option<f64> {
    out.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())?
        .trim_start_matches('<')
        .trim_end_matches(['g', 'G'])
        .parse()
        .ok()
}

/// fix-222: `pvs --noheadings -o pv_name <vg>` — the physical volume
/// backing a volume group, e.g. `  /dev/sda3`. A volume group spread over
/// more than one PV answers only the first, which is a fact to extend when
/// it ever applies here, not a case this guesses at.
pub fn parse_pv_name(out: &str) -> Option<String> {
    out.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// fix-222: the whole disk behind a partition device, so "which disk is
/// this" answers the disk a person can point at (`/dev/sda`) rather than
/// one of its partitions (`/dev/sda3`). Handles the common `sdX<n>` and
/// `nvme0n1p<n>`/`mmcblk0p<n>` shapes; a whole-disk PV (no partition
/// number, e.g. `/dev/sdb`) passes through unchanged. Known limitation: an
/// nvme/mmcblk device path with no partition suffix at all still ends in a
/// digit (`nvme0n1`), which this cannot tell apart from a trimmable
/// partition number from the string alone — pve's own install layout
/// (a GPT ESP plus a partition for the `pve` volume group) is always the
/// partitioned case, so this is accurate for the host this was built for.
pub fn whole_disk_device(partition: &str) -> String {
    let no_digits = partition.trim_end_matches(|c: char| c.is_ascii_digit());
    no_digits.strip_suffix('p').unwrap_or(no_digits).to_string()
}

/// fix-222: `blockdev --getsize64 <dev>` — one line, the size in bytes. GiB.
pub fn parse_blockdev_bytes(out: &str) -> Option<f64> {
    let bytes: f64 = out.lines().next()?.trim().parse().ok()?;
    Some(bytes / 1024.0 / 1024.0 / 1024.0)
}

/// fix-222: `du -x --max-depth=2 /` (`-x` stays on one filesystem, so a
/// managed container's own mounted subvolume is never double-counted into
/// root's own figure) — one `<KiB>\t<path>` line per directory. The root
/// line itself (`/`, the total) is dropped — these are "the biggest
/// directories ON root", not root itself — and the rest sorted largest
/// first, capped at `n`. GiB.
pub fn parse_du_top(out: &str, n: usize) -> Vec<(String, f64)> {
    let mut rows: Vec<(String, f64)> = out
        .lines()
        .filter_map(|l| {
            let (kb, path) = l.split_once('\t')?;
            let kb: f64 = kb.trim().parse().ok()?;
            let path = path.trim().to_string();
            (path != "/").then_some((path, kb / 1024.0 / 1024.0))
        })
        .collect();
    rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    rows.truncate(n);
    rows
}

/// redesign-host-4: `lsblk -d -n -o ROTA,TRAN <disk>` — the disk's kind as
/// a person names it: "NVMe SSD" by transport, otherwise "HDD" when it
/// rotates and "SSD" when it does not. None when the line does not read.
pub fn parse_disk_kind(out: &str) -> Option<String> {
    let line = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut parts = line.split_whitespace();
    let rota = parts.next()?;
    let tran = parts.next().unwrap_or("");
    let kind = match (rota, tran) {
        (_, "nvme") => "NVMe SSD",
        ("1", _) => "HDD",
        ("0", _) => "SSD",
        _ => return None,
    };
    Some(kind.to_string())
}
