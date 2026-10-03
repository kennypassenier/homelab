//! redesign-host-4 (3.71.0): the new host facts travel as optional fields,
//! so an older host (which sends none of them) and an older dashboard or
//! client (which sends no release proof) keep working.

use homelab_proto::{Command, FleetState, GuestUse, ReleaseProof, ReleaseVerdict, ThinPoolUse};

/// A `GetState` answer as a 3.70 host sends it: none of the new fields.
const OLD_STATE: &str = r#"{
  "host": {
    "name": "pve-01", "cpu_pct": 7, "ram_pct": 40, "disk_pct": 31,
    "tls_fingerprint": "SHA256:AA", "ram_total_mb": 65536, "ram_used_mb": 26000,
    "ram_committed_mb": 30000, "cores_total": 16, "load1_x100": 80,
    "disk_detail": {
      "root_lv_size_gb": 96.0, "root_disk_device": "/dev/sda",
      "root_disk_total_gb": 931.5, "thin_pool_size_gb": 780.0,
      "top_dirs": [["/var", 42.0]], "measured_at": 1
    }
  },
  "stacks": []
}"#;

#[test]
fn redesign_host_4_an_older_hosts_state_reads_with_every_new_fact_absent() {
    let s: FleetState = serde_json::from_str(OLD_STATE).expect("an older host's state reads");
    assert_eq!(s.host.uptime_s, None);
    assert_eq!(s.host.release, None);
    assert_eq!(s.host.guests_usage, None);
    let d = s.host.disk_detail.expect("disk detail");
    assert_eq!(d.thin_pool, None);
    assert_eq!(d.root_disk_kind, None);
}

#[test]
fn redesign_host_4_a_new_hosts_state_carries_every_new_fact() {
    let mut s: FleetState = serde_json::from_str(OLD_STATE).unwrap();
    s.host.uptime_s = Some(1_048_683);
    s.host.release = Some(ReleaseVerdict {
        signed: true,
        detail: "verified".into(),
    });
    s.host.guests_usage = Some(vec![GuestUse {
        vmid: 113,
        usage: homelab_proto::GuestUsage {
            cpu_permille: 125,
            ram_used_mb: 3072,
            ram_max_mb: 8192,
            uptime_s: 10,
        },
    }]);
    let d = s.host.disk_detail.as_mut().unwrap();
    d.root_disk_kind = Some("SSD".into());
    d.thin_pool = Some(ThinPoolUse {
        data_pct: 41.2,
        metadata_pct: 2.53,
        promised_gb: 524.0,
        volumes: 4,
    });
    let json = serde_json::to_value(&s).unwrap();
    // A guest's use is flat beside its vmid, the shape the page reads.
    assert_eq!(json["host"]["guests_usage"][0]["vmid"], 113);
    assert_eq!(json["host"]["guests_usage"][0]["ram_used_mb"], 3072);
    assert_eq!(json["host"]["disk_detail"]["thin_pool"]["data_pct"], 41.2);
    let back: FleetState = serde_json::from_value(json).unwrap();
    assert_eq!(back.host.uptime_s, Some(1_048_683));
    assert_eq!(back.host.guests_usage.unwrap()[0].usage.ram_max_mb, 8192);
    // Absent facts are left out on the wire, not sent as nulls.
    let bare =
        serde_json::to_value(serde_json::from_str::<FleetState>(OLD_STATE).unwrap()).unwrap();
    assert!(bare["host"].get("uptime_s").is_none());
    assert!(bare["host"]["disk_detail"].get("thin_pool").is_none());
}

#[test]
fn redesign_host_4_self_update_carries_its_release_proof_and_reads_without_one() {
    // An older client or dashboard sends no proof.
    let old: Command =
        serde_json::from_str(r#"{"cmd":"self_update_host","binary_b64":"QUJD"}"#).unwrap();
    assert!(matches!(old, Command::SelfUpdateHost { proof: None, .. }));
    let new = Command::SelfUpdateHost {
        binary_b64: "QUJD".into(),
        proof: Some(ReleaseProof {
            sums: "abc  homelab-host\n".into(),
            sig: "untrusted comment: x\n".into(),
        }),
    };
    let json = serde_json::to_string(&new).unwrap();
    assert!(json.contains("\"proof\""), "{json}");
    match serde_json::from_str::<Command>(&json).unwrap() {
        Command::SelfUpdateHost { proof: Some(p), .. } => assert_eq!(p.sums, "abc  homelab-host\n"),
        other => panic!("{other:?}"),
    }
    // A file shipped without a proof says nothing about one on the wire,
    // so an older host reads it exactly as before.
    let bare = serde_json::to_string(&Command::SelfUpdateHost {
        binary_b64: "QUJD".into(),
        proof: None,
    })
    .unwrap();
    assert!(!bare.contains("proof"), "{bare}");
}
