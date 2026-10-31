//! The one-line `sh -c` command the backup and restore steps run on the host.

use crate::executor::Cmd;

/// `sh -c <script>` with a timeout in seconds.
pub(super) fn sh(script: &str, timeout_s: u64) -> Cmd {
    Cmd::new("sh", &["-c", script], timeout_s)
}
