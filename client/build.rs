//! fix-149 (first-connect-pin, Kenny 2026-09-27: "Pin in de client"): the
//! fleet's certificate pin, read from the repository's `config/client.toml`
//! when the client is compiled, so a machine that has never connected does
//! not trust whatever certificate answers first. A deliberately plain line
//! scan: the file is parsed and validated properly at run time
//! (`repo_config::load`, `repo_config_tests`), and a build script with its
//! own TOML dependency would be one more thing to keep in step.

fn main() {
    let path = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../config/client.toml");
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
}
