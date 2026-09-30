//! feat-tiles-1: the tiles form — create/edit/rename/delete a stack's
//! `tiles:` map, and the validation `stackedit::changes` applies before any
//! text is touched. The Settings tab's own watch-only edit
//! (`settings_edit_writes_a_tiles_watch_override` in edit_core_tests.rs)
//! keeps working exactly as it did; these tests are the fuller form beside
//! it, writing the same `tiles.<key>.*` paths.

use homelab_admin::core::stackedit::{changes, StackEdit, StackTexts};
use homelab_admin::core::stackedit_tiles::{tiles_problems, TileFields, TileItemEdit, TilesEdit};

const BASE: &str = "stack_name: x\nvmid: 150\nhostname: 150-app-x\nnetwork:\n  ip: 10.10.10.50/24\n  gateway: 10.10.10.1\n  bridge: vmbr0\n  vlan: 10\nresources:\n  cores: 1\n  memory_mb: 512\n  swap_mb: 0\n  disk_gb: 8\n  storage: local-lvm\nlxc:\n  template: clone:996\n  unprivileged: true\n  features: nesting=1\n  protection: true\nboot:\n  onboot: true\napps: []\ntiles:\n  old.kp-soft.dev:\n    name: Old\n    group: Apps\n    url: http://10.10.10.50:8080/\n";

fn texts() -> StackTexts {
    let mut t = StackTexts::new();
    t.insert("lxc-compose.yml".to_string(), BASE.to_string());
    t
}

fn fields(name: &str, group: &str) -> TileFields {
    TileFields {
        name: name.to_string(),
        group: group.to_string(),
        order: None,
        description: None,
        url: Some("http://10.10.10.50:9000/".to_string()),
        reading: None,
        watch_every: None,
        down_after: None,
    }
}

#[test]
fn a_new_tile_is_created() {
    let edit = TilesEdit {
        tiles: vec![TileItemEdit {
            origin: None,
            key: "new.kp-soft.dev".to_string(),
            delete: false,
            tile: fields("New", "Own"),
        }],
    };
    let out = changes("x", &texts(), &StackEdit::Tiles(edit), None).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
    let new = out[0].new.as_ref().unwrap();
    assert!(new.contains("new.kp-soft.dev"), "{new}");
    // The existing tile's own block is untouched.
    assert!(new.contains("old.kp-soft.dev:\n    name: Old"), "{new}");
    let m = homelab_admin::core::stackedit::parse_manifest(new).unwrap();
    assert_eq!(m.tiles.len(), 2);
    assert_eq!(m.tiles["new.kp-soft.dev"].name, "New");
    assert!(m.tiles["new.kp-soft.dev"].probe.is_none());
}

#[test]
fn a_tile_is_renamed_keeping_its_fields() {
    let edit = TilesEdit {
        tiles: vec![TileItemEdit {
            origin: Some("old.kp-soft.dev".to_string()),
            key: "renamed.kp-soft.dev".to_string(),
            delete: false,
            tile: fields("Old", "Apps"),
        }],
    };
    let out = changes("x", &texts(), &StackEdit::Tiles(edit), None).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
    let m = homelab_admin::core::stackedit::parse_manifest(out[0].new.as_ref().unwrap()).unwrap();
    assert!(!m.tiles.contains_key("old.kp-soft.dev"));
    assert_eq!(m.tiles["renamed.kp-soft.dev"].name, "Old");
}

#[test]
fn a_tile_is_deleted() {
    let edit = TilesEdit {
        tiles: vec![TileItemEdit {
            origin: Some("old.kp-soft.dev".to_string()),
            key: "old.kp-soft.dev".to_string(),
            delete: true,
            tile: TileFields::default(),
        }],
    };
    let out = changes("x", &texts(), &StackEdit::Tiles(edit), None).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
    let m = homelab_admin::core::stackedit::parse_manifest(out[0].new.as_ref().unwrap()).unwrap();
    assert!(m.tiles.is_empty());
}

#[test]
fn editing_a_field_of_an_existing_tile_leaves_the_rest_as_is() {
    let edit = TilesEdit {
        tiles: vec![TileItemEdit {
            origin: Some("old.kp-soft.dev".to_string()),
            key: "old.kp-soft.dev".to_string(),
            delete: false,
            tile: TileFields {
                name: "Old".to_string(),
                group: "Apps".to_string(),
                order: None,
                description: None,
                url: Some("http://10.10.10.50:8080/".to_string()),
                reading: None,
                watch_every: Some(30),
                down_after: None,
            },
        }],
    };
    let out = changes("x", &texts(), &StackEdit::Tiles(edit), None).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
    let m = homelab_admin::core::stackedit::parse_manifest(out[0].new.as_ref().unwrap()).unwrap();
    let t = &m.tiles["old.kp-soft.dev"];
    assert_eq!(t.watch_every, Some(30));
    assert_eq!(t.name, "Old");
    assert_eq!(t.url.as_deref(), Some("http://10.10.10.50:8080/"));
}

#[test]
fn an_unchanged_tile_writes_no_file() {
    let edit = TilesEdit {
        tiles: vec![TileItemEdit {
            origin: Some("old.kp-soft.dev".to_string()),
            key: "old.kp-soft.dev".to_string(),
            delete: false,
            tile: TileFields {
                name: "Old".to_string(),
                group: "Apps".to_string(),
                order: None,
                description: None,
                url: Some("http://10.10.10.50:8080/".to_string()),
                reading: None,
                watch_every: None,
                down_after: None,
            },
        }],
    };
    let out = changes("x", &texts(), &StackEdit::Tiles(edit), None).unwrap();
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn a_tile_that_does_not_exist_is_refused() {
    let edit = TilesEdit {
        tiles: vec![TileItemEdit {
            origin: Some("missing.kp-soft.dev".to_string()),
            key: "missing.kp-soft.dev".to_string(),
            delete: false,
            tile: fields("Missing", "Own"),
        }],
    };
    let err = changes("x", &texts(), &StackEdit::Tiles(edit), None).unwrap_err();
    assert!(err.why.contains("is not a tile of this stack"), "{err:?}");
}

#[test]
fn a_new_key_that_collides_with_an_existing_tile_is_refused() {
    let edit = TilesEdit {
        tiles: vec![TileItemEdit {
            origin: None,
            key: "old.kp-soft.dev".to_string(),
            delete: false,
            tile: fields("Dup", "Own"),
        }],
    };
    let err = changes("x", &texts(), &StackEdit::Tiles(edit), None).unwrap_err();
    assert!(err.why.contains("already a tile"), "{err:?}");
}

#[test]
fn watch_validation_mirrors_the_settings_tab() {
    let too_low = TilesEdit {
        tiles: vec![TileItemEdit {
            origin: Some("old.kp-soft.dev".to_string()),
            key: "old.kp-soft.dev".to_string(),
            delete: false,
            tile: TileFields {
                watch_every: Some(5),
                ..fields("Old", "Apps")
            },
        }],
    };
    let problems = tiles_problems(&too_low);
    assert!(
        problems.iter().any(|p| p.contains("at least 10 s")),
        "{problems:?}"
    );

    let inverted = TilesEdit {
        tiles: vec![TileItemEdit {
            origin: Some("old.kp-soft.dev".to_string()),
            key: "old.kp-soft.dev".to_string(),
            delete: false,
            tile: TileFields {
                watch_every: Some(120),
                down_after: Some(60),
                ..fields("Old", "Apps")
            },
        }],
    };
    let problems = tiles_problems(&inverted);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("must be at least check every")),
        "{problems:?}"
    );
}

#[test]
fn a_tile_with_no_name_or_group_is_refused() {
    let edit = TilesEdit {
        tiles: vec![TileItemEdit {
            origin: None,
            key: "bare.kp-soft.dev".to_string(),
            delete: false,
            tile: TileFields::default(),
        }],
    };
    let problems = tiles_problems(&edit);
    assert!(
        problems.iter().any(|p| p.contains("needs a name")),
        "{problems:?}"
    );
    assert!(
        problems.iter().any(|p| p.contains("needs a group")),
        "{problems:?}"
    );
}

#[test]
fn deleting_a_new_tile_is_refused() {
    let edit = TilesEdit {
        tiles: vec![TileItemEdit {
            origin: None,
            key: "x.kp-soft.dev".to_string(),
            delete: true,
            tile: TileFields::default(),
        }],
    };
    let problems = tiles_problems(&edit);
    assert!(
        problems.iter().any(|p| p.contains("cannot be deleted")),
        "{problems:?}"
    );
}
