//! T87: the glibc check on a staged binary, before it replaces a running
//! one. A probe script that reads both sides on the container, and the pure
//! verdict on what it printed.

use crate::ops::util::shq;

/// T87: what the staged binary asks of the container's C library, and what
/// the container has — read on the container, in one line, without tools.
///
/// A dynamically linked program names every glibc version it needs as a
/// plain string (`GLIBC_2.39`) in its version-needs table, so `grep -a`
/// finds the highest one without binutils; a static build carries none.
/// `getconf GNU_LIBC_VERSION` is the C library asking itself. Measured on
/// CT 109 (glibc 2.41): the static kyu binary answers `need=none`, `curl`
/// answers `need=2.34`, `bash` `need=2.38`.
///
/// Why it exists: on 2026-09-09 three kit releases built against 2.39 were
/// installed on a Debian 12 container with 2.36, and the fault surfaced at
/// the restart as a crash loop under `Restart=always` — the most expensive
/// place there is (F304). Static builds made that go away for now; this is
/// the net under the next non-static release.
pub fn glibc_probe_script(staged: &str) -> String {
    format!(
        "need=$(grep -ao 'GLIBC_2\\.[0-9]*' {bin} 2>/dev/null | sed 's/GLIBC_//' | \
         sort -t. -k2,2n -u | tail -1); \
         have=$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{{print $2}}'); \
         echo \"need=${{need:-none}} have=${{have:-unknown}}\"",
        bin = shq(staged)
    )
}

/// The verdict on that probe's one line. `Ok` carries the sentence to log;
/// `Err` carries the sentence to refuse with. Fail-closed on purpose: a
/// requirement that cannot be compared is refused, because the alternative
/// is discovering the answer as a crash loop at the first restart.
pub fn glibc_verdict(probe_output: &str) -> Result<String, String> {
    let line = probe_output.trim();
    let mut need: Option<&str> = None;
    let mut have: Option<&str> = None;
    for tok in line.split_whitespace() {
        if let Some(v) = tok.strip_prefix("need=") {
            need = Some(v);
        } else if let Some(v) = tok.strip_prefix("have=") {
            have = Some(v);
        }
    }
    let (Some(need), Some(have)) = (need, have) else {
        return Err(format!(
            "could not read what glibc the staged binary needs (probe said '{}')",
            line
        ));
    };
    if need == "none" {
        return Ok("the binary is static — it asks nothing of the container's glibc".into());
    }
    let minor = |v: &str| -> Option<u32> { v.strip_prefix("2.")?.parse().ok() };
    match (minor(need), minor(have)) {
        (Some(n), Some(h)) if n <= h => Ok(format!(
            "the binary needs glibc {} and the container has {}",
            need, have
        )),
        (Some(_), Some(_)) => Err(format!(
            "the binary needs glibc {} and the container has {} — it would install fine and \
             crash-loop at the first restart; refusing. Ship a static build, or one \
             built against glibc {} or older",
            need, have, have
        )),
        _ => Err(format!(
            "the binary needs glibc {} and the container's version could not be read ('{}') — \
             refusing rather than finding out at the first restart",
            need, have
        )),
    }
}
