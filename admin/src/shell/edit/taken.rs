//! What the fleet already uses: stack names, container numbers and
//! addresses, for the new-stack wizard and the import.

use super::EditCtx;
use super::check::others;
use super::http::blocking;
use crate::core::newstack::Taken;

pub(super) async fn taken(c: &EditCtx) -> Taken {
    let mut t = Taken::default();
    if let Some(f) = c.shared.read().await.fleet.as_ref() {
        for s in &f.stacks {
            t.names.insert(s.name.clone());
            t.vmids.insert(s.vmid);
        }
    }
    let wc = c.wc.clone();
    let repo = blocking(move || others(&wc, "")).await.unwrap_or_default();
    let names = {
        let wc = c.wc.clone();
        blocking(move || wc.stack_names()).await.unwrap_or_default()
    };
    t.names.extend(names);
    for o in repo {
        t.vmids.insert(o.vmid);
        t.ips.insert(o.ip);
    }
    t.ips.extend(firewall_peers(c).await);
    t
}

/// Single addresses the firewalls name (Kenny's desktop, the router): in use
/// on the LAN even though no stack has them.
async fn firewall_peers(c: &EditCtx) -> Vec<String> {
    let wc = c.wc.clone();
    blocking(move || {
        let mut out = Vec::new();
        for n in wc.stack_names() {
            let Ok(m) = homelab_client::spec::build_manifest(&wc.repo.join("stacks").join(&n))
            else {
                continue;
            };
            for r in m.firewall.iter().flat_map(|f| f.rules.iter()) {
                for a in [&r.source, &r.dest].into_iter().flatten() {
                    if !a.contains('/') {
                        out.push(a.clone());
                    }
                }
            }
        }
        out
    })
    .await
    .unwrap_or_default()
}
