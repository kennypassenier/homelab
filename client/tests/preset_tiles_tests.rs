//! feat-tiles-2 (B): a preset's own suggested tile (`PresetMeta::tiles`),
//! applied automatically when a stack is scaffolded from it — never
//! forced: a preset that declares none scaffolds exactly as it always did.

use std::collections::BTreeMap;
use std::path::PathBuf;

use homelab_client::scaffold::{
    LoadedPreset, PresetMeta, PresetTile, StackDefaults, StackParams, scaffold_stack,
};
use homelab_core::manifest::StackManifest;

/// A fresh scratch directory, one per test (same idiom as the scaffold
/// tests in `tui_snapshot_tests.rs`): all tests in this binary share a
/// process id, so the tag must be unique per test, not the id alone.
fn scratch(tag: &str) -> PathBuf {
    let tmp =
        std::env::temp_dir().join(format!("homelab-preset-tiles-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    tmp
}

fn synth_preset(app: &str, image: &str, tiles: BTreeMap<String, PresetTile>) -> LoadedPreset {
    LoadedPreset {
        name: "synth".to_string(),
        meta: PresetMeta {
            description: "a synthetic preset for tests".to_string(),
            tiles,
            ..Default::default()
        },
        dir: None,
        apps: vec![app.to_string()],
        synth_app: Some((app.to_string(), image.to_string())),
    }
}

fn params<'a>(name: &'a str, preset: &'a LoadedPreset) -> StackParams<'a> {
    StackParams {
        name,
        vmid: 150,
        ram_mb: 512,
        cores: 1,
        disk_gb: 4,
        swap_mb: None,
        preset: Some(preset),
        no_data_paths: &[],
    }
}

#[test]
fn a_preset_with_no_tiles_scaffolds_without_a_tiles_key() {
    let tmp = scratch("notiles");
    let preset = synth_preset("app", "nginx:latest", BTreeMap::new());
    scaffold_stack(&tmp, &tmp.join("presets"), &params("plain", &preset)).unwrap();
    let text = std::fs::read_to_string(tmp.join("plain/lxc-compose.yml")).unwrap();
    assert!(!text.contains("tiles:"), "{text}");
    // And it still parses as a normal manifest.
    let m: StackManifest = serde_yaml::from_str(&text).unwrap();
    assert!(m.tiles.is_empty());
}

#[test]
fn a_presets_suggested_tile_is_written_for_its_app() {
    let tmp = scratch("suggested");
    let mut tiles = BTreeMap::new();
    tiles.insert(
        "app".to_string(),
        PresetTile {
            hostname: Some("__NAME__.kp-soft.dev".to_string()),
            name: Some("My App".to_string()),
            group: Some("Own".to_string()),
            description: Some("a test app".to_string()),
            watch_every: Some(30),
            down_after: Some(120),
        },
    );
    let preset = synth_preset("app", "nginx:latest", tiles);
    scaffold_stack(&tmp, &tmp.join("presets"), &params("tiled", &preset)).unwrap();
    let text = std::fs::read_to_string(tmp.join("tiled/lxc-compose.yml")).unwrap();
    let m: StackManifest = serde_yaml::from_str(&text).unwrap();
    let t = &m.tiles["tiled.kp-soft.dev"];
    assert_eq!(t.name, "My App");
    assert_eq!(t.group, "Own");
    assert_eq!(t.description.as_deref(), Some("a test app"));
    assert_eq!(t.watch_every, Some(30));
    assert_eq!(t.down_after, Some(120));
    assert!(t.probe.is_none());
}

#[test]
fn a_tile_with_no_hostname_template_is_not_written() {
    let tmp = scratch("nohost");
    let mut tiles = BTreeMap::new();
    tiles.insert(
        "app".to_string(),
        PresetTile {
            hostname: None,
            ..Default::default()
        },
    );
    let preset = synth_preset("app", "nginx:latest", tiles);
    scaffold_stack(&tmp, &tmp.join("presets"), &params("nohost", &preset)).unwrap();
    let text = std::fs::read_to_string(tmp.join("nohost/lxc-compose.yml")).unwrap();
    // The positive twin: this is a real, parseable manifest with no tile,
    // not an empty file that also has no `tiles:` key.
    let m: StackManifest = serde_yaml::from_str(&text).unwrap();
    assert!(m.tiles.is_empty());
    assert!(!text.contains("tiles:"), "{text}");
}

#[test]
fn defaults_fill_in_name_and_group_when_the_preset_leaves_them_out() {
    let tmp = scratch("bare");
    let mut tiles = BTreeMap::new();
    tiles.insert(
        "app".to_string(),
        PresetTile {
            hostname: Some("bare.kp-soft.dev".to_string()),
            ..Default::default()
        },
    );
    let preset = synth_preset("app", "nginx:latest", tiles);
    scaffold_stack(&tmp, &tmp.join("presets"), &params("bare", &preset)).unwrap();
    let text = std::fs::read_to_string(tmp.join("bare/lxc-compose.yml")).unwrap();
    let m: StackManifest = serde_yaml::from_str(&text).unwrap();
    let t = &m.tiles["bare.kp-soft.dev"];
    assert_eq!(t.name, "app");
    assert_eq!(t.group, "Apps");
}

#[test]
fn the_default_preset_meta_still_scaffolds_the_custom_empty_stack() {
    // PresetMeta gained a field (`tiles`); the manual `impl Default`
    // covers it, so `StackDefaults::default()` and the synthetic "custom"
    // preset (`synthetic_presets`) keep working unchanged.
    let d = StackDefaults::default();
    assert!(!d.ip_prefix.is_empty());
    let presets = homelab_client::scaffold::synthetic_presets();
    assert!(presets.iter().any(|p| p.name == "custom"));
}
