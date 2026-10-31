//! feat-secrets-2: a new secret value waits here, staged by the Secrets
//! page, until the `change-secret` job it is for takes it exactly once.

use super::{Actions, SECRET_STAGE_TTL};

/// feat-secrets-2: an opaque, single-use token for one staged value — 256
/// bits from the kernel's own CSPRNG (`/dev/urandom` via `getrandom(2)`,
/// the same source TLS key material on this host already comes from), so
/// it cannot be guessed or replayed from one request to the next the way a
/// counter or a timestamp could.
pub(super) fn secret_stage_token() -> String {
    let mut buf = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut buf))
        .unwrap_or_else(|_| {
            // No /dev/urandom (a non-Linux test run): fall back to a
            // process-unique counter mixed with the clock. Not a security
            // property, only a test-time escape hatch — production is
            // Linux-on-Proxmox, which always has it.
            static CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let c = CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            for (i, b) in buf.iter_mut().enumerate() {
                *b = ((c.wrapping_add(t)) >> ((i % 8) * 8)) as u8;
            }
        });
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

impl Actions {
    /// feat-secrets-2: stage a new secret value for one `change-secret`
    /// job, opaque token back. Never logged by the caller (`shell::secrets`)
    /// — the body it came from is not even parsed into anything that could
    /// be. Also sweeps expired entries, so a never-pressed stage does not
    /// sit in memory forever.
    pub fn stage_secret(&self, content: String) -> String {
        let token = secret_stage_token();
        let mut g = self
            .inner
            .secret_stage
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        g.retain(|_, (at, _)| at.elapsed() < SECRET_STAGE_TTL);
        g.insert(token.clone(), (std::time::Instant::now(), content));
        token
    }

    /// feat-secrets-2: the staged value, taken exactly once — a second call
    /// with the same token (a retried press, a replayed drive step) finds
    /// nothing, the same way a used ticket does not work twice.
    pub(super) fn take_staged_secret(&self, token: &str) -> Option<String> {
        let mut g = self
            .inner
            .secret_stage
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match g.remove(token) {
            Some((at, content)) if at.elapsed() < SECRET_STAGE_TTL => Some(content),
            _ => None,
        }
    }
}
