//! Publish-app flow (B, item 3): one app of the stack, given a hostname and
//! the container port it listens on, becomes reachable through the
//! gateway. Writes `gateway_route` (the stack's one primary route, content
//! in `traefik-routes.yml`) the first time a stack publishes anything, or
//! adds another router/service block to that same file for every publish
//! after — `extra_routes` (a file of its own under `routes/`) is kept for
//! the one case that actually needs a separate file, forced with
//! `separate_file`. A tile may be created in the same commit.
//!
//! `external`, per `GatewayRoute`'s own doc, names a backend this route
//! forwards to on purpose that is NOT one of the fleet's managed stacks
//! (Home Assistant, OPNsense, Proxmox — `core::routes::fleet_route_problems`
//! checks every other backend against the fleet). Publishing one of this
//! stack's own apps almost never needs it — the backend is this
//! container's own IP, already recognised as managed — so it is here only
//! for the rare case the port answers on an address the stack does not
//! itself own; left off, the file declares nothing external.

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use homelab_core::manifest::StackManifest;

use super::stackedit_tiles::TileFields;
use super::yamledit::Seg;

/// feat-publish-1: what the Apps tab's "Publish" dialog asks for.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishAppEdit {
    /// The app this route serves; the Traefik router and service are named
    /// after it, the same way `client::routes::service_addresses` already
    /// expects ("a stack's services are named after its apps").
    pub app: String,
    pub hostname: String,
    /// The port the app's own container answers on.
    pub port: u16,
    /// This route's backend is not one of the fleet's own stacks (see the
    /// module doc) — almost always `false` for an app of this stack.
    #[serde(default)]
    pub external: bool,
    /// Force a file of its own under `routes/` (`extra_routes`) even when
    /// the stack could add this router to its existing `traefik-routes.yml`
    /// instead. Ignored the first time a stack publishes anything, which
    /// always becomes the primary `gateway_route`.
    #[serde(default)]
    pub separate_file: bool,
    /// Create a tile for this hostname in the same commit; `None` = no
    /// tile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile: Option<TileFields>,
}

/// What the form keeps to before anything is written.
pub fn publish_problems(edit: &PublishAppEdit) -> Vec<String> {
    let mut out = Vec::new();
    if edit.app.trim().is_empty() {
        out.push("an app must be picked".to_string());
    }
    if !valid_hostname(&edit.hostname) {
        out.push(format!("{:?} is not a hostname", edit.hostname));
    }
    if edit.port == 0 {
        out.push("the port must be from 1 to 65535".to_string());
    }
    out
}

/// A DNS hostname: labels of letters, digits and `-`, never starting or
/// ending a label with `-`, at least one dot (bare `localhost` is not
/// something a router publishes).
pub fn valid_hostname(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && h.contains('.')
        && h.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// `<gateway_vmid>-app-<stack>.yml`, the only name `gateway_route.filename`
/// may have — the client independently derives and checks this same name
/// on destroy (`client::spec`'s own doc, F115).
pub fn primary_filename(gateway_vmid: u16, stack: &str) -> String {
    format!("{gateway_vmid}-app-{stack}.yml")
}

/// A stable, readable file name for an `extra_routes` entry — free-form by
/// the schema, but a name worth keeping is one that says what it is.
pub fn extra_filename(hostname: &str) -> String {
    let slug: String = hostname
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("{slug}.yml")
}

/// The router → service → loadBalancer fragment for one app, in the shape
/// `client::routes` reads: `http.routers.<service>.rule` names the host in
/// a `Host(...)` rule, `.service` points at `http.services.<service>`,
/// whose `loadBalancer.servers` carries the one backend url.
pub fn route_fragment(service: &str, hostname: &str, container_ip: &str, port: u16) -> Value {
    let mut router = Mapping::new();
    router.insert(
        Value::from("rule"),
        Value::from(format!("Host(`{hostname}`)")),
    );
    router.insert(Value::from("service"), Value::from(service));
    let mut routers = Mapping::new();
    routers.insert(Value::from(service), Value::Mapping(router));

    let mut server = Mapping::new();
    server.insert(
        Value::from("url"),
        Value::from(format!("http://{container_ip}:{port}")),
    );
    let mut lb = Mapping::new();
    lb.insert(
        Value::from("servers"),
        Value::Sequence(vec![Value::Mapping(server)]),
    );
    let mut svc = Mapping::new();
    svc.insert(Value::from("loadBalancer"), Value::Mapping(lb));
    let mut services = Mapping::new();
    services.insert(Value::from(service), Value::Mapping(svc));

    let mut http = Mapping::new();
    http.insert(Value::from("routers"), Value::Mapping(routers));
    http.insert(Value::from("services"), Value::Mapping(services));

    let mut root = Mapping::new();
    root.insert(Value::from("http"), Value::Mapping(http));
    Value::Mapping(root)
}

/// A brand-new route file for one app (the whole `traefik-routes.yml`, or
/// the whole file of an `extra_routes` entry).
pub fn new_route_file(service: &str, hostname: &str, container_ip: &str, port: u16) -> String {
    serde_yaml::to_string(&route_fragment(service, hostname, container_ip, port))
        .expect("a route fragment always serializes")
}

/// The ops that add this app's router and service to an EXISTING route
/// file's `http:` block, alongside whatever it already has (another app's
/// router keeps its own key and is untouched).
pub fn route_ops(
    service: &str,
    hostname: &str,
    container_ip: &str,
    port: u16,
) -> Vec<super::yamledit::Op> {
    let frag = route_fragment(service, hostname, container_ip, port);
    let at = |a: &str, b: &str| {
        frag.get(a)
            .and_then(|v| v.get(b))
            .and_then(|v| v.get(service))
            .cloned()
            .expect("route_fragment always has this shape")
    };
    vec![
        super::yamledit::Op::Set {
            path: vec![
                Seg::Key("http".into()),
                Seg::Key("routers".into()),
                Seg::Key(service.to_string()),
            ],
            value: at("http", "routers"),
        },
        super::yamledit::Op::Set {
            path: vec![
                Seg::Key("http".into()),
                Seg::Key("services".into()),
                Seg::Key(service.to_string()),
            ],
            value: at("http", "services"),
        },
    ]
}

/// The bare address of this stack's own container (`network.ip` is written
/// in CIDR, `10.10.10.50/24`).
pub fn container_ip(m: &StackManifest) -> &str {
    m.network.ip.split('/').next().unwrap_or(&m.network.ip)
}

fn gateway_route_value(gateway_vmid: u16, filename: &str, external: bool, backend: &str) -> Value {
    let mut g = Mapping::new();
    g.insert(Value::from("filename"), Value::from(filename));
    g.insert(Value::from("gateway_vmid"), Value::from(gateway_vmid));
    if external {
        g.insert(
            Value::from("external"),
            Value::Sequence(vec![Value::from(backend)]),
        );
    }
    Value::Mapping(g)
}

/// What this publish writes to `lxc-compose.yml`'s `gateway_route:` (the
/// stack's first publish) or `extra_routes:` (every one after, or any
/// publish the form asked to keep in a file of its own).
pub enum GatewayWrite {
    /// The stack had no `gateway_route` yet: set it, and the whole new
    /// `traefik-routes.yml`.
    Primary {
        filename: String,
        route_file: String,
    },
    /// The stack already has one: append a router+service to its existing
    /// `traefik-routes.yml`.
    Extend { ops: Vec<super::yamledit::Op> },
    /// A file of its own under `routes/`, appended to `extra_routes:`.
    Extra {
        filename: String,
        route_file: String,
        op: super::yamledit::Op,
    },
}

/// Decide which of the three this publish is, from whether the stack
/// already has a `gateway_route` and whether the form asked to keep this
/// one separate.
pub fn plan_gateway_write(
    m: &StackManifest,
    gateway_vmid: u16,
    stack: &str,
    edit: &PublishAppEdit,
    has_gateway_route: bool,
    extra_route_count: usize,
) -> GatewayWrite {
    let ip = container_ip(m);
    let backend = format!("http://{ip}:{}", edit.port);
    if !has_gateway_route {
        let filename = primary_filename(gateway_vmid, stack);
        return GatewayWrite::Primary {
            filename,
            route_file: new_route_file(&edit.app, &edit.hostname, ip, edit.port),
        };
    }
    if !edit.separate_file {
        return GatewayWrite::Extend {
            ops: route_ops(&edit.app, &edit.hostname, ip, edit.port),
        };
    }
    let filename = extra_filename(&edit.hostname);
    let value = gateway_route_value(gateway_vmid, &filename, edit.external, &backend);
    GatewayWrite::Extra {
        filename: filename.clone(),
        route_file: new_route_file(&edit.app, &edit.hostname, ip, edit.port),
        op: super::yamledit::Op::Seq {
            path: vec![Seg::Key("extra_routes".into())],
            items: (0..extra_route_count)
                .map(super::yamledit::Item::Keep)
                .chain(std::iter::once(super::yamledit::Item::New(value)))
                .collect(),
        },
    }
}

/// Whether `lxc-compose.yml`'s text already declares a `gateway_route:`,
/// and how many `extra_routes:` it already has — read directly off the
/// raw YAML, because these two keys are the stack file's own (like
/// `firewall`/`latch`) and `StackManifest` does not carry them
/// (`stackedit::parse_manifest`'s own comment: "the manifest ignores the
/// stack file's own keys").
pub fn gateway_route_state(manifest_text: &str) -> (bool, usize) {
    let Ok(doc) = serde_yaml::from_str::<Value>(manifest_text) else {
        return (false, 0);
    };
    let has_gateway_route = doc
        .get("gateway_route")
        .is_some_and(|v| !matches!(v, Value::Null));
    let extra_route_count = doc
        .get("extra_routes")
        .and_then(Value::as_sequence)
        .map_or(0, |s| s.len());
    (has_gateway_route, extra_route_count)
}

/// The primary `gateway_route:` op, when `plan_gateway_write` returned
/// `Primary`.
pub fn primary_op(
    gateway_vmid: u16,
    filename: &str,
    external: bool,
    backend: &str,
) -> super::yamledit::Op {
    super::yamledit::Op::Set {
        path: vec![Seg::Key("gateway_route".into())],
        value: gateway_route_value(gateway_vmid, filename, external, backend),
    }
}
