//! Settings a stack depends on that live outside the stacks: a DNS override
//! in OPNsense, a tunnel in Cloudflare. They cannot be declared as files, but
//! they belong to a stack, so the stack file lists them in `external:`. The
//! list goes into the disaster-recovery runbook (what to set again after a
//! rebuild) and is the place to look when the stack's address changes.
//!
//! `{ip}` and `{hostname}` in a line are filled from the same stack file, so
//! the line says the address it should hold today, not the one it held when it
//! was written.

use serde::Deserialize;

/// The read-only questions a `check:` may name. Anything else is refused when
/// the stack file is read, so a typo is not a silent "no check".
const CHECK_KINDS: &[&str] = &["unbound-override"];

/// One setting outside the stacks, as `lxc-compose.yml` declares it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalSetting {
    /// The system it lives in: `OPNsense`, `Cloudflare`.
    pub system: String,
    /// Where to find it there: a menu path or a page.
    #[serde(rename = "where")]
    pub place: String,
    /// What it must say, with `{ip}` and `{hostname}` for this stack's values.
    pub what: String,
    /// A read-only question the nightly round could ask the system, as
    /// `<kind> <argument>`; absent: only a person can tell.
    #[serde(default)]
    pub check: Option<String>,
    /// Why it exists, one line.
    #[serde(default)]
    pub why: Option<String>,
}

/// Refuse a setting that cannot be used: an empty field, or a check of a kind
/// nothing implements.
pub fn validate(items: &[ExternalSetting]) -> Result<(), String> {
    for (n, i) in items.iter().enumerate() {
        let label = format!("external[{n}] ({} / {})", i.system, i.place);
        if [&i.system, &i.place, &i.what]
            .iter()
            .any(|f| f.trim().is_empty())
        {
            return Err(format!("{label}: system, where and what must not be empty"));
        }
        if let Some(check) = &i.check {
            let kind = check.split_whitespace().next().unwrap_or_default();
            if !CHECK_KINDS.contains(&kind) {
                return Err(format!(
                    "{label}: check '{check}' is not a known kind :: known kinds are [{}]",
                    CHECK_KINDS.join(", ")
                ));
            }
        }
    }
    Ok(())
}

/// `{ip}` and `{hostname}` replaced by this stack\'s values; `ip` may carry the
/// prefix length (`10.10.10.4/24`), which is dropped.
pub fn fill(text: &str, ip: &str, hostname: &str) -> String {
    let bare_ip = ip.split('/').next().unwrap_or(ip);
    text.replace("{ip}", bare_ip)
        .replace("{hostname}", hostname)
}

/// The runbook lines for one stack: what to set again outside the stack after
/// a rebuild. Empty when the stack declares none.
pub fn runbook_lines(items: &[ExternalSetting], ip: &str, hostname: &str) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "- Outside the stack (set by hand, not rebuilt by any deploy; check these after a rebuild or when this stack\'s address changes):\n",
    );
    for i in items {
        out.push_str(&format!(
            "  - **{}**, {}: {}",
            i.system,
            i.place,
            fill(&i.what, ip, hostname)
        ));
        if let Some(why) = &i.why {
            out.push_str(&format!(" ({why})"));
        }
        if let Some(check) = &i.check {
            // Declared, not yet run by anything: said so, never implied.
            out.push_str(&format!(
                " (read-only check declared: `{check}`; no round runs it yet)"
            ));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(check: Option<&str>) -> ExternalSetting {
        ExternalSetting {
            system: "OPNsense".into(),
            place: "Services → Unbound DNS → Host Overrides".into(),
            what: "kitchen.kp-soft.dev A {ip}".into(),
            check: check.map(str::to_string),
            why: Some("the home route".into()),
        }
    }

    #[test]
    fn the_address_comes_from_the_stack_file_without_its_prefix() {
        assert_eq!(
            fill("A {ip} on {hostname}", "10.10.10.4/24", "104-app-gateway"),
            "A 10.10.10.4 on 104-app-gateway"
        );
    }

    #[test]
    fn only_a_known_check_kind_is_accepted() {
        assert!(validate(&[item(Some("unbound-override kitchen.kp-soft.dev"))]).is_ok());
        assert!(validate(&[item(None)]).is_ok());
        let err = validate(&[item(Some("dns-lookup kitchen"))]).unwrap_err();
        assert!(err.contains("known kinds are [unbound-override]"), "{err}");
    }

    #[test]
    fn an_empty_field_is_refused() {
        let mut bad = item(None);
        bad.what = "  ".into();
        assert!(validate(&[bad]).is_err());
    }

    #[test]
    fn the_runbook_names_the_value_it_should_hold_today() {
        let text = runbook_lines(
            &[item(Some("unbound-override kitchen.kp-soft.dev"))],
            "10.10.10.4/24",
            "104-app-gateway",
        );
        assert!(text.contains("**OPNsense**"));
        assert!(text.contains("kitchen.kp-soft.dev A 10.10.10.4 (the home route)"));
        assert!(text.contains("read-only check declared: `unbound-override kitchen.kp-soft.dev`"));
        assert_eq!(runbook_lines(&[], "10.10.10.4/24", "x"), "");
    }
}
