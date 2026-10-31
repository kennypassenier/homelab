//! Validation shared by every plan and commit: what the staged stack
//! directory holds, checked with the same functions `homelab plan` and the
//! deploy use, and the other stacks' numbers and addresses it must not
//! collide with.

use std::collections::BTreeMap;
use std::path::Path;

use homelab_proto::StackManifest;

use crate::core::stackedit::MANIFEST;
use crate::shell::workcopy::WorkingCopy;

/// Another stack's number and address, for the collision check.
#[derive(Debug, Clone)]
pub(super) struct Other {
    pub(super) name: String,
    pub(super) vmid: u16,
    pub(super) ip: String,
}

pub(super) fn others(wc: &WorkingCopy, except: &str) -> Vec<Other> {
    wc.stack_names()
        .into_iter()
        .filter(|n| n != except)
        .filter_map(|n| {
            let m = homelab_client::spec::build_manifest(&wc.repo.join("stacks").join(&n)).ok()?;
            Some(Other {
                name: n,
                vmid: m.vmid,
                ip: bare_ip(&m.network.ip),
            })
        })
        .collect()
}

pub(super) fn bare_ip(cidr: &str) -> String {
    cidr.split('/').next().unwrap_or(cidr).to_string()
}

/// What the staged stack directory holds, checked with the same functions
/// `homelab plan` and the deploy use.
pub(super) struct Checked {
    pub(super) problems: Vec<String>,
    pub(super) manifest: Option<StackManifest>,
    /// Every file a deploy would send → sha256 (`homelab check`'s digest).
    pub(super) digest: Option<BTreeMap<String, String>>,
}

pub(super) fn check_dir(dir: &Path, stack: &str, others: &[Other]) -> Checked {
    let mut problems = Vec::new();
    let mut manifest = None;
    if dir.join(MANIFEST).exists() {
        manifest = check_manifest(dir, stack, others, &mut problems);
        if let Err(e) = homelab_client::spec::route_files(dir) {
            problems.push(e);
        }
    }
    for (rel, text) in crate::shell::workcopy::read_texts(dir) {
        check_file(stack, &rel, &text, &mut problems);
    }
    let digest = homelab_client::spec::stack_digest(dir)
        .ok()
        .map(|d| d.files);
    Checked {
        problems,
        manifest,
        digest,
    }
}

/// The manifest builds, then reads as a valid one next to the fleet.
fn check_manifest(
    dir: &Path,
    stack: &str,
    others: &[Other],
    problems: &mut Vec<String>,
) -> Option<StackManifest> {
    match homelab_client::spec::build_manifest(dir) {
        Err(e) => {
            problems.push(e);
            None
        }
        Ok(m) => {
            manifest_problems(&m, stack, others, problems);
            Some(m)
        }
    }
}

fn manifest_problems(m: &StackManifest, stack: &str, others: &[Other], problems: &mut Vec<String>) {
    if m.stack_name != stack {
        problems.push(format!(
            "stack_name is {:?}, the directory is {stack:?}; they must be the same",
            m.stack_name
        ));
    }
    if let Err(e) = homelab_core::manifest::validate_manifest(m) {
        problems.push(e.to_string());
    }
    let gateway = homelab_core::safety::SafetyConfig::default().gateway_vmid;
    if m.vmid == gateway
        && let Some(fw) = m.firewall.as_ref().filter(|f| f.enabled)
    {
        problems.extend(homelab_core::firewall::gateway_problems(fw));
    }
    for o in others {
        if o.vmid == m.vmid {
            problems.push(format!("vmid {} is {}'s already", m.vmid, o.name));
        }
        if o.ip == bare_ip(&m.network.ip) {
            problems.push(format!("address {} is {}'s already", o.ip, o.name));
        }
    }
}

/// One staged file: YAML reads as YAML, and the two schema files read as
/// theirs.
fn check_file(stack: &str, rel: &str, text: &str, problems: &mut Vec<String>) {
    if (rel.ends_with("docker-compose.yml") || rel.ends_with(".yml") || rel.ends_with(".yaml"))
        && let Err(e) = serde_yaml::from_str::<serde_yaml::Value>(text)
    {
        problems.push(format!("stacks/{stack}/{rel} does not read as YAML: {e}"));
    }
    if rel.ends_with("service.yml") {
        check_native(stack, rel, text, problems);
    }
    if rel.ends_with("checks.yml") {
        // serde_yaml's Display carries "at line N column M" when the
        // file has one, which is all the file/line the parser can give.
        // Schema only, same as service.yml above: a shortcoming (no
        // check reaching the application layer, a missing blind_spot)
        // is judged elsewhere (checks::shortcomings, the checks page)
        // and does not block a save here.
        if let Err(e) = serde_yaml::from_str::<homelab_core::checks::ServiceChecks>(text) {
            problems.push(format!("stacks/{stack}/{rel}: {e}"));
        }
    }
}

fn check_native(stack: &str, rel: &str, text: &str, problems: &mut Vec<String>) {
    match serde_yaml::from_str::<homelab_proto::NativeServiceManifest>(text) {
        Ok(n) => {
            if let Err(p) = homelab_core::native::validate_native(&n) {
                problems.push(format!("stacks/{stack}/{rel}: {}", p.join("; ")));
            }
        }
        Err(e) => problems.push(format!("stacks/{stack}/{rel}: {e}")),
    }
}
