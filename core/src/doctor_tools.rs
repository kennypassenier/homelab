//! The doctor's view of managed tools (latch): one check per tool, built from
//! what the host read. Pure: the host does the reading.

use crate::doctor::{Check, Health};

/// What the host read about one managed tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolProbe {
    pub name: String,
    /// `None`: not installed.
    pub installed: Option<String>,
    /// The newest signed release, when it could be read.
    pub newest: Option<String>,
    /// A newer release that waits for a person: its version and whether it
    /// is a major step.
    pub held: Option<(String, bool)>,
    /// Why the release could not be read.
    pub error: Option<String>,
}

fn check(name: &str, health: Health, detail: String, remedy: Option<String>) -> Check {
    Check {
        name: format!("managed program {name}"),
        health,
        detail,
        remedy,
    }
}

/// One check per probed tool.
pub fn tool_checks(probes: &[ToolProbe]) -> Vec<Check> {
    probes.iter().map(tool_check).collect()
}

fn tool_check(p: &ToolProbe) -> Check {
    let n = &p.name;
    if let Some(e) = &p.error {
        return check(
            n,
            Health::Warn,
            format!("the newest release could not be read: {e}"),
            Some("check that the host reaches GitHub, then run `homelab tool status`".into()),
        );
    }
    let Some(have) = &p.installed else {
        return check(
            n,
            Health::Fail,
            "not installed".into(),
            Some(format!(
                "run `homelab tool update {n}` or press Update a program"
            )),
        );
    };
    match &p.held {
        Some((version, true)) => check(
            n,
            Health::Warn,
            format!("{have} installed; {version} is a major release and waits for you"),
            Some(format!(
                "read the release notes of {version}, then press Update a program for {n} \
                 (or `homelab tool update {n}`); `homelab tool rollback {n}` undoes it"
            )),
        ),
        Some((version, false)) => check(
            n,
            Health::Warn,
            format!("{have} installed; {version} is available and {n} is held"),
            Some(format!(
                "press Update a program for {n} (or `homelab tool update {n}`)"
            )),
        ),
        None => check(
            n,
            Health::Ok,
            match &p.newest {
                Some(newest) if newest != have => {
                    format!("{have} installed; {newest} installs tonight")
                }
                _ => format!("{have} installed, current"),
            },
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe() -> ToolProbe {
        ToolProbe {
            name: "latch".into(),
            installed: Some("2.7.0".into()),
            newest: Some("2.7.0".into()),
            held: None,
            error: None,
        }
    }

    #[test]
    fn a_current_tool_is_ok() {
        let c = tool_check(&probe());
        assert_eq!(c.health, Health::Ok);
        assert!(c.detail.contains("2.7.0"));
    }

    #[test]
    fn a_held_major_warns_and_says_what_to_press() {
        let mut p = probe();
        p.newest = Some("3.0.0".into());
        p.held = Some(("3.0.0".into(), true));
        let c = tool_check(&p);
        assert_eq!(c.health, Health::Warn);
        assert!(c.detail.contains("major"));
        assert!(c.remedy.unwrap().contains("Update a program"));
    }

    #[test]
    fn a_missing_tool_fails_and_an_unreadable_release_only_warns() {
        let mut p = probe();
        p.installed = None;
        assert_eq!(tool_check(&p).health, Health::Fail);
        let mut p = probe();
        p.error = Some("timeout".into());
        assert_eq!(tool_check(&p).health, Health::Warn);
    }
}
