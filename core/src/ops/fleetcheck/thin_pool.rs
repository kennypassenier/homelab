//! The local-lvm thin pool as `lvs` reports it.

use serde::Deserialize;

/// redesign-host-4: a thin pool's use and what its volumes are promised.
#[derive(Debug, Clone, PartialEq)]
pub struct ThinPoolReading {
    /// How full the pool's data area is, percent.
    pub data_pct: f64,
    /// How full its metadata area is, percent (a full metadata area stops
    /// every guest on the pool as surely as full data does).
    pub metadata_pct: f64,
    /// The sum of the virtual sizes of every volume carved from the pool
    /// (guest disks and mount points, template bases, snapshots), GiB: what
    /// the pool has promised. Above the pool's own size is thin
    /// over-provisioning.
    pub promised_gb: f64,
    /// How many volumes that sum covers.
    pub volumes: u32,
}

#[derive(Deserialize)]
struct LvsReport {
    report: Vec<LvsSection>,
}

#[derive(Deserialize)]
struct LvsSection {
    #[serde(default)]
    lv: Vec<LvsRow>,
}

#[derive(Deserialize)]
struct LvsRow {
    lv_name: String,
    #[serde(default)]
    lv_size: String,
    #[serde(default)]
    pool_lv: String,
    #[serde(default)]
    data_percent: String,
    #[serde(default)]
    metadata_percent: String,
}

/// An lvm number as `lvs` prints it: `794.30`, `794.30g`, `<794.30g`.
fn lvm_number(s: &str) -> Option<f64> {
    s.trim()
        .trim_start_matches('<')
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .parse()
        .ok()
}

/// redesign-host-4: `lvs --reportformat json --units g --nosuffix -o
/// lv_name,lv_size,pool_lv,data_percent,metadata_percent <vg>` — the pool
/// named `pool` and every volume whose `pool_lv` is it. None when the pool
/// is not in the report or reports no percentages (inactive): never a
/// made-up 0 %.
pub fn parse_thin_pool_report(json: &str, pool: &str) -> Option<ThinPoolReading> {
    let report: LvsReport = serde_json::from_str(json.trim()).ok()?;
    let rows: Vec<&LvsRow> = report.report.iter().flat_map(|s| s.lv.iter()).collect();
    let p = rows.iter().find(|r| r.lv_name == pool)?;
    let data_pct = lvm_number(&p.data_percent)?;
    let metadata_pct = lvm_number(&p.metadata_percent)?;
    let carved: Vec<&&LvsRow> = rows.iter().filter(|r| r.pool_lv == pool).collect();
    let promised_gb = carved
        .iter()
        .filter_map(|r| lvm_number(&r.lv_size))
        .sum::<f64>();
    Some(ThinPoolReading {
        data_pct,
        metadata_pct,
        promised_gb: (promised_gb * 100.0).round() / 100.0,
        volumes: u32::try_from(carved.len()).unwrap_or(u32::MAX),
    })
}
