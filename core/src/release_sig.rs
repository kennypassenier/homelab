//! fix-29: a release is installed only when its `SHA256SUMS` carries the
//! ecosystem's minisign signature.
//!
//! On 2026-09-27 the nightly round installed kyu 4.0.0 at 02:12 UTC, having
//! checked the binary against `SHA256SUMS` only — a file from the same
//! unsigned upload, so the check proved the download and not the author. The
//! signature arrived at 02:59. Checking the signature over `SHA256SUMS`, and
//! the binary against that file, proves both.

/// The ecosystem release key (minisign key id 1C88AB06D43C0B16), the same one
/// chassis-rs compiles into every kit service as `RELEASE_PUBKEY`.
pub const RELEASE_PUBKEY: &str = "RWQWCzzUBquIHGkS3YERMkuqEm4C3vBArnlb9rySbr8z5ytgVYuji3bS";

/// The asset name that carries the signature over `SHA256SUMS`.
pub const SIG_ASSET: &str = "SHA256SUMS.minisig";

/// redesign-host-4: where the host keeps the signature each binary it was
/// installed with carried, under its state directory: `<sha256>.sums` and
/// `<sha256>.minisig` per binary, the newest [`PROOFS_KEPT`] kept (a
/// rollback to the previous binary still finds its own).
pub const PROOF_DIR: &str = "release-proof";

/// How many installed binaries keep their recorded signature.
pub const PROOFS_KEPT: usize = 3;

/// redesign-host-4: whether the running binary is a signed release, as the
/// host verified it itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningVerdict {
    pub signed: bool,
    /// Why, in a sentence a person can act on.
    pub detail: String,
}

/// redesign-host-4: record the signature `binary` was installed with —
/// only after it verifies (the signature over `sums`, `binary` listed as
/// `asset` in it), so nothing unverified is ever recorded.
pub fn record_proof(
    state_dir: &std::path::Path,
    asset: &str,
    binary: &[u8],
    sums: &str,
    sig: &str,
) -> Result<(), String> {
    record_proof_with(RELEASE_PUBKEY, state_dir, asset, binary, sums, sig)
}

/// Same, with an explicit key — for tests.
pub fn record_proof_with(
    pubkey: &str,
    state_dir: &std::path::Path,
    asset: &str,
    binary: &[u8],
    sums: &str,
    sig: &str,
) -> Result<(), String> {
    verify_release_with(pubkey, asset, binary, sums, Some(sig))?;
    let dir = state_dir.join(PROOF_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    let sha = crate::manifest::sha256_hex(binary);
    for (ext, text) in [("sums", sums), ("minisig", sig)] {
        let path = dir.join(format!("{sha}.{ext}"));
        std::fs::write(&path, text).map_err(|e| format!("{}: {}", path.display(), e))?;
    }
    prune_proofs(&dir);
    Ok(())
}

/// Keep the proofs of the newest [`PROOFS_KEPT`] binaries; best effort.
fn prune_proofs(dir: &std::path::Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut sums: Vec<(std::time::SystemTime, std::path::PathBuf)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sums"))
        .map(|p| {
            let at = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (at, p)
        })
        .collect();
    sums.sort_by_key(|s| std::cmp::Reverse(s.0));
    for (_, p) in sums.into_iter().skip(PROOFS_KEPT) {
        let _ = std::fs::remove_file(p.with_extension("minisig"));
        let _ = std::fs::remove_file(&p);
    }
}

/// redesign-host-4: the running binary (`exe`, its own bytes) against the
/// signature recorded when it was installed.
pub fn running_verdict(state_dir: &std::path::Path, asset: &str, exe: &[u8]) -> RunningVerdict {
    running_verdict_with(RELEASE_PUBKEY, state_dir, asset, exe)
}

/// Same, with an explicit key — for tests.
pub fn running_verdict_with(
    pubkey: &str,
    state_dir: &std::path::Path,
    asset: &str,
    exe: &[u8],
) -> RunningVerdict {
    let sha = crate::manifest::sha256_hex(exe);
    let dir = state_dir.join(PROOF_DIR);
    let read = |ext: &str| std::fs::read_to_string(dir.join(format!("{sha}.{ext}"))).ok();
    let (Some(sums), Some(sig)) = (read("sums"), read("minisig")) else {
        return RunningVerdict {
            signed: false,
            detail: "no release signature was recorded for this binary: it was installed \
                     from a file (self-update), by hand, or by a client older than 3.71.0; \
                     `homelab release-update` installs a signed release"
                .into(),
        };
    };
    match verify_release_with(pubkey, asset, exe, &sums, Some(&sig)) {
        Ok(()) => RunningVerdict {
            signed: true,
            detail: "this binary is listed in its release's SHA256SUMS, and that list carries \
                     the release signature; verified by the host itself"
                .into(),
        },
        Err(e) => RunningVerdict {
            signed: false,
            detail: format!("the signature recorded for this binary does not verify: {e}"),
        },
    }
}

/// Verify `sums` against `sig` (the text of `SHA256SUMS.minisig`) with the
/// ecosystem key. Every refusal says what to do.
pub fn verify_sums(sums: &str, sig: &str) -> Result<(), String> {
    verify_sums_with(RELEASE_PUBKEY, sums, sig)
}

/// Same, with an explicit key — for tests, which sign with a throwaway one.
pub fn verify_sums_with(pubkey: &str, sums: &str, sig: &str) -> Result<(), String> {
    let pk = minisign_verify::PublicKey::from_base64(pubkey)
        .map_err(|e| format!("the release key does not parse: {}", e))?;
    let signature = minisign_verify::Signature::decode(sig).map_err(|e| {
        format!(
            "SHA256SUMS.minisig is not a minisign signature ({}) — refusing the release",
            e
        )
    })?;
    pk.verify(sums.as_bytes(), &signature, false).map_err(|e| {
        format!(
            "SHA256SUMS does not carry the ecosystem signature ({}) — refusing the release: \
             it was not signed with key 1C88AB06D43C0B16, or it changed after signing",
            e
        )
    })
}

/// fix-29, for any release asset: the signature over `SHA256SUMS` first,
/// then the asset against that list. `sig` is the text of
/// `SHA256SUMS.minisig`, None when the release carries none, which is
/// refused: an unsigned release is never installed. Used by the client
/// (`release-update`, `self-install`, `install-native`) and by the dashboard
/// ("Update host"), so both refuse the same things with the same words.
pub fn verify_release(
    asset: &str,
    binary: &[u8],
    sums: &str,
    sig: Option<&str>,
) -> Result<(), String> {
    verify_release_with(RELEASE_PUBKEY, asset, binary, sums, sig)
}

/// Same, with an explicit key — for tests.
pub fn verify_release_with(
    pubkey: &str,
    asset: &str,
    binary: &[u8],
    sums: &str,
    sig: Option<&str>,
) -> Result<(), String> {
    let Some(sig) = sig.filter(|s| !s.trim().is_empty()) else {
        return Err(format!(
            "the release is not signed (no {SIG_ASSET}) — not installing {asset}; sign it \
             (sign-releases), or wait for the author to"
        ));
    };
    verify_sums_with(pubkey, sums, sig)?;
    let actual = crate::manifest::sha256_hex(binary);
    let listed = sums.lines().any(|l| {
        let mut parts = l.split_whitespace();
        matches!((parts.next(), parts.next()),
            (Some(h), Some(f)) if h.eq_ignore_ascii_case(&actual)
                && f.trim_start_matches('*') == asset)
    });
    if !listed {
        return Err(format!(
            "CHECKSUM MISMATCH for {asset}: the signed SHA256SUMS does not list this download \
             — corrupted or tampered; not installing it"
        ));
    }
    Ok(())
}
