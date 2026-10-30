//! Whether this binary is a signed release, verified once against the recorded signature.

/// redesign-host-4: the running binary (`/proc/self/exe`, which still reads
/// after the file on disk was replaced) against the signature recorded when
/// it was installed.
pub(crate) fn own_release_verdict(state_dir: &str) -> homelab_proto::ReleaseVerdict {
    match std::fs::read("/proc/self/exe") {
        Ok(exe) => {
            let v = homelab_core::release_sig::running_verdict(
                std::path::Path::new(state_dir),
                SELF_ASSET,
                &exe,
            );
            homelab_proto::ReleaseVerdict {
                signed: v.signed,
                detail: v.detail,
            }
        }
        Err(e) => homelab_proto::ReleaseVerdict {
            signed: false,
            detail: format!("the host could not read its own binary to check it: {e}"),
        },
    }
}

/// The host binary's asset name in a homelab release.
pub(crate) const SELF_ASSET: &str = "homelab-host";
