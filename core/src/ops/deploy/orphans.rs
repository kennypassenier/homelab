//! Files on the container that the stack no longer declares (D84, ask-8).

use crate::executor::{pct_sh, shq};
use crate::manifest::DeploySpec;

/// Files under `/opt/<stack>/` on the container that this deploy did not send.
///
/// Compared against the spec's own file list, so the answer is "what the
/// repository no longer has" rather than "what looks old". Best-effort: a
/// container that will not answer produces an empty list rather than a
/// finding, because an unasked question must never become one.
///
/// `.env` files are excluded on purpose — they come from the host vault
/// (D12), never from the repository, so every one of them would be reported
/// as an orphan forever.
pub async fn orphan_files(
    exec: &dyn crate::executor::Executor,
    m: &crate::manifest::StackManifest,
    spec: &DeploySpec,
) -> Vec<String> {
    orphan_files_keeping(exec, m, spec, &[]).await
}

/// Directories inside container `vmid` that the orchestrator fills on behalf
/// of OTHER stacks: on the gateway, the route fragments (H1). Their files
/// are in no stack's file list, so without this a deploy of the gateway
/// would take them for its own orphans and remove every other stack's route.
pub fn generated_dirs(safety: &crate::safety::SafetyConfig, vmid: u16) -> Vec<String> {
    let mut v = Vec::new();
    if vmid == safety.gateway_vmid {
        v.push(safety.gateway_routes_dir.clone());
    }
    v
}

/// [`orphan_files`], leaving out anything under the `keep` directories
/// (absolute paths inside the container, see [`generated_dirs`]).
pub async fn orphan_files_keeping(
    exec: &dyn crate::executor::Executor,
    m: &crate::manifest::StackManifest,
    spec: &DeploySpec,
    keep: &[String],
) -> Vec<String> {
    let sent: std::collections::BTreeSet<&str> =
        spec.files.iter().map(|f| f.path.as_str()).collect();
    let kept = |rel: &str| {
        let abs = format!("/opt/{}/{}", m.stack_name, rel);
        keep.iter()
            .any(|d| abs.starts_with(&format!("{}/", d.trim_end_matches('/'))))
    };
    let out = match pct_sh(
        exec,
        m.vmid,
        &format!(
            "cd {0} 2>/dev/null && find . -type f -printf '%P\\n' 2>/dev/null || true",
            shq(&format!("/opt/{}", m.stack_name))
        ),
        120,
    )
    .await
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    out.stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !l.ends_with(".env"))
        .filter(|l| !sent.contains(*l))
        .filter(|l| !kept(l))
        .map(str::to_string)
        .collect()
}
