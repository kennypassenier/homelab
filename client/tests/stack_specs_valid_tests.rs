//! keep-storage-name-1: the 4.1.0 rollout's registry deploy was refused by
//! the deploy-time validator (`manifest::validate` over the whole spec),
//! which no test ran over the real stack files: the manifest-only check in
//! core/tests/stack_files_tests.rs passed. This runs the spec-level check a
//! deploy runs, over every compose stack in the repository, offline.

use std::path::PathBuf;

fn stacks_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("stacks")
}

#[test]
fn keep_storage_name_1_every_compose_stack_passes_the_deploy_validator() {
    let mut seen = 0;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(stacks_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("lxc-compose.yml").is_file())
        .collect();
    dirs.sort();
    let mut bad = Vec::new();
    for dir in dirs {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        match homelab_client::spec::build_spec_files_only(&dir) {
            Ok(spec) => {
                seen += 1;
                if let Err(e) = homelab_core::manifest::validate(&spec) {
                    bad.push(format!("stacks/{name}: {e}"));
                }
            }
            Err(e) => bad.push(format!("stacks/{name}: the spec does not build: {e}")),
        }
    }
    assert!(seen >= 10, "found only {seen} compose stacks");
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
