//! Shell snippets and unit-file readings the install path needs: who the
//! service runs as, which directories it declares, how its env file is
//! seeded. Pure string builders, no I/O.

use crate::native::NativeServiceManifest;
use crate::ops::util::shq;

/// The user a unit runs as, read from its `User=` line.
///
/// The orchestrator installs a binary as root; the chassis kit's own update
/// then wants to keep the version it replaces beside it, and a service cannot
/// move root's file out of the way. On 2026-09-10 that surfaced as
/// `cannot keep the previous binary at /opt/kyu/bin/kyu.prev: Operation not
/// permitted` on all three services of CT 109 — a deploy that had reported
/// success three times. The unit is the only place that says who the service
/// is, and it is already in hand here, so nothing new has to be declared.
pub fn unit_user(unit_file: &str) -> Option<String> {
    unit_file
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("User="))
        .map(str::trim)
        .filter(|u| !u.is_empty() && !u.starts_with('%'))
        .map(str::to_string)
}

/// Hand the service its own program directory.
///
/// Recursive on the directory holding the binary, which is exactly what the
/// kit needs to write its `.prev` beside it. Anything a service needs beyond
/// that, its own unit says and its own deploy does.
pub fn own_program_dir_script(user: &str, binary: &str) -> String {
    format!(
        "d=$(dirname {bin}) && chown -R {u}:{u} \"$d\" && chown {u}:{u} {bin}",
        bin = shq(binary),
        u = user
    )
}

/// native-first-install-1: make every directory the manifest declares
/// (`data_dirs` and the env file's directory) that does not exist yet, owned
/// by the unit's `User=` (root when it has none). Prints `created` when it
/// made anything; empty script when there is nothing declared.
pub fn create_dirs_script(m: &NativeServiceManifest, user: Option<&str>) -> String {
    let mut dirs: Vec<String> = m.data_dirs.clone();
    if let Some(env) = &m.env_file
        && let Some((dir, _)) = env.rsplit_once('/')
        && !dir.is_empty()
    {
        dirs.push(dir.to_string());
    }
    dirs.dedup();
    let mut script = String::new();
    for d in dirs {
        let q = shq(&d);
        let chown = match user {
            Some(u) => format!(" && chown {}: {}", shq(u), q),
            None => String::new(),
        };
        script.push_str(&format!(
            "[ -d {q} ] || {{ mkdir -p {q}{chown} && echo created; }}; "
        ));
    }
    if script.is_empty() {
        return script;
    }
    script.push_str("true");
    script
}

/// native-first-install-1: runs INSIDE the container. A non-empty env file
/// is kept (prints `KEEP`). Otherwise `cmd` runs under a pseudo-terminal
/// (`script -qec`, because chassis `gen-secret` refuses a non-terminal
/// stdout), only `KEY=value` lines are kept, and they land in `env` with mode
/// 0600 owned by `user`. Prints `KEY <name>` per key, never a value. Exit 10:
/// the command failed; 11: it printed no KEY=value lines; 12: writing failed.
pub fn seed_env_script(env: &str, cmd: &str, user: Option<&str>) -> String {
    let f = shq(env);
    let tmp = shq(&format!("{env}.homelab-new"));
    let chown = match user {
        Some(u) => format!("chown {}: {tmp} || {{ rm -f {tmp}; exit 12; }}; ", shq(u)),
        None => String::new(),
    };
    format!(
        "if [ -s {f} ]; then echo KEEP; exit 0; fi; umask 077; \
         out=$(script -qec {c} /dev/null) || exit 10; \
         printf '%s\n' \"$out\" | tr -d '\\r' | grep -E '^[A-Za-z_][A-Za-z0-9_]*=' > {tmp}; \
         [ -s {tmp} ] || {{ rm -f {tmp}; exit 11; }}; \
         chmod 600 {tmp} || {{ rm -f {tmp}; exit 12; }}; {chown}\
         mv -f {tmp} {f} || {{ rm -f {tmp}; exit 12; }}; \
         cut -d= -f1 {f} | sed 's/^/KEY /'",
        c = shq(cmd),
    )
}
