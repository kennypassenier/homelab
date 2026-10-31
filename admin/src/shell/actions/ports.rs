//! The seams the queue talks through: the live channel, the host line and
//! the Live view pause gate.

use std::sync::Arc;
use std::time::Duration;

use homelab_proto::{Command, RpcResponse, ServerMsg};
use tokio::sync::{broadcast, oneshot};

use super::BoxFut;
use crate::shell::host_link::HostClient;

/// The live channel, as the actions see it.
pub trait Publish: Send + Sync + 'static {
    fn publish(&self, event: &str, data: serde_json::Value);
}

impl Publish for chassis::shell::live::Live {
    fn publish(&self, event: &str, data: serde_json::Value) {
        let _ = chassis::shell::live::Live::publish(self, event, &data);
    }
}

/// The one line to the host, as the actions see it.
pub trait HostPort: Send + Sync + 'static {
    fn subscribe(&self) -> broadcast::Receiver<ServerMsg>;
    fn ask_traced(
        &self,
        command: Command,
        timeout: Duration,
        sent: Option<oneshot::Sender<u64>>,
    ) -> BoxFut<'_, Result<RpcResponse, String>>;
}

impl HostPort for HostClient {
    fn subscribe(&self) -> broadcast::Receiver<ServerMsg> {
        HostClient::subscribe(self)
    }
    fn ask_traced(
        &self,
        command: Command,
        timeout: Duration,
        sent: Option<oneshot::Sender<u64>>,
    ) -> BoxFut<'_, Result<RpcResponse, String>> {
        Box::pin(HostClient::ask_traced(self, command, timeout, sent))
    }
}

/// fix-172 (Kenny, 2026-10-02): what the queue asks the drive session
/// (Live view) before it runs the next queued job. A viewer's Pause used to
/// only hold a driven UI step that had not been sent yet — once the final
/// press fired a batch, the jobs it queued ran on regardless, because
/// nothing in the queue ever looked at `paused_by`. The driver (the only
/// thing that knows about Live view) registers itself here
/// ([`Actions::set_pause_gate`]), so the queue holds at the boundary
/// between two jobs the same way a driven step already held between two
/// screens — never mid-job, which stays Kenny's `stop_batch` boundary.
pub trait PauseGate: Send + Sync {
    /// Who has Live view paused right now; None to run the queue on.
    fn paused_by(&self) -> Option<String>;
    /// Notified whenever Pause, Continue or Stop changes the state above,
    /// so the queue's wait never has to poll.
    fn notified(&self) -> Arc<tokio::sync::Notify>;
}

/// fix-stale-refresh-1: told the stack of a deploy, update or restart that
/// finished well.
pub type SettledFn = Arc<dyn Fn(&str) + Send + Sync>;
