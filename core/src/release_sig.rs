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
