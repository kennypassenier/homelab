//! The security probes behind the doctor: exposure, loose files, privileged containers,
//! host-meta age, the restore drill, the password file, Drive space and route files.

use super::ProbeContext;
use crate::*;
use homelab_core::doctor::{DrillProbe, DriveSpace, Exposure, Freshness, Privileged, Probes};

type Out = Result<homelab_core::executor::CmdOutput, CoreError>;

/// Private files: group or world bits set is a finding. A path that does not
/// exist prints nothing on stdout, which is right: absent is not loose.
fn stat_cmd(pc: &ProbeContext) -> Cmd {
    let private = [
        pc.config_path.clone(),
        pc.password_file.clone(),
        format!("{}/secrets", pc.state_dir),
        format!("{}/tls-key.pem", pc.state_dir),
        format!("{}/audit.log", pc.state_dir),
        format!("{}/incidents", pc.state_dir),
    ];
    let mut args: Vec<&str> = vec!["-c", "%a %n"];
    args.extend(private.iter().map(String::as_str));
    Cmd::new("stat", &args, 20)
}

/// Privileged containers. Templates are left out (the golden -priv template
/// is privileged on purpose and never runs), and the no-touch guests'
/// configurations are not even read: the list is law.
fn privileged_cmd(pc: &ProbeContext) -> Cmd {
    let skip = pc
        .no_touch
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("|");
    let script = format!(
        "for f in /etc/pve/lxc/*.conf; do v=$(basename \"$f\" .conf); \
         case \"$v\" in {}) continue;; esac; \
         grep -q '^template: 1' \"$f\" && continue; \
         grep -q '^unprivileged: 1' \"$f\" || echo \"$v\"; done",
        if skip.is_empty() { "-".into() } else { skip }
    );
    Cmd::new("sh", &["-c", &script], 20)
}

/// fix-130 (second half): the same `ls -1A` the fleet check runs, so doctor can
/// name an unowned route file without waiting for a nightly round. Any
/// extension counts — a `.bak` is still a file nobody owns.
fn routes_cmd(pc: &ProbeContext) -> Cmd {
    homelab_core::executor::attach_sh(
        pc.gateway_vmid,
        &format!("ls -1A '{}'", pc.gateway_routes_dir),
        30,
    )
}

fn loose_files(out: &homelab_core::executor::CmdOutput) -> Vec<String> {
    out.stdout
        .lines()
        .filter_map(|l| l.split_once(' '))
        .filter(|(mode, _)| u32::from_str_radix(mode, 8).is_ok_and(|m| m & 0o077 != 0))
        .map(|(mode, path)| format!("{} ({})", path, mode))
        .collect()
}

fn privileged_containers(out: &homelab_core::executor::CmdOutput, pc: &ProbeContext) -> Privileged {
    let mut vmids: Vec<u16> = out
        .stdout
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect();
    vmids.sort_unstable();
    let outside_policy = vmids
        .iter()
        .copied()
        .filter(|v| !pc.privileged_vmids.contains(v))
        .collect();
    Privileged {
        vmids,
        outside_policy,
    }
}

/// Host-meta and the restore drill, from the record.
fn record_probes(
    hs: &homelab_core::state::HostState,
    pc: &ProbeContext,
    now_unix: u64,
) -> (Freshness, DrillProbe) {
    let age = |t: u64| (t > 0).then(|| now_unix.saturating_sub(t) / 3600);
    let host_meta = Freshness {
        age_h: age(hs.last_host_meta),
    };
    let drill = DrillProbe {
        age_h: age(hs.last_restore_drill),
        interval_h: pc.drill_interval_s / 3600,
        failing: hs
            .restore_drills
            .iter()
            .filter_map(|(repo, r)| r.last_error.as_ref().map(|e| format!("{}: {}", repo, e)))
            .collect(),
    };
    (host_meta, drill)
}

/// fix-130 (second half): judged the same way the fleet check judges it
/// (`fleetcheck::unowned_route_files`) — only when both the listing and the
/// state parsed; an unreadable gateway is no fact, not an empty one.
fn unowned_routes(hs: &homelab_core::state::HostState, routes: &Out) -> Option<Vec<String>> {
    let out = routes.as_ref().ok().filter(|o| o.success())?;
    let on_disk: Vec<String> = out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    Some(homelab_core::ops::fleetcheck::unowned_route_files(
        hs, &on_disk,
    ))
}

/// Drive's space, trash included: pruned packs stay in the trash and count
/// against the quota until it is emptied.
fn drive_space(about: Option<Out>) -> Option<DriveSpace> {
    let out = about?.ok()?;
    let v = serde_json::from_str::<serde_json::Value>(&out.stdout).ok()?;
    let n = |k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    (n("total") > 0).then(|| DriveSpace {
        total: n("total"),
        free: n("free"),
        trashed: n("trashed"),
    })
}

/// fix-130 (expert panel, doctor-checks-too-little, 2026-09-27): the probes
/// doctor lacked. Cheap reads only: `stat`, one loop over the container
/// configurations, state, `test -s` and one `rclone about`. A probe that
/// cannot be read stays `None`, so doctor leaves its line out rather than
/// guess.
pub(crate) async fn gather_security_probes(
    exec: &dyn Executor,
    pc: &ProbeContext,
    now_unix: u64,
    probes: &mut Probes,
) {
    probes.exposure = Some(Exposure {
        listen: pc.listen.clone(),
        exec_enabled: pc.exec_enabled,
    });
    let (stat, privileged, routes) = (stat_cmd(pc), privileged_cmd(pc), routes_cmd(pc));
    let state_path = format!("{}/state.json", pc.state_dir);
    // Present and not empty; the content never leaves the file.
    let password_cmd = Cmd::new("test", &["-s", &pc.password_file], 10);
    let about_cmd = Cmd::new("rclone", &["about", "gdrive:", "--json"], 60);
    let offsite_configured = probes.offsite_configured;
    let about_fut = async {
        if offsite_configured {
            Some(exec.run(&about_cmd).await)
        } else {
            None
        }
    };
    // ops::pool: six independent reads, overlapped; polled in the order they
    // used to run.
    let (stat_out, privileged_out, state_raw, password_out, about_out, routes_out) = futures_util::join!(
        exec.run(&stat),
        exec.run(&privileged),
        exec.read_file(&state_path),
        exec.run(&password_cmd),
        about_fut,
        exec.run(&routes)
    );
    probes.loose_files = stat_out.ok().map(|o| loose_files(&o));
    probes.privileged = privileged_out.ok().map(|o| privileged_containers(&o, pc));
    let hs: Option<homelab_core::state::HostState> = state_raw
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());
    if let Some(hs) = &hs {
        let (host_meta, drill) = record_probes(hs, pc, now_unix);
        probes.host_meta = Some(host_meta);
        probes.restore_drill = Some(drill);
        probes.unowned_route_files = unowned_routes(hs, &routes_out);
    }
    probes.password_file_ok = password_out.ok().map(|o| o.success());
    probes.drive = drive_space(about_out);
}
