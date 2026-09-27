//! fix-149 (first-connect-pin, Kenny 2026-09-27: "Pin in de client"): the
//! fleet's certificate pin, read from the repository's `config/client.toml`
//! when the client is compiled, so a machine that has never connected does
//! not trust whatever certificate answers first. A deliberately plain line
//! scan: the file is parsed and validated properly at run time
//! (`repo_config::load`, `repo_config_tests`), and a build script with its
//! own TOML dependency would be one more thing to keep in step.
//!
//! fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): compile
//! `git describe --dirty` into the client as HOMELAB_BUILD, so `homelab ping`
//! says which tree it was built from and a deploy can record it.

include!("../build-support/git_describe.rs");

fn main() {
    let dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    let path = dir.join("../config/client.toml");
    println!("cargo:rerun-if-changed={}", path.display());
    let pin = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let rest = line.trim().strip_prefix("pin")?.trim_start();
                let value = rest.strip_prefix('=')?.trim().trim_matches('"').trim();
                (!value.is_empty()).then(|| value.to_string())
            })
        })
        .unwrap_or_default();
    println!("cargo:rustc-env=HOMELAB_BUILT_IN_PIN={}", pin);

    for p in rerun_paths(&dir) {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    // The sources this binary is made of: an edit there moves `-dirty`.
    for rel in [
        "src",
        "build.rs",
        "../core/src",
        "../proto/src",
        "../build-support",
        "../Cargo.lock",
    ] {
        println!("cargo:rerun-if-changed={}", dir.join(rel).display());
    }
    println!("cargo:rustc-env=HOMELAB_BUILD={}", git_describe(&dir));
}
