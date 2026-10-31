//! `homelab tool status | update <name> | rollback <name>`: the managed tools
//! on the host (latch first). Only the argument reading lives here; the host
//! does the work.

use homelab_proto::Command;

const USAGE: &str = "usage: homelab tool status | update <name> | rollback <name>";

/// The request one `homelab tool …` command line stands for.
pub fn command(args: &[String]) -> Result<Command, String> {
    let name = |what: &str| {
        args.get(3)
            .filter(|n| !n.starts_with('-'))
            .cloned()
            .ok_or_else(|| format!("homelab tool {what} needs a tool name :: {USAGE}"))
    };
    match args.get(2).map(String::as_str) {
        Some("status") => Ok(Command::ToolStatus),
        Some("update") => Ok(Command::ToolUpdate {
            name: name("update")?,
        }),
        Some("rollback") => Ok(Command::ToolRollback {
            name: name("rollback")?,
        }),
        Some(other) => Err(format!("unknown tool command '{other}' :: {USAGE}")),
        None => Err(USAGE.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        ["homelab", "tool"]
            .iter()
            .chain(words)
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn reads_the_three_commands() {
        assert!(matches!(
            command(&args(&["status"])),
            Ok(Command::ToolStatus)
        ));
        assert!(matches!(
            command(&args(&["update", "latch"])),
            Ok(Command::ToolUpdate { name }) if name == "latch"
        ));
        assert!(matches!(
            command(&args(&["rollback", "latch"])),
            Ok(Command::ToolRollback { name }) if name == "latch"
        ));
    }

    #[test]
    fn a_missing_name_or_a_flag_in_its_place_is_refused() {
        assert!(command(&args(&["update"])).is_err());
        assert!(command(&args(&["rollback", "--force"])).is_err());
        assert!(command(&args(&[])).is_err());
        assert!(command(&args(&["install"])).is_err());
    }
}
