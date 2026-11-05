//! `log_drop:` in a stack file: lines the log shipper must not forward.
//!
//! Generic by design: no test names an app. The behaviours held here are the
//! rendered shipper config, the manifest validation of an entry, and the
//! guarantee that a stack without the key keeps the manifest hash (and so the
//! shipper config) it had before the key existed.

use homelab_core::manifest::{CURRENT_CLIENT_SCHEMA, DeploySpec};
use homelab_core::manifest::{StackManifest, intent_hash, validate_manifest};
use homelab_core::ops::logshipper::{config, config_with_drops};

const LOKI: &str = "http://10.10.10.13:3100";

fn manifest_json() -> serde_json::Value {
    serde_json::json!({
        "stack_name": "demo", "vmid": 150, "hostname": "150-app-demo",
        "network": {"ip": "10.10.10.50/24", "gateway": "10.10.10.1", "bridge": "vmbr0", "vlan": 10},
        "resources": {"cores": 1, "memory_mb": 512, "swap_mb": 0, "disk_gb": 4, "storage": "local-lvm"},
        "lxc": {"template": "local:vztmpl/x.tar.zst"},
        "boot": {"onboot": true, "order": 50},
        "storage": [], "apps": ["demo"]
    })
}

fn manifest_with(drop: &[&str]) -> StackManifest {
    let mut v = manifest_json();
    if !drop.is_empty() {
        v["log_drop"] = serde_json::json!(drop);
    }
    serde_json::from_value(v).expect("manifest parses")
}

fn render(drop: &[&str]) -> String {
    let d: Vec<String> = drop.iter().map(|s| s.to_string()).collect();
    config_with_drops("demo", "150-app-demo", LOKI, &[], &[], &d)
}

fn problems(drop: &[&str]) -> String {
    match validate_manifest(&manifest_with(drop)) {
        Ok(()) => String::new(),
        Err(e) => e.to_string(),
    }
}

#[test]
fn no_list_renders_exactly_the_config_of_before() {
    assert_eq!(
        render(&[]),
        config("demo", "150-app-demo", LOKI, &[], &[]),
        "a stack without log_drop must keep its shipper config byte for byte"
    );
    assert!(!render(&[]).contains("loki.process \"drop\""));
}

#[test]
fn a_list_adds_one_drop_stage_per_entry_and_routes_every_source_through_it() {
    let c = render(&["first noise", "second (a|b) noise"]);
    assert!(c.contains("loki.process \"drop\""), "{c}");
    assert_eq!(c.matches("stage.drop").count(), 2, "{c}");
    assert!(c.contains("expression = \"first noise\""), "{c}");
    // Docker, journal and syslog sources all forward to the drop stage; only
    // the drop stage itself writes to Loki.
    assert_eq!(
        c.matches("forward_to = [loki.process.drop.receiver]")
            .count(),
        3,
        "{c}"
    );
    assert_eq!(
        c.matches("forward_to = [loki.write.default.receiver]")
            .count(),
        1,
        "{c}"
    );
}

#[test]
fn quotes_and_backslashes_are_escaped_in_the_generated_config() {
    let c = render(&["say \"hi\" \\d+"]);
    assert!(c.contains("expression = \"say \\\"hi\\\" \\\\d+\""), "{c}");
}

#[test]
fn an_entry_that_is_empty_multiline_or_not_a_regex_is_refused() {
    assert!(problems(&[""]).contains("log_drop"), "empty");
    assert!(problems(&["   "]).contains("log_drop"), "blank");
    assert!(problems(&["a\nb"]).contains("log_drop"), "newline");
    assert!(problems(&["a\u{0}b"]).contains("log_drop"), "control char");
    assert!(problems(&["(unclosed"]).contains("log_drop"), "bad regex");
    assert!(problems(&["ok", "ok"]).contains("log_drop"), "duplicate");
}

#[test]
fn a_plain_phrase_with_quotes_is_accepted() {
    assert_eq!(problems(&["it said \"no\" to me"]), "");
}

#[test]
fn the_key_is_invisible_to_the_hash_when_empty() {
    let spec = |m: StackManifest| DeploySpec {
        secret_files: Vec::new(),
        backup_first: false,
        client_schema: CURRENT_CLIENT_SCHEMA,
        source: None,
        native_binaries: Default::default(),
        native_manifests: Default::default(),
        manifest: m,
        files: Vec::new(),
        env: Default::default(),
        extra_routes: Vec::new(),
        gateway_route: None,
        checks: Default::default(),
    };
    let none = manifest_with(&[]);
    let json = serde_json::to_string(&none).unwrap();
    assert!(
        !json.contains("log_drop"),
        "empty list must not serialize: {json}"
    );
    // The serialized manifest is the hash input; adding the key moves it.
    let with = manifest_with(&["x"]);
    assert!(serde_json::to_string(&with).unwrap().contains("log_drop"));
    assert_ne!(intent_hash(&spec(none)), intent_hash(&spec(with)));
}
