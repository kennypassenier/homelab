//! fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): compile
//! `git describe --dirty` into the daemon as HOMELAB_BUILD, so a hand-built
//! binary (`make host-binary`, `homelab self-update`) no longer passes for
//! the release of the same version.

include!("../build-support/git_describe.rs");

fn main() {
    let dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
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
