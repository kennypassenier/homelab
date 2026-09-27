//! feat-client-1 · the host address is a fact about the fleet, not about
//! the machine typing, so it lives in the repository.
//!
//! Kenny, 2026-09-19 (gap-13 deep dive): "ik wil niet dat het afhankelijk
//! is van de machine" — a second desktop, or a Windows box later, must find
//! the daemon without a per-machine env file. `config/client.toml` carries
//! the address and the daemon's certificate fingerprint; the machine keeps
//! only its token.

use homelab_client::repo_config::{
    find_repo_file, load, reconcile_pin, resolve_host, ClientConfig, HostSource, DEFAULT_HOST,
    REPO_FILE,
};
use std::path::{Path, PathBuf};

/// A throwaway directory under the system temp dir, unique per test and per
/// process, without pulling in a crate for it (CI builds `--locked`).
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "homelab-repo-config-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn repo(host: &str) -> ClientConfig {
    ClientConfig {
        host: Some(host.into()),
        pin: None,
    }
}

/// The whole point: the repository's word beats what this machine's env
/// file says, and the compiled-in default is the last resort.
#[test]
fn the_repo_file_beats_the_machine_file_and_the_default() {
    let cfg = repo("10.10.10.250:8443");
    let (host, src) = resolve_host(
        None,
        Some((Path::new("/r/config/client.toml"), &cfg)),
        Some("10.10.5.250:8443".into()),
    );
    assert_eq!(host, "10.10.10.250:8443");
    assert!(matches!(src, HostSource::RepoConfig(_)), "{:?}", src);
}

/// A one-off override typed before the command must keep working — that is
/// how the 3.52.0 rollout got past the stalled path on 2026-09-19.
#[test]
fn an_explicit_environment_variable_still_wins() {
    let cfg = repo("10.10.10.250:8443");
    let (host, src) = resolve_host(
        Some("10.10.5.250:8443".into()),
        Some((Path::new("/r/config/client.toml"), &cfg)),
        None,
    );
    assert_eq!(host, "10.10.5.250:8443");
    assert_eq!(src, HostSource::Environment);
}

#[test]
fn without_a_repo_file_the_machine_file_speaks_and_then_the_default() {
    let (host, src) = resolve_host(None, None, Some("10.10.5.250:8443".into()));
    assert_eq!(host, "10.10.5.250:8443");
    assert_eq!(src, HostSource::MachineConfig);
    let (host, src) = resolve_host(None, None, None);
    assert_eq!(host, DEFAULT_HOST);
    assert_eq!(src, HostSource::Default);
}

/// A repo file that names no host is not an override of anything.
#[test]
fn a_repo_file_without_a_host_line_changes_nothing() {
    let cfg = ClientConfig {
        host: None,
        pin: Some("AA:BB".into()),
    };
    let (host, src) = resolve_host(None, Some((Path::new("/r/x"), &cfg)), None);
    assert_eq!(host, DEFAULT_HOST);
    assert_eq!(src, HostSource::Default);
}

/// `homelab deploy stacks/gateway` is typed from the repo root, but `homelab
/// check` is typed from anywhere — including a subdirectory of the repo.
#[test]
fn the_file_is_found_from_a_subdirectory_of_the_repo() {
    let dir = scratch("subdir");
    let cfg_path = dir.join(REPO_FILE);
    std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
    std::fs::write(&cfg_path, "host = \"10.10.10.250:8443\"\n").unwrap();
    let deep = dir.join("stacks/gateway");
    std::fs::create_dir_all(&deep).unwrap();
    assert_eq!(find_repo_file(&deep).as_deref(), Some(cfg_path.as_path()));
    let (found, cfg) = load(&deep).unwrap().expect("found");
    assert_eq!(found, cfg_path);
    assert_eq!(cfg.host.as_deref(), Some("10.10.10.250:8443"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn outside_any_repo_there_is_simply_no_file() {
    let dir = scratch("outside");
    assert!(find_repo_file(&dir).is_none());
    assert!(load(&dir).unwrap().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Standing rule 45: an input the program could not parse is refused, never
/// quietly replaced by a default — a typo in the address file would
/// otherwise send every command to the wrong door without a word.
#[test]
fn a_file_that_does_not_parse_is_refused_not_defaulted() {
    let dir = scratch("malformed");
    let cfg_path = dir.join(REPO_FILE);
    std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
    std::fs::write(&cfg_path, "host = 10.10.10.250:8443\n").unwrap();
    let err = load(&dir).expect_err("unparseable");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(err.contains("client.toml"), "{}", err);
    assert!(
        err.contains("host = \"") || err.contains("quote"),
        "the message carries its remedy: {}",
        err
    );
}

/// The daemon's fingerprint in the repo means a fresh machine does not
/// trust on first use — but it may never overrule a machine that already
/// pinned something else, because that is exactly what a changed
/// certificate looks like.
#[test]
fn a_repo_pin_fills_an_empty_machine_and_never_overrides_a_different_one() {
    let a = "85:00:F8:84:44:87:7E:85:FA:2E:29:97:15:16:74:15:43:FE:0E:70:5A:AE:F8:1C:EE:ED:F8:25:42:DC:7F:2A";
    let b = "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00";
    let d = reconcile_pin(None, Some(&format!("SHA256:{}", a))).unwrap();
    assert_eq!(d.pin.as_deref(), Some(a), "the SHA256: prefix is cosmetic");
    assert!(d.adopted_from_repo);
    let d = reconcile_pin(Some(a.into()), Some(a)).unwrap();
    assert_eq!(d.pin.as_deref(), Some(a));
    assert!(!d.adopted_from_repo);
    let d = reconcile_pin(Some(a.into()), None).unwrap();
    assert_eq!(d.pin.as_deref(), Some(a));
    let d = reconcile_pin(None, None).unwrap();
    assert!(d.pin.is_none(), "nothing pinned anywhere: TOFU as before");
    let err = reconcile_pin(Some(a.into()), Some(b)).expect_err("two different pins");
    assert!(
        err.contains("config/client.toml") && err.contains(".config/homelab/pin"),
        "{}",
        err
    );
}

/// The committed file itself, as the deploy would read it: the in-VLAN
/// address chosen on 2026-09-19 and the daemon's real fingerprint.
#[test]
fn the_committed_client_file_names_the_in_vlan_door_and_a_real_fingerprint() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let (_, cfg) = load(root)
        .unwrap()
        .expect("config/client.toml is committed");
    assert_eq!(cfg.host.as_deref(), Some("10.10.10.250:8443"));
    let pin = cfg.pin.expect("the pin is committed");
    let hex: Vec<&str> = pin.trim_start_matches("SHA256:").split(':').collect();
    assert_eq!(hex.len(), 32, "a SHA-256 fingerprint is 32 bytes: {}", pin);
    assert!(
        hex.iter()
            .all(|h| h.len() == 2 && u8::from_str_radix(h, 16).is_ok()),
        "{}",
        pin
    );
}

/// first-connect-pin (Kenny, 2026-09-27: "Pin in de client"): a machine
/// with no pin of its own trusted the first certificate it saw and sent the
/// bearer token to it. The fleet's pin is compiled into the client, and it
/// is the only certificate trusted, first connection included.
/// covers: fix-149
#[test]
fn fix_149_the_built_in_pin_is_the_only_certificate_trusted() {
    use homelab_client::repo_config::reconcile_pin_built_in;
    let a = "AA:AA";
    let b = "BB:BB";
    // First connection on a fresh machine: no trust on first use.
    let d = reconcile_pin_built_in(Some(a), None, None).unwrap();
    assert_eq!(d.pin.as_deref(), Some(a));
    assert!(d.adopted_from_repo, "the machine saves it");
    // The same pin everywhere: nothing to say.
    let d = reconcile_pin_built_in(Some(a), Some(a.into()), Some(a)).unwrap();
    assert_eq!(d.pin.as_deref(), Some(a));
    assert!(!d.adopted_from_repo);
    // A machine pin or a repository pin that disagrees is refused, with the
    // remedy named.
    let e = reconcile_pin_built_in(Some(a), Some(b.into()), None).unwrap_err();
    assert!(e.contains(".config/homelab/pin"), "{}", e);
    let e = reconcile_pin_built_in(Some(a), None, Some(b)).unwrap_err();
    assert!(
        e.contains("config/client.toml") && e.contains("self-install"),
        "{}",
        e
    );
    // A client built without a pin behaves as before.
    assert_eq!(
        reconcile_pin_built_in(None, None, Some(b)).unwrap(),
        reconcile_pin(None, Some(b)).unwrap()
    );
}

/// The released client carries the committed pin, and the DR runbook says
/// what a regenerated certificate then asks for.
/// covers: fix-149
#[test]
fn fix_149_the_client_is_built_with_the_committed_pin() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let (_, cfg) = load(root)
        .unwrap()
        .expect("config/client.toml is committed");
    let committed = cfg.pin.expect("the pin is committed");
    assert_eq!(
        homelab_client::repo_config::built_in_pin().map(str::to_string),
        Some(committed.trim_start_matches("SHA256:").to_string())
    );
    let out = std::env::temp_dir().join(format!("homelab-dr-f149-{}.md", std::process::id()));
    homelab_client::spec::generate_runbook(&root.join("stacks"), out.to_str().unwrap()).unwrap();
    let doc = std::fs::read_to_string(&out).unwrap();
    let _ = std::fs::remove_file(&out);
    assert!(
        doc.contains("built into the client"),
        "the runbook predates fix-149"
    );
    assert!(
        doc.contains("homelab self-install"),
        "the runbook names no way to rebuild"
    );
}

/// cli-path-vs-name-and-cwd (expert panel, 2026-09-27): half the verbs took
/// `stacks/<name>`, the other half `<name>`, and the wrong form answered
/// "cannot read almanac/lxc-compose.yml" with no hint. Every spelling now
/// names the same stack.
/// covers: fix-101
#[test]
fn fix_101_every_spelling_of_a_stack_names_the_same_stack() {
    use homelab_client::repo_config::stack_name;
    for arg in [
        "almanac",
        "almanac/",
        "stacks/almanac",
        "stacks/almanac/",
        "./stacks/almanac",
        "/home/k/Projects/homelab/stacks/almanac",
    ] {
        assert_eq!(stack_name(arg), "almanac", "{}", arg);
    }
    // A unit inside a stack, and an app of a retired stack, keep their part.
    assert_eq!(stack_name("stacks/kyu/kyu-runner"), "kyu/kyu-runner");
    assert_eq!(stack_name("media/radarr"), "media/radarr");
}

/// The verbs read `./stacks`, so outside the repository `check` checked half
/// the fleet and `deploy` found nothing. The repository is found once: up
/// from the working directory, or where `HOMELAB_REPO` points.
/// covers: fix-101
#[test]
fn fix_101_a_stack_resolves_from_its_name_from_any_directory() {
    use homelab_client::repo_config::{repo_root, stack_dir, stacks_dir};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let elsewhere = scratch("elsewhere");

    assert_eq!(
        repo_root(&root.join("stacks/almanac"), None).as_deref(),
        Some(root)
    );
    assert_eq!(
        repo_root(&elsewhere, Some(root.to_str().unwrap())).as_deref(),
        Some(root)
    );
    assert_eq!(repo_root(&elsewhere, None), None);
    assert_eq!(
        repo_root(&elsewhere, Some(elsewhere.to_str().unwrap())),
        None
    );

    let almanac = root.join("stacks/almanac");
    for arg in ["almanac", "stacks/almanac/", "almanac/"] {
        assert_eq!(stack_dir(arg, &elsewhere, Some(root)), almanac, "{}", arg);
    }
    // Standing in the repository, the path as typed still works.
    assert_eq!(stack_dir("stacks/almanac", root, Some(root)), almanac);
    assert_eq!(stacks_dir(Some(root)), root.join("stacks"));
    assert_eq!(stacks_dir(None), PathBuf::from("stacks"));
}
