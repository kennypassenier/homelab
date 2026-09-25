//! Print the Alloy configuration the deploy would render for one stack.
//!
//! `cargo run -q -p homelab-core --example render_alloy -- stacks/gateway/lxc-compose.yml http://10.10.10.4:3100`
//!
//! Exists for gap-11: the only way to see what a deploy will write to
//! `/etc/alloy/config.alloy` was to deploy, and the gateway is the one stack
//! where a deploy is not free (gap-12). Calls the same `config()` the deploy
//! calls, so what it prints is what would land — diff it against the file
//! in the container.
fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(path), Some(loki)) = (args.next(), args.next()) else {
        eprintln!("usage: render_alloy <stacks/<name>/lxc-compose.yml> <loki base url>");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {} — pass the path to a stack's lxc-compose.yml",
            path, e
        )
    });
    let m: homelab_core::manifest::StackManifest = serde_yaml::from_str(&text)
        .unwrap_or_else(|e| panic!("{} does not parse as a stack manifest: {}", path, e));
    homelab_core::manifest::validate_manifest(&m)
        .unwrap_or_else(|e| panic!("{} would be refused by the deploy: {}", path, e));
    print!(
        "{}",
        homelab_core::ops::logshipper::config(
            &m.stack_name,
            &m.hostname,
            &loki,
            &m.syslog_receivers
        )
    );
}
