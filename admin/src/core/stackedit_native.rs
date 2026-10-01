//! feat-native-1 (D): the native-service form — editing one native unit's
//! `service.yml`, adding a new unit, removing one. A native service (C7)
//! has no compose file; its whole shape is `service.yml`
//! (`NativeServiceManifest`) plus a systemd unit beside it
//! (`<unit>/<unit>.service`, wherever `service.yml` itself sits — F301: a
//! stack's first native keeps it at the stack's root, every other one
//! takes its own `<unit>/` directory).
//!
//! Mirrors `stackedit`'s settings/firewall ops: comments in `service.yml`
//! are kept by editing the text with `yamledit`, not by round-tripping
//! through serde. Pure: current texts in, new texts out; the shell
//! validates with `homelab_core::native::validate_native` the same way it
//! validates every other `service.yml` (`admin/src/shell/edit.rs`'s
//! `check_dir`), so this module does not repeat that check.

use homelab_core::native::{BackupPause, UpdatePolicy};
use homelab_proto::{NativeServiceManifest, StackManifest};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use super::stackedit::StackTexts;
use super::yamledit::{Item, Op, path};

/// The `service.yml` text of one native unit of a stack, wherever it lives.
pub fn native_path(texts: &StackTexts, unit: &str) -> Option<String> {
    if let Some(t) = texts.get("service.yml")
        && serde_yaml::from_str::<NativeServiceManifest>(t)
            .map(|m| m.unit == unit)
            .unwrap_or(false)
    {
        return Some("service.yml".to_string());
    }
    let sub = format!("{unit}/service.yml");
    texts.contains_key(&sub).then_some(sub)
}

/// The unit file's path: always `<unit>/<unit>.service`.
pub fn unit_file_path(unit: &str) -> String {
    format!("{unit}/{unit}.service")
}

/// One native unit, for the form's first read: its manifest, if the text
/// reads as one (a broken `service.yml` still gets a row, with `manifest:
/// null`, so the raw editor is reachable).
#[derive(Debug, Clone, Serialize)]
pub struct NativeView {
    pub unit: String,
    pub path: String,
    pub unit_file_path: String,
    pub manifest: Option<NativeServiceManifest>,
}

/// Every native unit the manifest's `natives:` names that the texts can
/// locate, in declared order.
pub fn native_views(texts: &StackTexts, natives: &[String]) -> Vec<NativeView> {
    natives
        .iter()
        .filter_map(|unit| {
            let p = native_path(texts, unit)?;
            let manifest = texts.get(&p).and_then(|t| serde_yaml::from_str(t).ok());
            Some(NativeView {
                unit: unit.clone(),
                unit_file_path: unit_file_path(unit),
                path: p,
                manifest,
            })
        })
        .collect()
}

/// feat-native-1: edit one native unit's `service.yml`. A field left out is
/// left as it is; for an optional one, an empty string clears it (the key
/// is removed — absent is what `NativeServiceManifest` reads as "not
/// set"). `unit` picks which of the stack's native units this edits.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEdit {
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dirs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_cmd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stateless: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_from_newest: Option<String>,
    /// fix-146: the command run inside the container to re-seed the live
    /// store after a restore, before the unit starts. Empty string clears
    /// it, same as every other optional text field here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_restore: Option<String>,
    /// The select sends `false`, `true` or `chassis`, read back the same
    /// way `service.yml` itself does (fix-113, owner decision 2026-10-01).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_pause: Option<BackupPause>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_policy: Option<UpdatePolicy>,
    /// The metrics picker's choice; absent = leave it as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<MetricsChoice>,
}

/// The metrics field's picker: `Measured` clears the key (absent means
/// measured, `core/src/native.rs`) rather than writing `metrics: true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricsChoice {
    Measured,
    NotMeasured,
}

/// feat-native-1: a new native unit's `service.yml`, always written under
/// `<unit>/` — a second native cannot also claim the stack's root, so every
/// added unit takes the subdirectory, whether or not the stack already had
/// one at the root.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AddNativeEdit {
    pub unit: String,
    pub binary: String,
    #[serde(default)]
    pub env_file: Option<String>,
    #[serde(default)]
    pub data_dirs: Vec<String>,
    #[serde(default)]
    pub stateless: bool,
    #[serde(default)]
    pub update_cmd: Option<String>,
    #[serde(default)]
    pub release_repo: Option<String>,
    #[serde(default)]
    pub release_asset: Option<String>,
    /// systemd's `Type=`: `notify` for a unit that signals readiness (the
    /// chassis-rs kit's shape, and the form's default), `exec` for one that
    /// does not.
    #[serde(default)]
    pub notify: bool,
}

/// Trim, then `None` when empty — the edit's "" clears an optional field.
fn cleared(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn set_or_remove(ops: &mut Vec<Op>, key: &str, old: Option<&str>, want: Option<&str>) {
    let Some(want) = want else { return };
    let want = want.trim();
    let new = (!want.is_empty()).then_some(want);
    if new == old {
        return;
    }
    match new {
        Some(v) => ops.push(Op::Set {
            path: path(key),
            value: Value::from(v),
        }),
        None => ops.push(Op::Remove { path: path(key) }),
    }
}

/// `backup_pause`'s three states, written the way `service.yml` itself
/// writes them: a plain bool for `Off`/`Unit`, the string `chassis` for
/// `Chassis` — the same shape `BackupPause`'s own `Serialize` produces, but
/// `yamledit`'s ops work on `serde_yaml::Value` directly rather than through
/// serde.
fn backup_pause_value(v: BackupPause) -> Value {
    match v {
        BackupPause::Off => Value::from(false),
        BackupPause::Unit => Value::from(true),
        BackupPause::Chassis => Value::from("chassis"),
    }
}

/// The ops that turn `old` into what `e` asks, on `old`'s own text.
pub fn native_ops(old: &NativeServiceManifest, e: &NativeEdit) -> Vec<Op> {
    let mut ops = Vec::new();
    if let Some(b) = e
        .binary
        .as_deref()
        .map(str::trim)
        .filter(|b| *b != old.binary)
    {
        ops.push(Op::Set {
            path: path("binary"),
            value: Value::from(b),
        });
    }
    set_or_remove(
        &mut ops,
        "env_file",
        old.env_file.as_deref(),
        e.env_file.as_deref(),
    );
    if let Some(dirs) = &e.data_dirs {
        let dirs: Vec<String> = dirs
            .iter()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty())
            .collect();
        if dirs != old.data_dirs {
            ops.push(Op::Seq {
                path: path("data_dirs"),
                items: dirs
                    .into_iter()
                    .map(|d| Item::New(Value::from(d)))
                    .collect(),
            });
        }
    }
    set_or_remove(
        &mut ops,
        "update_cmd",
        old.update_cmd.as_deref(),
        e.update_cmd.as_deref(),
    );
    if let Some(v) = e.stateless.filter(|v| *v != old.stateless) {
        ops.push(Op::Set {
            path: path("stateless"),
            value: Value::from(v),
        });
    }
    set_or_remove(
        &mut ops,
        "restore_note",
        old.restore_note.as_deref(),
        e.restore_note.as_deref(),
    );
    set_or_remove(
        &mut ops,
        "release_repo",
        old.release_repo.as_deref(),
        e.release_repo.as_deref(),
    );
    set_or_remove(
        &mut ops,
        "release_asset",
        old.release_asset.as_deref(),
        e.release_asset.as_deref(),
    );
    set_or_remove(
        &mut ops,
        "backup_from_newest",
        old.backup_from_newest.as_deref(),
        e.backup_from_newest.as_deref(),
    );
    set_or_remove(
        &mut ops,
        "after_restore",
        old.after_restore.as_deref(),
        e.after_restore.as_deref(),
    );
    if let Some(v) = e.backup_pause.filter(|v| *v != old.backup_pause) {
        ops.push(Op::Set {
            path: path("backup_pause"),
            value: backup_pause_value(v),
        });
    }
    if let Some(v) = e.update_policy.filter(|v| *v != old.update_policy) {
        let word = match v {
            UpdatePolicy::Manual => "manual",
            UpdatePolicy::Auto => "auto",
            UpdatePolicy::OwnVerb => "self",
        };
        ops.push(Op::Set {
            path: path("update_policy"),
            value: Value::from(word),
        });
    }
    if let Some(choice) = e.metrics {
        match choice {
            MetricsChoice::Measured => {
                if old.metrics.is_some() {
                    ops.push(Op::Remove {
                        path: path("metrics"),
                    });
                }
            }
            MetricsChoice::NotMeasured => {
                if old.metrics != Some(false) {
                    ops.push(Op::Set {
                        path: path("metrics"),
                        value: Value::from(false),
                    });
                }
            }
        }
    }
    ops
}

/// A new native unit's `service.yml` text and its systemd unit file. There
/// are no comments to keep (the file does not exist yet), so this writes
/// the manifest with a plain serialization rather than `yamledit`.
pub fn add_native_files(stack: &StackManifest, a: &AddNativeEdit) -> (String, String) {
    let m = NativeServiceManifest {
        stack_name: stack.stack_name.clone(),
        vmid: stack.vmid,
        hostname: stack.hostname.clone(),
        unit: a.unit.trim().to_string(),
        binary: a.binary.trim().to_string(),
        env_file: cleared(a.env_file.clone()),
        data_dirs: a
            .data_dirs
            .iter()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty())
            .collect(),
        update_cmd: cleared(a.update_cmd.clone()),
        stateless: a.stateless,
        restore_note: None,
        release_repo: cleared(a.release_repo.clone()),
        release_asset: cleared(a.release_asset.clone()),
        backup_from_newest: None,
        backup_pause: BackupPause::Off,
        update_policy: UpdatePolicy::Manual,
        after_restore: None,
        metrics: None,
    };
    let yml = serde_yaml::to_string(&m).unwrap_or_default();
    let unit_file = unit_template(&m, a.notify);
    (yml, unit_file)
}

/// A generic systemd unit for a native service, built only from the form's
/// own fields — no app knowledge: the shape every unit in `stacks/*/<unit>/`
/// shares (`Type=`, the start-limit and hardening block), with nothing that
/// names a particular service.
fn unit_template(m: &NativeServiceManifest, notify: bool) -> String {
    let working_dir = m
        .env_file
        .as_deref()
        .and_then(|p| p.rsplit_once('/').map(|(d, _)| d))
        .or_else(|| m.data_dirs.first().map(String::as_str))
        .unwrap_or("/");
    let mut s = String::new();
    s.push_str("[Unit]\n");
    s.push_str(&format!("Description={} — a native service\n", m.unit));
    s.push_str("Wants=network-online.target\n");
    s.push_str("After=network-online.target\n");
    s.push_str("StartLimitIntervalSec=0\n");
    s.push_str("StartLimitBurst=0\n");
    s.push('\n');
    s.push_str("[Service]\n");
    s.push_str(&format!(
        "Type={}\n",
        if notify { "notify" } else { "exec" }
    ));
    s.push_str(&format!("User={}\n", m.unit));
    s.push_str(&format!("Group={}\n", m.unit));
    if let Some(env) = &m.env_file {
        s.push_str(&format!("EnvironmentFile={env}\n"));
    }
    s.push_str(&format!("WorkingDirectory={working_dir}\n"));
    if notify {
        s.push_str(&format!("ExecStartPre={} --check\n", m.binary));
    }
    s.push_str(&format!("ExecStart={}\n", m.binary));
    s.push_str("Restart=always\n");
    s.push_str("RestartSec=5s\n");
    s.push_str("KillSignal=SIGTERM\n");
    s.push_str("TimeoutStopSec=60\n");
    s.push_str("NoNewPrivileges=yes\n");
    s.push_str("PrivateTmp=yes\n");
    s.push_str("ProtectSystem=strict\n");
    s.push_str("ProtectHome=yes\n");
    s.push_str("ProtectKernelTunables=yes\n");
    s.push_str("ProtectControlGroups=yes\n");
    s.push_str("RestrictSUIDSGID=yes\n");
    if !m.data_dirs.is_empty() {
        s.push_str(&format!("ReadWritePaths={}\n", m.data_dirs.join(" ")));
    }
    s.push_str("UMask=0077\n");
    s.push_str("CapabilityBoundingSet=\n");
    s.push_str("RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX\n");
    s.push_str("SystemCallFilter=@system-service\n");
    s.push_str("SystemCallArchitectures=native\n");
    s.push_str("RestrictNamespaces=yes\n");
    s.push_str("LockPersonality=yes\n");
    s.push_str("MemoryDenyWriteExecute=yes\n");
    s.push_str("ProtectKernelModules=yes\n");
    s.push_str("ProtectKernelLogs=yes\n");
    s.push_str("ProtectClock=yes\n");
    s.push_str("ProtectHostname=yes\n");
    s.push_str("PrivateDevices=yes\n");
    s.push_str("ProtectProc=invisible\n");
    s.push('\n');
    s.push_str("[Install]\n");
    s.push_str("WantedBy=multi-user.target\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> NativeServiceManifest {
        NativeServiceManifest {
            stack_name: "example".into(),
            vmid: 150,
            hostname: "150-app-example".into(),
            unit: "example".into(),
            binary: "/opt/example/bin/example".into(),
            env_file: Some("/appdata/example/example-config/example.env".into()),
            data_dirs: vec!["/appdata/example/example-config".into()],
            update_cmd: None,
            stateless: false,
            restore_note: None,
            release_repo: None,
            release_asset: None,
            backup_from_newest: None,
            backup_pause: BackupPause::Off,
            update_policy: UpdatePolicy::Manual,
            after_restore: None,
            metrics: None,
        }
    }

    #[test]
    fn no_edit_is_no_ops() {
        assert!(native_ops(&base(), &NativeEdit::default()).is_empty());
    }

    #[test]
    fn empty_string_clears_optional_field() {
        let old = base();
        let e = NativeEdit {
            env_file: Some(String::new()),
            ..Default::default()
        };
        let ops = native_ops(&old, &e);
        assert_eq!(
            ops,
            vec![Op::Remove {
                path: path("env_file"),
            }]
        );
    }

    #[test]
    fn metrics_measured_clears_the_key() {
        let mut old = base();
        old.metrics = Some(false);
        let e = NativeEdit {
            after_restore: None,
            metrics: Some(MetricsChoice::Measured),
            ..Default::default()
        };
        let ops = native_ops(&old, &e);
        assert_eq!(
            ops,
            vec![Op::Remove {
                path: path("metrics"),
            }]
        );
    }

    #[test]
    fn add_native_files_have_no_secrets_and_no_app_name() {
        let stack = StackManifest {
            stack_name: "example".into(),
            vmid: 150,
            hostname: "150-app-example".into(),
            ..stack_manifest_default()
        };
        let a = AddNativeEdit {
            unit: "worker".into(),
            binary: "/opt/worker/bin/worker".into(),
            env_file: Some("/appdata/example/worker-config/worker.env".into()),
            data_dirs: vec!["/appdata/example/worker-config".into()],
            notify: true,
            ..Default::default()
        };
        let (yml, unit) = add_native_files(&stack, &a);
        assert!(yml.contains("unit: worker"));
        assert!(unit.contains("ExecStart=/opt/worker/bin/worker"));
        assert!(unit.contains("Type=notify"));
    }

    /// A bare-minimum manifest for the test above; every field the test
    /// does not care about at its type's default.
    fn stack_manifest_default() -> StackManifest {
        serde_yaml::from_str(
            "stack_name: x\nvmid: 1\nhostname: h\n\
             network: {ip: 10.0.0.1/24, gateway: 10.0.0.1}\n\
             resources: {cores: 1, memory_mb: 128, disk_gb: 2}\n\
             lxc: {template: t}\n\
             boot: {}\n\
             apps: []\n\
             natives: []\n",
        )
        .expect("a minimal manifest reads")
    }
}
