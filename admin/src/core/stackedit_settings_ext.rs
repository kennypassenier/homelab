//! feat-stacks-9 (Area C, 2026-09-30): the settings form's second page —
//! network, the lxc flags, resources.storage, on_demand and the stack's own
//! retention — kept apart from [`super::stackedit::SettingsEdit`] (cores,
//! memory, boot, protection, images, tiles) so neither form's ops function
//! grows past what one page of the dashboard needs. Pure: the manifest and
//! the wanted values come in, the ops that turn one into the other go out.

use homelab_core::retention::RetentionTier;
use homelab_proto::StackManifest;
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use super::yamledit::{Op, path};

/// A field left out is left as it is — the same rule `SettingsEdit` uses.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsExtEdit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vlan: Option<u16>,
    /// H4/O5: changing this only ever takes effect at a rebuild (`pct
    /// create`/`pct clone` are the only places it is set); a deploy of a
    /// running container leaves it exactly as it is. The plan says so
    /// (`editplan::effects`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unprivileged: Option<bool>,
    /// H4: same as `unprivileged` — the device is passed in at creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vpn: Option<bool>,
    /// T62: same as `unprivileged`/`gpu`/`vpn` — applied by `pct create`
    /// only, so a deploy of a running container leaves it exactly as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_demand: Option<bool>,
    /// W2. None: leave alone. `Some(&[])`: clear the stack's own tiers, back
    /// to the fleet-wide policy. `Some(tiers)`: this stack's own policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention: Option<Vec<RetentionTierEdit>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionTierEdit {
    pub every_days: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_days: Option<u32>,
}

fn to_tier(e: &RetentionTierEdit) -> RetentionTier {
    RetentionTier {
        every_days: e.every_days,
        span_days: e.span_days,
    }
}

fn dotted_octets(s: &str) -> Option<[u16; 4]> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut out = [0u16; 4];
    for (i, p) in parts.iter().enumerate() {
        let n: u16 = p.parse().ok()?;
        if n > 255 || (p.len() > 1 && p.starts_with('0')) {
            return None;
        }
        out[i] = n;
    }
    Some(out)
}

/// `10.10.10.10/24`: four octets, a slash, a prefix 0 to 32.
fn valid_cidr(s: &str) -> bool {
    let Some((addr, prefix)) = s.split_once('/') else {
        return false;
    };
    dotted_octets(addr).is_some() && prefix.parse::<u8>().is_ok_and(|p| p <= 32)
}

/// A bare address, no prefix (the gateway).
fn valid_ip(s: &str) -> bool {
    !s.contains('/') && dotted_octets(s).is_some()
}

/// A Linux interface name: 1 to 15 bytes (IFNAMSIZ), no `/` or whitespace.
fn valid_bridge(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 15
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// A timezone argument to `pct create --timezone`: `host`, or an IANA zone
/// name (`Europe/Amsterdam`, `UTC`) — letters, digits, `/`, `+`, `-`, `_`.
fn valid_timezone(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'+' | b'-' | b'_'))
}

/// A Proxmox storage id: letters, digits, `-` and `_`.
fn valid_storage_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// Everything this form keeps to before any text is touched; the deeper
/// checks (CIDR shape, a vmid or address another stack already has) run
/// afterwards on the staged manifest the same way every other edit's do
/// (`edit::check_dir`).
pub fn problems(s: &SettingsExtEdit) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(ip) = &s.ip
        && !valid_cidr(ip)
    {
        out.push(format!(
            "network ip {ip:?} must be CIDR, e.g. 10.10.10.10/24"
        ));
    }
    if let Some(gw) = &s.gateway
        && !valid_ip(gw)
    {
        out.push(format!("gateway {gw:?} must be an address like 10.10.10.1"));
    }
    if let Some(b) = &s.bridge
        && !valid_bridge(b)
    {
        out.push(format!(
            "bridge {b:?} must be 1 to 15 letters, digits, '-' or '_', like vmbr0"
        ));
    }
    if let Some(v) = s.vlan
        && !(1..=4094).contains(&v)
    {
        out.push(format!("vlan {v} must be 1 to 4094"));
    }
    if let Some(st) = &s.storage
        && !valid_storage_id(st)
    {
        out.push(format!(
            "storage {st:?} must be letters, digits, '-' or '_', like local-lvm"
        ));
    }
    if let Some(tz) = &s.timezone
        && !valid_timezone(tz)
    {
        out.push(format!(
            "timezone {tz:?} must be \"host\" or an IANA zone like Europe/Amsterdam"
        ));
    }
    if let Some(tiers) = &s.retention {
        for t in tiers {
            if !(1..=3650).contains(&t.every_days) {
                out.push(format!(
                    "retention: every {} days must be 1 to 3650",
                    t.every_days
                ));
            }
            if let Some(span) = t.span_days {
                if !(1..=3650).contains(&span) {
                    out.push(format!("retention: span {span} days must be 1 to 3650"));
                }
                if span < t.every_days {
                    out.push(format!(
                        "retention: a tier kept for {span} days must span at least its own \
                         every-{} days",
                        t.every_days
                    ));
                }
            }
        }
    }
    out
}

fn retention_value(tiers: &[RetentionTierEdit]) -> Value {
    Value::Sequence(
        tiers
            .iter()
            .map(|t| {
                let mut m = Mapping::new();
                m.insert("every_days".into(), Value::from(t.every_days));
                if let Some(span) = t.span_days {
                    m.insert("span_days".into(), Value::from(span));
                }
                Value::Mapping(m)
            })
            .collect(),
    )
}

/// The ops this page's form asks for, against the manifest as it reads now.
pub fn ops(m: &StackManifest, s: &SettingsExtEdit) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut set_str = |key: &str, now: &str, want: &Option<String>| {
        if let Some(w) = want.as_deref().filter(|w| *w != now) {
            ops.push(Op::Set {
                path: path(key),
                value: Value::from(w),
            });
        }
    };
    set_str("network.ip", &m.network.ip, &s.ip);
    set_str("network.gateway", &m.network.gateway, &s.gateway);
    set_str("network.bridge", &m.network.bridge, &s.bridge);
    set_str("resources.storage", &m.resources.storage, &s.storage);
    set_str("lxc.timezone", &m.lxc.timezone, &s.timezone);
    if let Some(v) = s.vlan.filter(|v| m.network.vlan != Some(*v)) {
        ops.push(Op::Set {
            path: path("network.vlan"),
            value: Value::from(v),
        });
    }
    let mut set_bool = |key: &str, now: bool, want: Option<bool>| {
        if let Some(w) = want.filter(|w| *w != now) {
            ops.push(Op::Set {
                path: path(key),
                value: Value::from(w),
            });
        }
    };
    set_bool("lxc.unprivileged", m.lxc.unprivileged, s.unprivileged);
    set_bool("lxc.gpu", m.lxc.gpu, s.gpu);
    set_bool("lxc.vpn", m.lxc.vpn, s.vpn);
    set_bool("on_demand", m.on_demand, s.on_demand);
    if let Some(tiers) = &s.retention {
        let now: Option<Vec<RetentionTier>> = m.retention.clone();
        if tiers.is_empty() {
            if now.is_some() {
                ops.push(Op::Remove {
                    path: path("retention"),
                });
            }
        } else {
            let want: Vec<RetentionTier> = tiers.iter().map(to_tier).collect();
            if now.as_deref() != Some(want.as_slice()) {
                ops.push(Op::Set {
                    path: path("retention"),
                    value: retention_value(tiers),
                });
            }
        }
    }
    ops
}

/// The change in a few words, for the commit subject.
pub fn describe(s: &SettingsExtEdit, m: &StackManifest) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(v) = &s.ip
        && *v != m.network.ip
    {
        parts.push(format!("address {v}"));
    }
    if let Some(v) = &s.gateway
        && *v != m.network.gateway
    {
        parts.push(format!("gateway {v}"));
    }
    if let Some(v) = &s.bridge
        && *v != m.network.bridge
    {
        parts.push(format!("bridge {v}"));
    }
    if let Some(v) = s.vlan.filter(|v| m.network.vlan != Some(*v)) {
        parts.push(format!("vlan {v}"));
    }
    if let Some(v) = s.unprivileged.filter(|v| *v != m.lxc.unprivileged) {
        parts.push(format!(
            "{} (rebuild needed)",
            if v { "unprivileged" } else { "privileged" }
        ));
    }
    if let Some(v) = s.gpu.filter(|v| *v != m.lxc.gpu) {
        parts.push(format!(
            "gpu {} (rebuild needed)",
            if v { "on" } else { "off" }
        ));
    }
    if let Some(v) = s.vpn.filter(|v| *v != m.lxc.vpn) {
        parts.push(format!(
            "vpn {} (rebuild needed)",
            if v { "on" } else { "off" }
        ));
    }
    if let Some(v) = &s.storage
        && *v != m.resources.storage
    {
        parts.push(format!("storage {v} (rebuild needed)"));
    }
    if let Some(v) = &s.timezone
        && *v != m.lxc.timezone
    {
        parts.push(format!("timezone {v} (rebuild needed)"));
    }
    if let Some(v) = s.on_demand.filter(|v| *v != m.on_demand) {
        parts.push(format!("on demand {}", if v { "on" } else { "off" }));
    }
    if let Some(tiers) = &s.retention {
        if tiers.is_empty() {
            parts.push("retention back to the fleet default".into());
        } else {
            parts.push(format!("retention: {} tier(s) of its own", tiers.len()));
        }
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> StackManifest {
        serde_yaml::from_str(
            r#"
stack_name: demo
vmid: 200
hostname: 200-app-demo
network: { ip: "10.10.10.10/24", gateway: "10.10.10.1", bridge: vmbr0 }
resources: { cores: 2, memory_mb: 1024, disk_gb: 8 }
lxc: { template: "local:vztmpl/x.tar.zst" }
boot: {}
apps: []
"#,
        )
        .unwrap()
    }

    #[test]
    fn valid_cidr_accepts_and_rejects() {
        assert!(valid_cidr("10.10.10.10/24"));
        assert!(!valid_cidr("10.10.10.10"));
        assert!(!valid_cidr("10.10.10.256/24"));
        assert!(!valid_cidr("10.10.10.10/33"));
        assert!(!valid_cidr("not-an-ip/24"));
    }

    #[test]
    fn unchanged_fields_make_no_ops() {
        let m = manifest();
        let edit = SettingsExtEdit {
            ip: Some("10.10.10.10/24".into()),
            unprivileged: Some(true),
            ..Default::default()
        };
        assert!(ops(&m, &edit).is_empty());
    }

    #[test]
    fn changed_network_and_lxc_flags_emit_ops() {
        let m = manifest();
        let edit = SettingsExtEdit {
            ip: Some("10.10.10.20/24".into()),
            vlan: Some(30),
            gpu: Some(true),
            on_demand: Some(true),
            ..Default::default()
        };
        let got = ops(&m, &edit);
        assert_eq!(got.len(), 4);
        let words = describe(&edit, &m);
        assert!(words.iter().any(|w| w.contains("10.10.10.20/24")));
        assert!(words.iter().any(|w| w.contains("gpu on (rebuild needed)")));
    }

    #[test]
    fn retention_empty_clears_an_override() {
        let mut m = manifest();
        m.retention = Some(vec![RetentionTier {
            every_days: 1,
            span_days: Some(7),
        }]);
        let edit = SettingsExtEdit {
            retention: Some(vec![]),
            ..Default::default()
        };
        let got = ops(&m, &edit);
        assert_eq!(
            got,
            vec![Op::Remove {
                path: path("retention")
            }]
        );
    }

    #[test]
    fn retention_problems_catch_bad_spans() {
        let edit = SettingsExtEdit {
            retention: Some(vec![RetentionTierEdit {
                every_days: 10,
                span_days: Some(3),
            }]),
            ..Default::default()
        };
        let p = problems(&edit);
        assert_eq!(p.len(), 1);
        assert!(p[0].contains("span"));
    }

    #[test]
    fn bridge_and_storage_id_charset() {
        assert!(valid_bridge("vmbr0"));
        assert!(!valid_bridge("vmbr 0"));
        assert!(!valid_bridge(""));
        assert!(valid_storage_id("local-lvm"));
        assert!(!valid_storage_id("local lvm"));
    }

    #[test]
    fn timezone_charset() {
        assert!(valid_timezone("host"));
        assert!(valid_timezone("Europe/Amsterdam"));
        assert!(valid_timezone("Etc/GMT+1"));
        assert!(!valid_timezone(""));
        assert!(!valid_timezone("not a zone"));
    }

    #[test]
    fn timezone_edit_emits_a_rebuild_warning() {
        let m = manifest();
        assert_eq!(m.lxc.timezone, "host");
        let edit = SettingsExtEdit {
            timezone: Some("Europe/Amsterdam".into()),
            ..Default::default()
        };
        let got = ops(&m, &edit);
        assert_eq!(
            got,
            vec![Op::Set {
                path: path("lxc.timezone"),
                value: Value::from("Europe/Amsterdam"),
            }]
        );
        let words = describe(&edit, &m);
        assert!(
            words
                .iter()
                .any(|w| w.contains("timezone Europe/Amsterdam (rebuild needed)"))
        );
    }
}
