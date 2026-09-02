//! G10 · the thirteen stack files that actually run this house, validated.
//!
//! Every other test in this workspace builds a manifest in code. That proves
//! the validator works; it proves nothing about the files on disk, which are
//! the ones a deploy reads. The gap the Phase-7 audit found is exactly that
//! distance — and the register is full of faults that lived in a real file
//! while every synthetic one was fine: a template pointing at the retired
//! golden image, a stack contradicting itself in consecutive lines, a
//! promtail label naming the wrong container.
//!
//! These run offline. The half that needs latch (substituting secrets into
//! the compose files before parsing them) is deliberately not here — a test
//! that cannot run without a decrypted vault is a test that does not run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use homelab_core::manifest::{validate_manifest, StackManifest};

fn stacks_dir() -> PathBuf {
    // core/tests/ -> repo root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("stacks")
}

/// Every stack directory that carries a compose manifest. Native stacks
/// (`service.yml`) are a different shape and are not this test's subject.
fn compose_stacks() -> Vec<(String, StackManifest)> {
    let mut out = Vec::new();
    let dir = stacks_dir();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("stacks/ must be readable at {:?}: {}", dir, e))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        let f = p.join("lxc-compose.yml");
        if !f.is_file() {
            continue;
        }
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&f).unwrap();
        let m: StackManifest = serde_yaml::from_str(&text)
            .unwrap_or_else(|e| panic!("{} does not parse as a manifest: {}", name, e));
        out.push((name, m));
    }
    assert!(
        out.len() >= 10,
        "found only {} compose stacks — this test is looking in the wrong place",
        out.len()
    );
    out
}

#[test]
fn every_stack_file_on_disk_passes_the_validator_a_deploy_would_run() {
    for (name, m) in compose_stacks() {
        validate_manifest(&m).unwrap_or_else(|e| panic!("stacks/{} is not valid: {:?}", name, e));
    }
}

#[test]
fn the_directory_name_is_the_stack_name() {
    for (name, m) in compose_stacks() {
        assert_eq!(
            m.stack_name, name,
            "stacks/{} calls itself {} — every lookup in this project keys off the \
             directory, so the two must agree",
            name, m.stack_name
        );
    }
}

#[test]
fn no_two_stacks_claim_the_same_vmid_hostname_or_address() {
    let mut vmids: BTreeMap<u16, String> = BTreeMap::new();
    let mut hosts: BTreeMap<String, String> = BTreeMap::new();
    let mut ips: BTreeMap<String, String> = BTreeMap::new();
    for (name, m) in compose_stacks() {
        if let Some(prev) = vmids.insert(m.vmid, name.clone()) {
            panic!("{} and {} both claim vmid {}", prev, name, m.vmid);
        }
        if let Some(prev) = hosts.insert(m.hostname.clone(), name.clone()) {
            panic!("{} and {} both claim hostname {}", prev, name, m.hostname);
        }
        let ip = m.network.ip.clone();
        if let Some(prev) = ips.insert(ip.clone(), name.clone()) {
            panic!("{} and {} both claim {}", prev, name, ip);
        }
    }
}

/// The layout's own rule, and the one that makes a container findable without
/// looking anything up: `<vmid>-app-<stack>` at `10.10.10.<vmid - 100>`.
#[test]
fn hostname_and_address_both_follow_from_the_vmid() {
    for (name, m) in compose_stacks() {
        assert!(
            m.hostname.starts_with(&format!("{}-", m.vmid)),
            "stacks/{}: hostname {} does not start with its vmid {}",
            name,
            m.hostname,
            m.vmid
        );
        assert!(
            m.hostname.ends_with(&format!("-{}", m.stack_name)),
            "stacks/{}: hostname {} does not end with its stack name",
            name,
            m.hostname
        );
        let expected = format!("10.10.10.{}/24", m.vmid - 100);
        assert_eq!(
            m.network.ip, expected,
            "stacks/{}: vmid {} means {}, not {}",
            name, m.vmid, expected, m.network.ip
        );
    }
}

/// Found by making this mistake while building the drill stack: a `sed` over
/// a copied config left `host: 117-app-drill` on a container called
/// `118-app-drill`. Nothing would have complained — the logs would simply
/// have arrived under another container's name, and the three dashboards
/// that group by host would have quietly lied. Twelve of these labels are
/// hand-copied and nothing checked a single one.
#[test]
fn every_promtail_label_names_the_container_it_actually_runs_on() {
    for (name, m) in compose_stacks() {
        let cfg = stacks_dir()
            .join(&name)
            .join("promtail")
            .join("promtail-config.yml");
        if !cfg.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&cfg).unwrap();
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            if let Some(v) = t.strip_prefix("host: ") {
                assert_eq!(
                    v.trim(),
                    m.hostname,
                    "stacks/{}/promtail/promtail-config.yml:{} labels its logs {} \
                     while the container is {}",
                    name,
                    i + 1,
                    v.trim(),
                    m.hostname
                );
            }
            if let Some(v) = t.strip_prefix("stack: ") {
                assert_eq!(
                    v.trim(),
                    m.stack_name,
                    "stacks/{}/promtail/promtail-config.yml:{} labels its logs for stack {}",
                    name,
                    i + 1,
                    v.trim()
                );
            }
        }
    }
}

#[test]
fn every_app_listed_by_a_stack_has_a_compose_file_and_the_other_way_round() {
    for (name, m) in compose_stacks() {
        let dir = stacks_dir().join(&name);
        for app in &m.apps {
            let f = dir.join(app).join("docker-compose.yml");
            assert!(
                f.is_file(),
                "stacks/{} lists app {} and there is no {:?}",
                name,
                app,
                f
            );
        }
        // And nothing on disk that the manifest forgot: a compose file in a
        // directory nobody lists is a service that silently never deploys.
        for e in std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
            let p = e.path();
            if p.join("docker-compose.yml").is_file() {
                let app = p.file_name().unwrap().to_string_lossy().to_string();
                assert!(
                    m.apps.contains(&app),
                    "stacks/{} has a compose file for {} that the manifest does not list — \
                     it would never be deployed and nothing would say so",
                    name,
                    app
                );
            }
        }
    }
}

#[test]
fn every_mount_belongs_to_an_app_the_stack_actually_runs() {
    for (name, m) in compose_stacks() {
        for s in &m.storage {
            // `app` is optional in the schema; a mount without one belongs to
            // the stack rather than to a service, which is legal.
            let Some(app) = s.app.as_ref() else { continue };
            // A native stack (T5) runs systemd units rather than compose
            // apps, and its mounts are owned by those.
            assert!(
                m.apps.contains(app) || m.natives.contains(app),
                "stacks/{}: mount {} is owned by {}, which this stack neither runs as \
                 a compose app nor as a native service",
                name,
                s.host_path,
                app
            );
        }
    }
}

/// F187's shape, generalised: a stack cloning a template that is not there
/// rebuilds into nothing, and it fails at the moment the rebuild matters.
#[test]
fn every_template_is_one_of_the_golden_ones_that_exist() {
    for (name, m) in compose_stacks() {
        let t = m.lxc.template.trim_matches('"').to_string();
        let Some(vmid) = t.strip_prefix("clone:") else {
            continue;
        };
        assert!(
            ["997", "998"].contains(&vmid.trim()),
            "stacks/{} clones {} — 999 is the retired v1 image and anything else \
             does not exist on this host",
            name,
            t
        );
    }
}
