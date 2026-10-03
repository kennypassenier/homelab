//! feat-stacks-3: the new-stack wizard's request, checked against what the
//! fleet and the working copy already hold before anything is scaffolded.
//! The files themselves come from the client's own scaffold (the same one
//! `homelab new` and the TUI wizard use), with the repository's presets.
//! Pure.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::actions::valid_stack_name;

/// What the wizard sends.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NewStack {
    pub name: String,
    pub vmid: u16,
    /// A preset directory under `presets/`, or [`EMPTY_PRESET`] for a
    /// stack with no apps (New stack's "Empty" route).
    pub preset: String,
    pub ram_mb: u32,
    pub cores: u16,
    pub disk_gb: u16,
    /// None: the scaffold's own formula (a quarter of the memory, 512 MB to
    /// 2 GB).
    #[serde(default)]
    pub swap_mb: Option<u32>,
    /// `/appdata` paths whose app keeps nothing of its own (`no_data`).
    #[serde(default)]
    pub no_data: Vec<String>,
    /// feat-tiles-3: the wizard's own optional Tile step, applied to the
    /// freshly scaffolded manifest in the SAME commit as the stack itself
    /// (`edit::prepare_new`) — `None` is the step's "no tile" answer.
    #[serde(default)]
    pub tile: Option<NewStackTile>,
}

/// feat-tiles-3: one tile, as the new-stack wizard's Tile step sends it —
/// `hostname` becomes the tile's key in `tiles:`, the rest
/// `stackedit_tiles::TileFields` (minus `url`/`reading`, which the wizard
/// does not ask for).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NewStackTile {
    pub hostname: String,
    pub name: String,
    pub group: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub watch_every: Option<u64>,
    #[serde(default)]
    pub down_after: Option<u64>,
}

impl NewStackTile {
    pub fn fields(&self) -> super::stackedit_tiles::TileFields {
        super::stackedit_tiles::TileFields {
            name: self.name.clone(),
            group: self.group.clone(),
            order: None,
            description: self.description.clone(),
            url: None,
            watch_url: None,
            reading: None,
            watch_every: self.watch_every,
            down_after: self.down_after,
        }
    }
}

/// redesign-stacks-8: the preset name New stack's "Empty" route sends: no
/// preset at all, so the client's scaffold writes a stack with no apps
/// (`scaffold_stack` with `preset: None`), whether or not the repository
/// carries a `custom` preset.
pub const EMPTY_PRESET: &str = "";

/// What is taken already: names and vmids from the host's fleet and the
/// working copy's stack directories, and every address a stack file uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Taken {
    pub names: BTreeSet<String>,
    pub vmids: BTreeSet<u16>,
    pub ips: BTreeSet<String>,
}

/// The address the scaffold gives a vmid: `10.10.10.<vmid - 100>`.
pub fn ip_for(vmid: u16) -> Option<String> {
    let last = vmid.checked_sub(100)?;
    (2..=254)
        .contains(&last)
        .then(|| format!("10.10.10.{last}"))
}

/// The first vmid from 104 up that is free, not on the no-touch list and
/// has an address.
pub fn suggest_vmid(taken: &Taken) -> Option<u16> {
    (104..=354).find(|v| {
        !taken.vmids.contains(v)
            && !homelab_core::safety::DEFAULT_NO_TOUCH.contains(v)
            && ip_for(*v).is_some_and(|ip| !taken.ips.contains(&ip))
    })
}

/// Every reason the request cannot be scaffolded, in the order the wizard
/// asks: name, container number, preset, size.
pub fn problems(req: &NewStack, taken: &Taken, presets: &[String]) -> Vec<(String, String)> {
    let mut out = identity_problems(&req.name, req.vmid, taken);
    let mut say = |field: &str, why: String| out.push((field.to_string(), why));
    if req.preset != EMPTY_PRESET && !presets.contains(&req.preset) {
        say(
            "preset",
            format!("there is no preset called {}", req.preset),
        );
    }
    size_problems(req, &mut say);
    if let Some(tile) = &req.tile {
        if tile.hostname.trim().is_empty() {
            say("tile_hostname", "a tile needs a hostname".into());
        }
        if tile.name.trim().is_empty() {
            say("tile_name", "a tile needs a name".into());
        }
        if tile.group.trim().is_empty() {
            say("tile_group", "a tile needs a group".into());
        }
    }
    out
}

/// The new stack's name and container number, as the wizard and an import
/// (TUI parity, `homelab import`) check them: a free, valid name; a free
/// vmid off the no-touch list whose address exists.
pub fn identity_problems(name: &str, vmid: u16, taken: &Taken) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut say = |field: &str, why: String| out.push((field.to_string(), why));
    if !valid_stack_name(name) || name.len() > 32 {
        say(
            "name",
            "a stack name is 1 to 32 lowercase letters, digits and dashes, not starting with a dash"
                .into(),
        );
    } else if taken.names.contains(name) {
        say("name", format!("there is a stack called {} already", name));
    } else if name == "_host" || name == "new" {
        say("name", format!("{} is reserved", name));
    }
    if homelab_core::safety::DEFAULT_NO_TOUCH.contains(&vmid) {
        say(
            "vmid",
            format!(
                "{} is on the no-touch list (OPNsense, Home Assistant, 102, 103)",
                vmid
            ),
        );
    } else if taken.vmids.contains(&vmid) {
        say("vmid", format!("CT {} exists already", vmid));
    } else {
        match ip_for(vmid) {
            None => say(
                "vmid",
                "the container number must be 102 to 354, so its address 10.10.10.<number − 100> exists"
                    .into(),
            ),
            Some(ip) if taken.ips.contains(&ip) => {
                say("vmid", format!("{ip} is used by another stack already"))
            }
            Some(_) => {}
        }
    }
    out
}

fn size_problems(req: &NewStack, say: &mut impl FnMut(&str, String)) {
    if !(128..=262_144).contains(&req.ram_mb) {
        say("ram_mb", "memory must be from 128 MB to 256 GB".into());
    }
    if !(1..=64).contains(&req.cores) {
        say("cores", "cores must be from 1 to 64".into());
    }
    if !(2..=4096).contains(&req.disk_gb) {
        say("disk_gb", "disk must be from 2 to 4096 GB".into());
    }
    if req.swap_mb.is_some_and(|s| s > 65_536) {
        say("swap_mb", "swap must be at most 64 GB".into());
    }
    for p in &req.no_data {
        if !p.starts_with("/appdata/") {
            say(
                "no_data",
                format!("{p} is not an /appdata path of this stack"),
            );
        }
    }
}
