//! Tiles form (dashboard gap: only `watch_every`/`down_after` of a tile
//! were editable, on the Settings tab, and a tile could not be created,
//! renamed or removed at all). Full create/edit/rename/delete of a stack's
//! `tiles:` map (`homelab_core::manifest::Tile`), keyed by the hostname the
//! tile opens.
//!
//! The Settings tab's two watch fields (`stackedit::SettingsEdit::tiles`,
//! `TileEdit`) are left as they are — a quick edit of an existing tile's
//! watch seconds without opening the full form — and write into the same
//! `tiles.<key>.{watch_every,down_after}` path this module does, field by
//! field, so the two stay consistent with each other.
//!
//! A tile's `probe` field is never written here: it is computed by the
//! client at deploy time (`manifest::Tile::probe`'s own doc — "Never edited
//! in the stack file").

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use homelab_core::manifest::{StackManifest, Tile};

use super::yamledit::{Op, Seg};

/// feat-tiles-1: the whole `tiles:` map as the editor holds it, one entry
/// per row of the form.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TilesEdit {
    pub tiles: Vec<TileItemEdit>,
}

/// One tile, and the hostname it had before (`origin`, `None`: new). `key`
/// is the hostname it should have afterward; `delete` removes it (then
/// `tile` is ignored).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TileItemEdit {
    #[serde(default)]
    pub origin: Option<String>,
    pub key: String,
    #[serde(default)]
    pub delete: bool,
    #[serde(default)]
    pub tile: TileFields,
}

/// A tile's own fields, everything but the client-computed `probe`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TileFields {
    // A `delete: true` row's own `tile` is ignored (`TileItemEdit`'s doc),
    // but still has to deserialize — the browser and the driving surface
    // both send `"tile":{}` for a tombstone rather than omitting the key,
    // so these two can't be required the way a real tile's are (checked
    // instead by `tile_problems`, before a non-delete row is ever sent).
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub order: Option<u32>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub watch_url: Option<String>,
    #[serde(default)]
    pub reading: Option<String>,
    #[serde(default)]
    pub watch_every: Option<u64>,
    #[serde(default)]
    pub down_after: Option<u64>,
}

fn tile_of(f: &TileFields) -> Tile {
    Tile {
        name: f.name.clone(),
        group: f.group.clone(),
        order: f.order.unwrap_or(100),
        description: f.description.clone(),
        url: f.url.clone(),
        watch_url: f.watch_url.clone(),
        reading: f.reading.clone(),
        probe: None,
        watch_every: f.watch_every,
        down_after: f.down_after,
    }
}

/// feat-tiles-1: the ranges and picks the tiles form keeps to, checked
/// before any text is touched. `groups` is the list of groups already used
/// elsewhere in the fleet (the form offers them plus free text), only
/// advisory — a new group is not an error.
pub fn tiles_problems(edit: &TilesEdit) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen_keys: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut seen_origins: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for (n, item) in edit.tiles.iter().enumerate() {
        let who = |k: &str| {
            if k.trim().is_empty() {
                format!("tile {}", n + 1)
            } else {
                k.to_string()
            }
        };
        if let Some(o) = &item.origin {
            if !seen_origins.insert(o.as_str()) {
                out.push(format!("{}: listed twice", who(o)));
            }
        }
        if item.delete {
            if item.origin.is_none() {
                out.push(format!("{}: a new tile cannot be deleted", who(&item.key)));
            }
            continue;
        }
        if item.key.trim().is_empty() {
            out.push(format!("{}: needs a hostname", who(&item.key)));
            continue;
        }
        if !seen_keys.insert(item.key.as_str()) {
            out.push(format!("{}: used by two tiles in this edit", item.key));
        }
        if item.tile.name.trim().is_empty() {
            out.push(format!("{}: needs a name", item.key));
        }
        if item.tile.group.trim().is_empty() {
            out.push(format!("{}: needs a group", item.key));
        }
        if let Some(w) = item.tile.watch_every {
            if w < 10 {
                out.push(format!(
                    "{}: check every must be at least 10 s, not {w}",
                    item.key
                ));
            }
        }
        if let Some(d) = item.tile.down_after {
            if d < 10 {
                out.push(format!(
                    "{}: down after must be at least 10 s, not {d}",
                    item.key
                ));
            }
            if let Some(w) = item.tile.watch_every {
                if d < w {
                    out.push(format!(
                        "{}: down after ({d} s) must be at least check every ({w} s)",
                        item.key
                    ));
                }
            }
        }
    }
    out
}

/// The ops that turn the stack file's `tiles:` map into what `edit`
/// describes. `m` is the manifest as it reads now, to diff against and to
/// check a rename or delete's `origin` really exists. `manifest_text` is
/// the raw file, so a stack with no `tiles:` key at all yet can be told
/// apart from one whose map is merely empty — a nested `Set` needs the
/// mapping to exist first.
pub fn tiles_ops(
    m: &StackManifest,
    manifest_text: &str,
    edit: &TilesEdit,
) -> Result<Vec<Op>, String> {
    let has_tiles = serde_yaml::from_str::<Value>(manifest_text)
        .ok()
        .and_then(|d| d.get("tiles").cloned())
        .is_some_and(|v| !v.is_null());
    let mut ops = Vec::new();
    let mut tiles_created = false;
    for item in &edit.tiles {
        if item.delete {
            let origin = item.origin.as_ref().expect("checked by tiles_problems");
            if !m.tiles.contains_key(origin) {
                return Err(format!("{origin} is not a tile of this stack"));
            }
            ops.push(Op::Remove {
                path: vec![Seg::Key("tiles".into()), Seg::Key(origin.clone())],
            });
            continue;
        }
        match &item.origin {
            None => {
                if m.tiles.contains_key(&item.key) {
                    return Err(format!("{} is already a tile of this stack", item.key));
                }
                if !has_tiles && !tiles_created {
                    ops.push(Op::Set {
                        path: vec![Seg::Key("tiles".into())],
                        value: Value::Mapping(serde_yaml::Mapping::new()),
                    });
                    tiles_created = true;
                }
                ops.push(Op::Set {
                    path: vec![Seg::Key("tiles".into()), Seg::Key(item.key.clone())],
                    value: tile_value(&item.tile),
                });
            }
            Some(origin) => {
                let Some(now) = m.tiles.get(origin) else {
                    return Err(format!("{origin} is not a tile of this stack"));
                };
                if origin != &item.key {
                    if m.tiles.contains_key(&item.key) {
                        return Err(format!("{} is already a tile of this stack", item.key));
                    }
                    ops.push(Op::Remove {
                        path: vec![Seg::Key("tiles".into()), Seg::Key(origin.clone())],
                    });
                    ops.push(Op::Set {
                        path: vec![Seg::Key("tiles".into()), Seg::Key(item.key.clone())],
                        value: tile_value(&item.tile),
                    });
                    continue;
                }
                field_ops(&mut ops, &item.key, now, &item.tile);
            }
        }
    }
    Ok(ops)
}

fn tile_value(f: &TileFields) -> Value {
    serde_yaml::to_value(tile_of(f)).expect("Tile always serializes")
}

/// `tile_value`, exposed for the publish-app flow's "also create a tile"
/// step (`stackedit_publish`), which sets one tile directly rather than
/// going through a whole `TilesEdit`.
pub fn tile_value_for(f: &TileFields) -> Value {
    tile_value(f)
}

/// The ops that set one tile directly on a manifest's own text — the
/// publish-app flow's own shape (`stackedit::changes`'s `PublishApp` arm),
/// reused by `AddApp`'s per-app tile and the new-stack wizard's tile step:
/// `tiles:` itself may not be in the file yet (a fresh scaffold, or a stack
/// that never had one), and a nested `Op::Set` at `tiles.<hostname>` needs
/// the mapping to exist first — so an empty one is set ahead of it when
/// missing, applied within the same `yamledit::edit` call so the second op
/// sees the first one's result.
pub fn set_tile_ops(text: &str, hostname: &str, fields: &TileFields) -> Vec<Op> {
    let mut ops = Vec::new();
    if !has_tiles(text) {
        ops.push(Op::Set {
            path: vec![Seg::Key("tiles".into())],
            value: Value::Mapping(serde_yaml::Mapping::new()),
        });
    }
    ops.push(Op::Set {
        path: vec![Seg::Key("tiles".into()), Seg::Key(hostname.to_string())],
        value: tile_value_for(fields),
    });
    ops
}

/// Whether `text` already has a (non-null) `tiles:` key — checked by
/// parsing rather than a text search, the same care `set_tile_ops` and the
/// publish-app flow it was lifted from already took.
pub fn has_tiles(text: &str) -> bool {
    serde_yaml::from_str::<Value>(text)
        .ok()
        .and_then(|d| d.get("tiles").cloned())
        .is_some_and(|v| !v.is_null())
}

/// Set or remove only the fields of one tile that actually changed, so an
/// edit of an existing, unrenamed tile leaves the rest of its block (and
/// any comments inside it) alone.
fn field_ops(ops: &mut Vec<Op>, key: &str, now: &Tile, want: &TileFields) {
    let seg = |field: &str| {
        vec![
            Seg::Key("tiles".into()),
            Seg::Key(key.to_string()),
            Seg::Key(field.to_string()),
        ]
    };
    if now.name != want.name {
        ops.push(Op::Set {
            path: seg("name"),
            value: Value::from(want.name.as_str()),
        });
    }
    if now.group != want.group {
        ops.push(Op::Set {
            path: seg("group"),
            value: Value::from(want.group.as_str()),
        });
    }
    let want_order = want.order.unwrap_or(100);
    if now.order != want_order {
        ops.push(Op::Set {
            path: seg("order"),
            value: Value::from(want_order),
        });
    }
    let text_field =
        |ops: &mut Vec<Op>, field: &str, now: &Option<String>, want: &Option<String>| {
            let now = now.as_deref().map(str::trim).filter(|s| !s.is_empty());
            let want = want.as_deref().map(str::trim).filter(|s| !s.is_empty());
            if now != want {
                match want {
                    Some(v) => ops.push(Op::Set {
                        path: seg(field),
                        value: Value::from(v),
                    }),
                    None => ops.push(Op::Remove { path: seg(field) }),
                }
            }
        };
    text_field(ops, "description", &now.description, &want.description);
    text_field(ops, "url", &now.url, &want.url);
    text_field(ops, "watch_url", &now.watch_url, &want.watch_url);
    text_field(ops, "reading", &now.reading, &want.reading);
    let num_field = |ops: &mut Vec<Op>, field: &str, now: Option<u64>, want: Option<u64>| {
        if now != want {
            match want {
                Some(v) => ops.push(Op::Set {
                    path: seg(field),
                    value: Value::from(v),
                }),
                None => ops.push(Op::Remove { path: seg(field) }),
            }
        }
    };
    num_field(ops, "watch_every", now.watch_every, want.watch_every);
    num_field(ops, "down_after", now.down_after, want.down_after);
}

/// The groups already in use in this stack's own `tiles:` map — a starting
/// point for the form's group picker; the browser combines these across
/// stacks itself.
pub fn groups_in_use(m: &StackManifest) -> BTreeMap<String, ()> {
    m.tiles.values().map(|t| (t.group.clone(), ())).collect()
}

impl TilesEdit {
    /// The change in a few words, for the commit subject.
    pub fn describe(&self) -> String {
        let added = self
            .tiles
            .iter()
            .filter(|t| t.origin.is_none() && !t.delete)
            .count();
        let removed = self.tiles.iter().filter(|t| t.delete).count();
        let renamed = self
            .tiles
            .iter()
            .filter(|t| !t.delete && t.origin.as_deref().is_some_and(|o| o != t.key))
            .count();
        let mut parts = Vec::new();
        if added > 0 {
            parts.push(format!("{added} tile(s) added"));
        }
        if removed > 0 {
            parts.push(format!("{removed} tile(s) removed"));
        }
        if renamed > 0 {
            parts.push(format!("{renamed} tile(s) renamed"));
        }
        if parts.is_empty() {
            parts.push("tile fields changed".to_string());
        }
        format!("tiles: {}", parts.join(", "))
    }
}
