//! redesign-host-4 (3.71.0): the host facts the approved Host page shows
//! that the host did not gather — how full the thin pool is and what its
//! volumes are promised, the root disk's kind, the host's uptime, and
//! whether the running daemon is a signed release. Parsing and judging are
//! pure and tested here; the host only runs the commands.

use homelab_core::ops::fleetcheck::{
    ThinPoolReading, parse_disk_kind, parse_proc_uptime, parse_thin_pool_report,
};
use homelab_core::release_sig::{record_proof_with, running_verdict_with};

/// `lvs --reportformat json --units g --nosuffix -o
/// lv_name,lv_size,pool_lv,data_percent,metadata_percent pve` on a default
/// Proxmox install: root, swap, the `data` thin pool and its volumes (two
/// guest disks, one template base, one snapshot).
const LVS: &str = r#"  {
      "report": [
          {
              "lv": [
                  {"lv_name":"data", "lv_size":"794.30", "pool_lv":"", "data_percent":"41.20", "metadata_percent":"2.53"},
                  {"lv_name":"root", "lv_size":"96.00", "pool_lv":"", "data_percent":"", "metadata_percent":""},
                  {"lv_name":"swap", "lv_size":"8.00", "pool_lv":"", "data_percent":"", "metadata_percent":""},
                  {"lv_name":"vm-104-disk-0", "lv_size":"8.00", "pool_lv":"data", "data_percent":"55.10", "metadata_percent":""},
                  {"lv_name":"vm-113-disk-0", "lv_size":"500.00", "pool_lv":"data", "data_percent":"60.00", "metadata_percent":""},
                  {"lv_name":"base-996-disk-0", "lv_size":"8.00", "pool_lv":"data", "data_percent":"20.00", "metadata_percent":""},
                  {"lv_name":"snap_vm-104-disk-0_pre", "lv_size":"8.00", "pool_lv":"data", "data_percent":"50.00", "metadata_percent":""}
              ]
          }
      ]
  }
"#;

#[test]
fn redesign_host_4_the_thin_pool_reads_its_use_and_what_its_volumes_are_promised() {
    let r = parse_thin_pool_report(LVS, "data").expect("the pool is in the report");
    assert_eq!(
        r,
        ThinPoolReading {
            data_pct: 41.2,
            metadata_pct: 2.53,
            promised_gb: 524.0,
            volumes: 4,
        }
    );
    // Units left on (`--units g` without `--nosuffix`) and lvm's `<` for a
    // rounded-down size read the same.
    let suffixed = LVS
        .replace("\"794.30\"", "\"<794.30g\"")
        .replace("\"500.00\"", "\"500.00g\"");
    assert_eq!(parse_thin_pool_report(&suffixed, "data"), Some(r));
    // No such pool, an inactive pool (no percent) and garbage: nothing,
    // never a made-up 0 %.
    assert_eq!(parse_thin_pool_report(LVS, "other"), None);
    assert_eq!(
        parse_thin_pool_report(&LVS.replace("\"41.20\"", "\"\""), "data"),
        None
    );
    assert_eq!(parse_thin_pool_report("not json", "data"), None);
}

#[test]
fn redesign_host_4_the_root_disk_kind_comes_from_rotation_and_transport() {
    // `lsblk -d -n -o ROTA,TRAN <disk>`
    assert_eq!(parse_disk_kind("   0 sata\n").as_deref(), Some("SSD"));
    assert_eq!(parse_disk_kind("0 nvme\n").as_deref(), Some("NVMe SSD"));
    assert_eq!(parse_disk_kind("   1 sata\n").as_deref(), Some("HDD"));
    // A virtual disk names no transport.
    assert_eq!(parse_disk_kind("   0 \n").as_deref(), Some("SSD"));
    assert_eq!(parse_disk_kind(""), None);
    assert_eq!(parse_disk_kind("lsblk: /dev/sdz: not a block device"), None);
}

#[test]
fn redesign_host_4_the_uptime_is_the_first_number_of_proc_uptime() {
    assert_eq!(
        parse_proc_uptime("1048683.27 15620044.81\n"),
        Some(1_048_683)
    );
    assert_eq!(parse_proc_uptime(""), None);
    assert_eq!(parse_proc_uptime("x y"), None);
}

/// A throwaway minisign key (made for this test, never used for anything
/// else) and its signature over `fixtures/release-proof/SHA256SUMS`, which
/// lists `BINARY` as `homelab-host`.
const TEST_KEY: &str = "RWSgaoeojAWI/vU6NOdqW6/vkgWw+t6O/vB0AGjuarwryTbLP5ENCeHM";
const SUMS: &str = include_str!("fixtures/release-proof/SHA256SUMS");
const SIG: &str = include_str!("fixtures/release-proof/SHA256SUMS.minisig");
const BINARY: &[u8] = b"homelab-host test binary\n";

#[test]
fn redesign_host_4_a_binary_installed_with_its_signature_reports_itself_signed() {
    let dir = tempfile::tempdir().unwrap();
    // Nothing recorded: not a signed release, and why.
    let v = running_verdict_with(TEST_KEY, dir.path(), "homelab-host", BINARY);
    assert!(!v.signed);
    assert!(v.detail.contains("no release signature"), "{}", v.detail);
    // Recorded at install: signed.
    record_proof_with(TEST_KEY, dir.path(), "homelab-host", BINARY, SUMS, SIG).unwrap();
    let v = running_verdict_with(TEST_KEY, dir.path(), "homelab-host", BINARY);
    assert!(v.signed, "{}", v.detail);
    // Another binary (a hand-built one installed from a file, say) finds no
    // signature of its own, even though one is recorded for the release.
    let v = running_verdict_with(TEST_KEY, dir.path(), "homelab-host", b"hand-built\n");
    assert!(!v.signed);
    // A signature that does not verify is never recorded.
    let bad = SIG.replace("RN0OIk0", "RN0OIk1");
    assert!(record_proof_with(TEST_KEY, dir.path(), "homelab-host", b"x", SUMS, &bad).is_err());
    // Nor one for a binary the list does not carry.
    assert!(record_proof_with(TEST_KEY, dir.path(), "homelab-host", b"x", SUMS, SIG).is_err());
}

#[test]
fn redesign_host_4_a_recorded_signature_altered_on_disk_is_not_trusted() {
    let dir = tempfile::tempdir().unwrap();
    record_proof_with(TEST_KEY, dir.path(), "homelab-host", BINARY, SUMS, SIG).unwrap();
    let proof_dir = dir.path().join(homelab_core::release_sig::PROOF_DIR);
    for e in std::fs::read_dir(&proof_dir).unwrap().flatten() {
        if e.path().extension().is_some_and(|x| x == "sums") {
            std::fs::write(e.path(), format!("{SUMS}0000  extra\n")).unwrap();
        }
    }
    let v = running_verdict_with(TEST_KEY, dir.path(), "homelab-host", BINARY);
    assert!(!v.signed, "{}", v.detail);
}
