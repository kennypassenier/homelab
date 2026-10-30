//! Tracing set-up: the journal subscriber, the compact log ring file that feeds the dashboard's log view, and the spans that tag a line with its request and stack.

use crate::*;

/// The daemon's log subscriber, writing to `writer` (stderr, which systemd
/// puts in the journal).
///
/// fix-122 (expert panel, journal-lines-lack-op-context, 2026-09-27): no
/// ANSI colour. tracing-subscriber colours by default, so a journal line
/// read `\x1b[33m WARN\x1b[0m` and `grep 'WARN scheduler'` found nothing.
///
/// `main` builds its own subscriber (`init_production_logging`, below) —
/// journald plus the JSONL ring, one reloadable filter for both. This one
/// stays test-only: it pins the ANSI-free, span-carrying shape of a single
/// journald line without the ring's file I/O getting in the way.
#[cfg(test)]
pub(crate) fn journal_subscriber<W>(
    writer: W,
    default_filter: &str,
) -> impl tracing::Subscriber + Send + Sync + use<W>
where
    W: for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .with_ansi(false)
        .with_writer(writer)
        .finish()
}

/// fix-122 (AR15's JSONL ring + runtime debug toggle, Kenny's go
/// 2026-10-01, `main`'s own subscriber — `journal_subscriber` above stays
/// as it was, for the test that pins its ANSI-free, span-carrying shape):
/// journald (no ANSI, same as `journal_subscriber`) plus a size-capped
/// JSONL ring under `<state_dir>/logs/host.jsonl` (`RingWriter`), behind
/// one `EnvFilter` both sinks share — reloadable at runtime
/// (`Rpc::SetHostConfig`/`Rpc::ApplyHostConfig`'s `log_level`) through the
/// `LogFilterHandle` this returns, with no restart.
///
/// `RUST_LOG` wins when set, exactly as `journal_subscriber` already did;
/// `initial_level` (`config.log_level`, host.toml's own) is the fallback.
pub(crate) fn init_production_logging(
    state_dir: &str,
    initial_level: &str,
    ring_max_bytes: u64,
) -> LogFilterHandle {
    use tracing_subscriber::prelude::*;

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| initial_level.into());
    let (filter, handle) = tracing_subscriber::reload::Layer::new(filter);

    let journal_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(std::io::stderr);

    let ring_path = std::path::Path::new(state_dir)
        .join("logs")
        .join("host.jsonl");
    let ring_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .json()
        .with_writer(RingWriter::new(ring_path, ring_max_bytes));

    tracing_subscriber::registry()
        .with(filter)
        .with(journal_layer)
        .with(ring_layer)
        .init();
    handle
}

/// fix-122: writes to `logs/host.jsonl`, cutting the file back to half its
/// cap — the same shape `compact_journal_file` cuts `journal.jsonl` in —
/// once a write leaves it over `max_bytes`. Reopened on every write rather
/// than held open: the daemon's own log volume is small (a home fleet, not
/// a datacentre), and this way a rotated or removed file is simply
/// recreated on the next line instead of silently writing nowhere.
#[derive(Clone)]
pub(crate) struct RingWriter {
    pub(crate) path: std::path::PathBuf,
    pub(crate) max_bytes: u64,
}

impl RingWriter {
    pub(crate) fn new(path: std::path::PathBuf, max_bytes: u64) -> Self {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        RingWriter { path, max_bytes }
    }
}

impl std::io::Write for RingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(buf)?;
        let over = f
            .metadata()
            .map(|m| m.len() > self.max_bytes)
            .unwrap_or(false);
        drop(f);
        if over {
            compact_log_ring_file(&self.path, self.max_bytes);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// fix-122: `compact_journal_file`'s shape, for `logs/host.jsonl` instead
/// of `journal.jsonl` — same atomic write-then-rename, same 0600, no
/// "interrupted operation" to pin (`logring::compact_ring` keeps only the
/// newest lines, nothing more).
pub(crate) fn compact_log_ring_file(path: &std::path::Path, max_bytes: u64) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Some(cut) = homelab_core::logring::compact_ring(&content, max_bytes as usize) else {
        return;
    };
    let tmp = path.with_extension("jsonl.compact.tmp");
    let written = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(cut.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        tracing::warn!("{}: could not compact :: {}", path.display(), e);
    }
}

/// fix-122: the span that names the stack an operation works on. Every
/// journal line inside it carries `stack=<name>`, which is what tells three
/// concurrent nightly backups apart.
pub(crate) fn stack_span(stack: &str) -> tracing::Span {
    tracing::info_span!("stack", stack = %stack)
}

/// fix-122: the span a request runs in: its id, and its stack when it has
/// one (a request about the host or the fleet has none, and says nothing).
pub(crate) fn rpc_span(req: &RpcRequest) -> tracing::Span {
    let span = tracing::info_span!("rpc", id = req.id, stack = tracing::field::Empty);
    if let Some(stack) = rpc_stack(&req.command) {
        span.record("stack", stack.as_str());
    }
    span
}

/// fix-122: the stack a request is about, for its span. `None` for requests
/// about the host or the whole fleet.
pub(crate) fn rpc_stack(command: &Rpc) -> Option<String> {
    match command {
        Rpc::DeployStack(spec) => Some(spec.manifest.stack_name.clone()),
        Rpc::DestroyStack { manifest, .. }
        | Rpc::RestoreStack { manifest, .. }
        | Rpc::UpdateStack { manifest, .. }
        | Rpc::PruneOrphans { manifest, .. } => Some(manifest.stack_name.clone()),
        Rpc::BackupStack(m) | Rpc::ApplyResources(m) => Some(m.stack_name.clone()),
        Rpc::StageNativeBinary { stack, .. }
        | Rpc::BackupNative { stack }
        | Rpc::UpdateNative { stack }
        | Rpc::ReleaseUpdateNative { stack }
        | Rpc::ForgetStack { stack }
        | Rpc::DestroyRecorded { stack, .. }
        | Rpc::SetStackEnabled { stack, .. }
        | Rpc::SealEnv { stack }
        | Rpc::GetApplied { stack }
        | Rpc::GetBackups { stack, .. }
        | Rpc::RestoreNative { stack, .. }
        | Rpc::VerifyRestore { stack, .. }
        | Rpc::RevealSecret { stack, .. }
        | Rpc::SetSecret { stack, .. } => Some(stack.clone()),
        Rpc::InstallNative { manifest, .. }
        | Rpc::InstallNativeRelease { manifest, .. }
        | Rpc::AdoptService(manifest) => Some(manifest.stack_name.clone()),
        _ => None,
    }
}
