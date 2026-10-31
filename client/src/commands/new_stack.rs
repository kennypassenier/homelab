//! `homelab new`: scaffold a stack from a preset.

use super::*;

/// A `--flag value` pair from the arguments.
fn flag(args: &[String], k: &str) -> Option<String> {
    args.iter()
        .position(|a| a == k)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn num_flag(args: &[String], k: &str) -> Option<u64> {
    flag(args, k).map(|v| {
        v.parse()
            .unwrap_or_else(|_| die(&format!("{} takes a number, got '{}'", k, v)))
    })
}

/// Every `--no-data` flag names one path that keeps nothing of its
/// own (D79). The wizard asks this per directory; here it is
/// repeatable so a script can say the same thing.
fn no_data_paths(args: &[String]) -> Vec<String> {
    args.iter()
        .enumerate()
        .filter(|(_, a)| *a == "--no-data")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect()
}

fn find_preset<'a>(
    presets: &'a [homelab_client::scaffold::LoadedPreset],
    args: &[String],
) -> &'a homelab_client::scaffold::LoadedPreset {
    let preset_name = flag(args, "--preset")
        .unwrap_or_else(|| die("--preset is required; `homelab presets` lists them"));
    presets
        .iter()
        .find(|p| p.name == preset_name)
        .unwrap_or_else(|| {
            die(&format!(
                "no preset '{}' — `homelab presets` lists what there is",
                preset_name
            ))
        })
}

/// T65 / F136: a stack could only ever be created through the TUI
/// wizard. Twenty-one commands and not one of them scaffolded, so
/// every stack made without the TUI was hand-written — which is
/// how three stack files ended up claiming live vmids.
///
/// Same scaffolder, same preset catalogue, same defaults as the
/// wizard: `homelab new <name> --preset <p> --vmid <n> [...]`.
pub(crate) async fn cmd_new(c: &Ctx) {
    let args = &c.args;
    let name = args
        .get(2)
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| {
            die("usage: homelab new <name> --preset <preset> --vmid <n> \
                 [--ram MiB] [--cores N] [--disk GiB] [--swap MiB] \
                 [--no-data /appdata/<stack>/<app>-config]")
        });
    let presets_path = in_repo("presets");
    let presets_dir = presets_path.as_path();
    let presets = homelab_client::scaffold::scan_presets(presets_dir);
    let preset = find_preset(&presets, args);
    let vmid = num_flag(args, "--vmid")
        .unwrap_or_else(|| die("--vmid is required: the address and hostname are derived from it"))
        as u16;
    let d = homelab_client::scaffold::StackDefaults::default();
    let ram = num_flag(args, "--ram").unwrap_or(preset.meta.ram_mb as u64) as u32;
    let no_data = no_data_paths(args);
    let params = homelab_client::scaffold::StackParams {
        name: &name,
        vmid,
        ram_mb: ram,
        cores: num_flag(args, "--cores").unwrap_or(preset.meta.cores.unwrap_or(2) as u64) as u16,
        disk_gb: num_flag(args, "--disk").unwrap_or(preset.meta.disk_gb.unwrap_or(8) as u64) as u16,
        swap_mb: Some(num_flag(args, "--swap").unwrap_or(d.swap_for(ram) as u64) as u32),
        preset: Some(preset),
        no_data_paths: &no_data,
    };
    match homelab_client::scaffold::scaffold_stack(&stacks_base(), presets_dir, &params) {
        Ok(s) => print_scaffolded(&s, &name),
        Err(e) => die(&e),
    }
}

fn print_scaffolded(s: &homelab_client::scaffold::Scaffolded, name: &str) {
    println!(
        "{}✓ scaffolded {}{} — {} file(s)",
        C_GREEN,
        s.dir.display(),
        C_RESET,
        s.files.len()
    );
    for f in &s.files {
        println!("    {}", f);
    }
    println!(
        "\n  next: read the compose files, then `homelab plan stacks/{}`",
        name
    );
}

pub(crate) async fn cmd_templates(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    rpc(host, token, Command::ListTemplates).await;
}

pub(crate) async fn cmd_template_build(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    // B8: bake the golden template. Temp vmid defaults to 999.
    // O2: `--privileged` builds the second template. CT 105 and 106
    // are privileged and a clone cannot change that, so they need one.
    // `--base <vztmpl>` picks the OS. Absent keeps the host's default,
    // so this stays the command it has always been for a caller that
    // does not care which Debian it is baking.
    // fix-110 (see REGISTER.md): an argument that does not parse is
    // refused.
    let homelab_client::version::TemplateArgs {
        temp_vmid,
        version,
        unprivileged,
        base_template,
    } = homelab_client::version::template_build_args(&c.args[2..]).unwrap_or_else(|e| die(&e));
    let shown = base_template
        .as_deref()
        .map(homelab_core::ops::template::os_slug)
        .unwrap_or_else(|| "the host's default OS".into());
    println!(
        "{}▶ template build :: {}-homelab-v{}{} on temp vmid {}{}",
        C_CYAN,
        shown,
        version,
        if unprivileged { "" } else { "-priv" },
        temp_vmid,
        C_RESET
    );
    let build = Command::BuildTemplate {
        temp_vmid,
        version,
        unprivileged,
        base_template,
    };
    rpc(host, token, build).await;
}

pub(crate) async fn cmd_presets() {
    // G2: list the data-driven preset catalog (local, no network).
    for pr in homelab_client::scaffold::scan_presets(&in_repo("presets")) {
        let src = if pr.dir.is_some() {
            ""
        } else {
            " (built-in fallback)"
        };
        println!(
            "{:<14} {:>5} MiB  {}  [{}]{}",
            pr.name,
            pr.meta.ram_mb,
            pr.meta.description,
            if pr.apps.is_empty() {
                "no apps".to_string()
            } else {
                pr.apps.join(", ")
            },
            src
        );
    }
}
