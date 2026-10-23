//! pinned-digest-pruned-1: where the client finds the keep registry.

use homelab_client::pinexists::declared_keep;

#[test]
fn the_keep_registry_comes_from_the_declared_host_toml() {
    let t = "[registry_cache]\nhost = \"10.10.10.17\"\nkeep_port = 5004\n";
    assert_eq!(declared_keep(t), Some(("10.10.10.17".to_string(), 5004)));
    assert_eq!(
        declared_keep("[registry_cache]\nhost = \"10.10.10.17\"\n"),
        None
    );
    assert_eq!(declared_keep("nonsense ["), None);
}
