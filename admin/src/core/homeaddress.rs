//! eten-home-ip-1 (Kenny, 2026-10-22: "mee in de volgende release van admin
//! dashboard, daar kan dan ergens de optie komen om het adres te updaten bij
//! cloudflare. Dit is niet iets dat Kitchen moet doen"): the house has no
//! static public address. Two Cloudflare Access policies let the house in
//! without a login (decision `bypass`, one `ip` include such as
//! `203.0.113.7/32`): the reusable policy "home address, no login" and, where
//! one was made by hand, a bypass policy inside an application. When the
//! provider changes the address the house gets login screens and eten refuses
//! it. This module decides, without I/O, what the address is, what the
//! policies hold, whether they agree and exactly which policies to rewrite.
//!
//! The rules, all tested:
//! * only a policy whose decision is `bypass` is ever read or rewritten;
//! * only an `ip` include that is ONE address (`/32`, `/128`, or bare) is
//!   treated as the home address; a wider range is somebody else's rule and
//!   is never touched, so the rewrite can never widen access;
//! * only an include holding the OLD address is replaced, by the new address
//!   as a single-address range; every other include, exclude and require
//!   rule is carried over unchanged;
//! * two different old addresses at once is not guessed at: it is refused
//!   with both listed.

use std::net::IpAddr;

use serde::Serialize;
use serde_json::{Value, json};

/// Where the dashboard asks what its public address is, unless
/// `HOMELAB_ADMIN_HOME_ADDRESS_URL` says otherwise. The answer is the bare
/// address as plain text.
pub const DEFAULT_LOOKUP_URL: &str = "https://api.ipify.org";
/// Cloudflare's API, unless `HOMELAB_ADMIN_CLOUDFLARE_API` says otherwise
/// (a test or a proxy).
pub const DEFAULT_API_BASE: &str = "https://api.cloudflare.com/client/v4";
/// How often the dashboard reads both sides for the Inbox, in seconds.
pub const DEFAULT_CHECK_EVERY_S: u64 = 900;

pub const ENV_ACCOUNT: &str = "HOMELAB_ADMIN_CLOUDFLARE_ACCOUNT_ID";
pub const ENV_TOKEN: &str = "HOMELAB_ADMIN_CLOUDFLARE_ACCESS_TOKEN";
pub const ENV_LOOKUP: &str = "HOMELAB_ADMIN_HOME_ADDRESS_URL";

/// The page section the Inbox row points at.
pub const SECTION_PATH: &str = "/settings?section=home-address";

/// What the setting for Cloudflare lacks, in the words the page shows; None
/// when both values are there.
pub fn config_problem(account: Option<&str>, token: Option<&str>) -> Option<String> {
    let miss = |v: Option<&str>| v.is_none_or(|s| s.trim().is_empty());
    let mut lacking = Vec::new();
    if miss(account) {
        lacking.push(ENV_ACCOUNT);
    }
    if miss(token) {
        lacking.push(ENV_TOKEN);
    }
    if lacking.is_empty() {
        return None;
    }
    Some(format!(
        "Cloudflare is not set up for the dashboard yet: {} {} missing. Put the Cloudflare account id and an API token that may edit Access policies into the dashboard's admin.env (through latch), then restart the dashboard.",
        lacking.join(" and "),
        if lacking.len() == 1 { "is" } else { "are" },
    ))
}

/// The plain-text answer of a "what is my address" service, as the address
/// of the house. A private, loopback, link-local or unspecified answer is
/// refused: it would be a proxy's or a broken service's, and writing it
/// into Cloudflare would lock the house out.
pub fn parse_public_address(body: &str) -> Result<IpAddr, String> {
    let text = body.trim();
    let ip: IpAddr = text.parse().map_err(|_| {
        let shown: String = text.chars().take(40).collect();
        format!("The address service did not answer with an address (it said {shown:?})")
    })?;
    let bad = match ip {
        IpAddr::V4(v) => {
            v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_broadcast()
                || v.is_multicast()
        }
        IpAddr::V6(v) => {
            v.is_loopback()
                || v.is_unspecified()
                || v.is_multicast()
                || (v.segments()[0] & 0xfe00) == 0xfc00
                || (v.segments()[0] & 0xffc0) == 0xfe80
        }
    };
    if bad {
        return Err(format!(
            "The address service answered {ip}, which is not a public address"
        ));
    }
    Ok(ip)
}

/// The range Cloudflare holds for exactly this one address.
pub fn range_of(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(_) => format!("{ip}/32"),
        IpAddr::V6(_) => format!("{ip}/128"),
    }
}

/// The address of a range that is exactly one address (`1.2.3.4/32`,
/// `::1/128`, or a bare address); None for any wider range.
pub fn host_of(range: &str) -> Option<IpAddr> {
    let range = range.trim();
    match range.split_once('/') {
        None => range.parse().ok(),
        Some((a, bits)) => {
            let ip: IpAddr = a.parse().ok()?;
            let full = if ip.is_ipv4() { 32 } else { 128 };
            (bits.parse::<u8>().ok()? == full).then_some(ip)
        }
    }
}

/// Where a policy lives, which decides the call that writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Place {
    /// A reusable account policy (`/access/policies/{id}`).
    Reusable,
    /// A policy made inside one application (`/access/apps/{app}/policies/{id}`).
    App { app_id: String, app_name: String },
}

/// One bypass policy that holds at least one single-address `ip` include.
#[derive(Debug, Clone, PartialEq)]
pub struct BypassPolicy {
    pub id: String,
    pub name: String,
    pub place: Place,
    /// The single addresses its `ip` includes hold, in order.
    pub addresses: Vec<IpAddr>,
    /// The policy as Cloudflare returned it.
    pub body: Value,
}

impl BypassPolicy {
    /// "reusable policy" or "application <name>", for the page.
    pub fn where_words(&self) -> String {
        match &self.place {
            Place::Reusable => "reusable policy".into(),
            Place::App { app_name, .. } => format!("application {app_name}"),
        }
    }
}

/// The single addresses held by the `ip` includes of one policy body.
fn ip_includes(body: &Value) -> Vec<IpAddr> {
    body.get("include")
        .and_then(Value::as_array)
        .map(|rules| {
            rules
                .iter()
                .filter_map(|r| r.pointer("/ip/ip").and_then(Value::as_str))
                .filter_map(host_of)
                .collect()
        })
        .unwrap_or_default()
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn bypass_of(body: &Value, place: Place) -> Option<BypassPolicy> {
    if str_of(body, "decision") != "bypass" {
        return None;
    }
    let id = str_of(body, "id");
    if id.is_empty() {
        return None;
    }
    let addresses = ip_includes(body);
    if addresses.is_empty() {
        return None;
    }
    Some(BypassPolicy {
        id: id.to_string(),
        name: str_of(body, "name").to_string(),
        place,
        addresses,
        body: body.clone(),
    })
}

/// Every bypass policy with a single-address `ip` include: the account's
/// reusable policies, then the policies written inside applications. A
/// reusable policy an application only references shows up once, as
/// reusable; an application's list holds it by id (or flagged `reusable`).
pub fn bypass_policies(reusable: &[Value], apps: &[Value]) -> Vec<BypassPolicy> {
    let mut out: Vec<BypassPolicy> = reusable
        .iter()
        .filter_map(|p| bypass_of(p, Place::Reusable))
        .collect();
    let shared: std::collections::BTreeSet<&str> = reusable
        .iter()
        .map(|p| str_of(p, "id"))
        .filter(|id| !id.is_empty())
        .collect();
    for app in apps {
        let app_id = str_of(app, "id");
        if app_id.is_empty() {
            continue;
        }
        let app_name = str_of(app, "name").to_string();
        let Some(list) = app.get("policies").and_then(Value::as_array) else {
            continue;
        };
        for p in list {
            if !p.is_object() || p.get("reusable").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            if shared.contains(str_of(p, "id")) {
                continue;
            }
            if let Some(b) = bypass_of(
                p,
                Place::App {
                    app_id: app_id.to_string(),
                    app_name: app_name.clone(),
                },
            ) {
                out.push(b);
            }
        }
    }
    out
}

/// The distinct single addresses the bypass policies hold, sorted.
pub fn held_addresses(policies: &[BypassPolicy]) -> Vec<IpAddr> {
    let mut all: Vec<IpAddr> = policies
        .iter()
        .flat_map(|p| p.addresses.iter().copied())
        .collect();
    all.sort();
    all.dedup();
    all
}

/// Whether the house's address and the policies agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Every bypass policy holds the current address.
    Match,
    /// At least one holds another address.
    Differs,
    /// The current address or the policies could not be read.
    Unknown,
    /// Cloudflare is not configured, so only the house's address is known.
    NotConfigured,
    /// Cloudflare holds no bypass policy with a single address.
    NoPolicy,
}

/// The verdict from both readings. `policies` is None when Cloudflare was
/// not read (not configured, or the read failed).
pub fn judge(current: Option<IpAddr>, policies: Option<&[BypassPolicy]>) -> State {
    let (Some(cur), Some(policies)) = (current, policies) else {
        return State::Unknown;
    };
    let held = held_addresses(policies);
    if held.is_empty() {
        State::NoPolicy
    } else if held.iter().all(|a| *a == cur) {
        State::Match
    } else {
        State::Differs
    }
}

/// One policy to write.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub id: String,
    pub name: String,
    pub place: Place,
    pub from: IpAddr,
    /// The body for the PUT.
    pub body: Value,
}

/// The fields a write may carry. Read-only ones (id, timestamps, counts,
/// `reusable`) are left out so Cloudflare does not refuse the write.
const WRITABLE: &[&str] = &[
    "name",
    "decision",
    "include",
    "exclude",
    "require",
    "precedence",
    "approval_required",
    "approval_groups",
    "isolation_required",
    "purpose_justification_required",
    "purpose_justification_prompt",
    "session_duration",
    "connection_rules",
    "mfa_config",
];

/// The policy body with every include holding `old` replaced by `new` as a
/// single-address range, ready to write. None when nothing in it holds
/// `old` (so nothing is written). Only `ip` includes are looked at; every
/// other rule, and every `ip` include for another address or a wider range,
/// is copied unchanged.
pub fn rewritten(body: &Value, old: IpAddr, new: IpAddr) -> Option<Value> {
    let include = body.get("include")?.as_array()?;
    let mut hit = false;
    let swapped: Vec<Value> = include
        .iter()
        .map(|rule| {
            let holds = rule
                .pointer("/ip/ip")
                .and_then(Value::as_str)
                .and_then(host_of)
                == Some(old);
            if holds {
                hit = true;
                json!({ "ip": { "ip": range_of(new) } })
            } else {
                rule.clone()
            }
        })
        .collect();
    if !hit {
        return None;
    }
    let mut out = serde_json::Map::new();
    for key in WRITABLE {
        if let Some(v) = body.get(*key) {
            out.insert((*key).to_string(), v.clone());
        }
    }
    out.insert("include".into(), Value::Array(swapped));
    Some(Value::Object(out))
}

/// What pressing "Update the home address in Cloudflare" would write: one
/// [`Change`] per bypass policy holding the old address. Ok(empty) when no
/// policy holds another address (nothing to do). Refused when the policies
/// hold more than one address other than the current one, because which of
/// them is the house's cannot be known.
pub fn plan(policies: &[BypassPolicy], current: IpAddr) -> Result<Vec<Change>, String> {
    let old: Vec<IpAddr> = held_addresses(policies)
        .into_iter()
        .filter(|a| *a != current)
        .collect();
    let [old] = old.as_slice() else {
        if old.is_empty() {
            return Ok(Vec::new());
        }
        let list: Vec<String> = old.iter().map(|a| a.to_string()).collect();
        return Err(format!(
            "The bypass policies hold {} different addresses besides the current one ({}), so the dashboard cannot tell which is the house. Remove the ones that are not the house in Cloudflare, then try again.",
            old.len(),
            list.join(", ")
        ));
    };
    Ok(policies
        .iter()
        .filter(|p| p.addresses.contains(old))
        .filter_map(|p| {
            rewritten(&p.body, *old, current).map(|body| Change {
                id: p.id.clone(),
                name: p.name.clone(),
                place: p.place.clone(),
                from: *old,
                body,
            })
        })
        .collect())
}

/// One row of the page's list of policies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyRow {
    pub name: String,
    pub place: String,
    pub address: String,
    /// Whether this policy holds the house's current address.
    pub current: bool,
}

/// What `GET /data/home-address` answers and the Home address section and
/// the Inbox row draw.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub state: State,
    /// The house's address now, when it could be read.
    pub current: Option<String>,
    /// The distinct addresses the bypass policies hold.
    pub held: Vec<String>,
    pub policies: Vec<PolicyRow>,
    /// One plain sentence saying what is the case.
    pub message: String,
    /// What to set up, when Cloudflare is not configured.
    pub configure: Option<String>,
    /// Whether the update button may be pressed, and why not when it may not.
    pub can_update: bool,
    pub why_not: Option<String>,
    /// The service the address was asked of.
    pub lookup_url: String,
    /// Unix seconds of the reading.
    pub read_at: i64,
    /// A warning about the Cloudflare API token (expired, disabled, or
    /// expiring within 14 days), with how to renew it.
    #[serde(default)]
    pub token_note: Option<String>,
    /// The complete fix, step by step, when the failure is the token's.
    #[serde(default)]
    pub fix_steps: Vec<String>,
}

/// The status from both readings. `current` and `policies` carry the reason
/// when a side could not be read.
pub fn status(
    current: Result<IpAddr, String>,
    policies: Result<Vec<BypassPolicy>, String>,
    configure: Option<String>,
    lookup_url: &str,
    read_at: i64,
) -> Status {
    let cur = current.as_ref().ok().copied();
    let pol = policies.as_ref().ok().map(Vec::as_slice);
    let state = if configure.is_some() {
        State::NotConfigured
    } else {
        judge(cur, pol)
    };
    let held = pol.map(held_addresses).unwrap_or_default();
    let rows: Vec<PolicyRow> = pol
        .unwrap_or_default()
        .iter()
        .flat_map(|p| {
            p.addresses.iter().map(move |a| PolicyRow {
                name: p.name.clone(),
                place: p.where_words(),
                address: a.to_string(),
                current: Some(*a) == cur,
            })
        })
        .collect();
    let mut why_not = None;
    let message = match (state, &current, &policies) {
        (State::NotConfigured, Err(e), _) => format!("{e}. Cloudflare is not set up either."),
        (State::NotConfigured, Ok(a), _) => format!(
            "The house's address is {a}. Cloudflare is not set up, so the policies were not read."
        ),
        (_, Err(e), _) => format!("The house's address could not be read: {e}"),
        (_, _, Err(e)) => format!("The Cloudflare policies could not be read: {e}"),
        (State::Match, Ok(a), _) => {
            format!("The house's address {a} is the one Cloudflare lets in without a login.")
        }
        (State::NoPolicy, Ok(a), _) => format!(
            "The house's address is {a}, but Cloudflare holds no bypass policy with a single address to compare with."
        ),
        (State::Differs, Ok(a), _) => format!(
            "The house's address is now {a}, but Cloudflare lets in {} without a login. The house gets login screens and eten refuses it until the address is updated.",
            held.iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(" and ")
        ),
        (State::Unknown, _, _) => "Not read yet.".into(),
    };
    if state != State::Differs {
        why_not = Some(match state {
            State::Match => "Cloudflare already holds the house's address".to_string(),
            State::NotConfigured => configure.clone().unwrap_or_default(),
            State::NoPolicy => "no bypass policy holds a single address to replace".into(),
            _ => "both the house's address and the policies must be read first".into(),
        });
    }
    Status {
        state,
        current: cur.map(|a| a.to_string()),
        held: held.iter().map(|a| a.to_string()).collect(),
        policies: rows,
        message,
        configure,
        can_update: state == State::Differs,
        why_not,
        lookup_url: lookup_url.to_string(),
        read_at,
        token_note: None,
        fix_steps: Vec::new(),
    }
}

/// The complete fix for a token that is missing, refused or without the
/// right permission, step by step (Kenny, 2026-10-23: "zet er ook duidelijk
/// in welke rechten dat zijn, ik wil een complete oplossing zien").
pub fn token_steps(account_id: Option<&str>) -> Vec<String> {
    let account = account_id
        .filter(|a| !a.is_empty())
        .map(|a| format!("your account (id {a})"))
        .unwrap_or_else(|| "your account".to_string());
    vec![
        "In the Cloudflare dashboard open My Profile, API Tokens, and press Create Token, then Create Custom Token.".into(),
        "Name it so you recognise it, e.g. \"homelab admin, home address\".".into(),
        "Permissions: one row, Account · Access: Apps and Policies · Edit. Nothing else is needed.".into(),
        format!("Account Resources: Include · {account}."),
        "TTL: an end date is fine; the dashboard warns in the Inbox 14 days before it.".into(),
        "Press Continue to summary, then Create Token, and copy the token (Cloudflare shows it only once).".into(),
        "Here, press Change the Cloudflare token: set HOMELAB_ADMIN_CLOUDFLARE_ACCESS_TOKEN in admin.env (stack admin) to the copied token, tick the restart and save.".into(),
        "Back in this section, press Check again: both addresses and the policies read again.".into(),
    ]
}

/// Whether a failure is the token's (refused, expired, missing permission
/// or not set up), so the section shows `token_steps` under it.
pub fn is_token_failure(message: &str) -> bool {
    message.contains("API token was not accepted")
        || message.contains("token is valid but may not do this")
        || message.contains("HOMELAB_ADMIN_CLOUDFLARE_ACCESS_TOKEN")
}

/// What to do when Cloudflare refuses a call, from its HTTP status and its
/// own error codes (measured 2026-10-23: a bad or expired token answers
/// HTTP 400 with code 9106 "Authentication failed", a revoked one 401 with
/// 1000 "Invalid API Token"; only the happy path had a remedy before).
pub fn cloudflare_failure(what: &str, status: u16, codes: &[i64], said: &str) -> String {
    let token_fix = "Make a new token with the permission Account · Access: Apps and Policies · \
         Edit and set it as HOMELAB_ADMIN_CLOUDFLARE_ACCESS_TOKEN in admin.env; the steps are listed below.";
    let auth = codes
        .iter()
        .any(|c| matches!(c, 1000 | 9106 | 9109 | 10000 | 10001));
    let cause = if auth || status == 401 {
        format!(
            "the API token was not accepted; it has expired, was revoked or is mistyped. {token_fix}"
        )
    } else if status == 403 {
        format!("the API token is valid but may not do this. {token_fix}")
    } else if status == 404 {
        "Cloudflare does not know this account. Check HOMELAB_ADMIN_CLOUDFLARE_ACCOUNT_ID in admin.env against the account id in the Cloudflare dashboard.".to_string()
    } else if status == 429 {
        "Cloudflare is limiting requests for a moment. Press Check again in a minute.".to_string()
    } else if status >= 500 {
        "Cloudflare itself had a fault. Press Check again in a few minutes; nothing was changed."
            .to_string()
    } else {
        "nothing was changed.".to_string()
    };
    let said = if said.is_empty() {
        String::new()
    } else {
        format!(": {said}")
    };
    format!("Cloudflare refused to {what} (HTTP {status}{said}). Cause: {cause}")
}

/// What to do when Cloudflare did not answer at all.
pub fn cloudflare_unreachable(what: &str, err: &str) -> String {
    format!(
        "Cloudflare did not answer when asked to {what} ({err}). The dashboard (CT 120) could not reach \
         api.cloudflare.com: check its internet connection, DNS and the CT 120 firewall, then press Check again."
    )
}

/// A warning about the API token itself, from Cloudflare's token check:
/// `status` ("active", "expired", "disabled") and `expires_on` (RFC 3339 or
/// empty). None while it is active and not close to expiring.
pub fn token_note(status: &str, expires_on: &str, now_unix: i64) -> Option<String> {
    let renew = "Create a new one in Cloudflare (My Profile, API Tokens) with Access: Apps and \
         Policies, Edit and put it in admin.env as HOMELAB_ADMIN_CLOUDFLARE_ACCESS_TOKEN.";
    if status != "active" {
        return Some(format!("The Cloudflare API token is {status}. {renew}"));
    }
    let day = expires_on.get(..10)?;
    let days_left = days_until(day, now_unix)?;
    if days_left < 0 {
        Some(format!(
            "The Cloudflare API token expired on {day}. {renew}"
        ))
    } else if days_left <= 14 {
        Some(format!(
            "The Cloudflare API token expires on {day}, in {days_left} day(s). {renew}"
        ))
    } else {
        None
    }
}

/// Whole days from `now_unix` to the start of `day` (YYYY-MM-DD, UTC).
fn days_until(day: &str, now_unix: i64) -> Option<i64> {
    let mut it = day.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days - now_unix.div_euclid(86_400))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn policy(id: &str, decision: &str, includes: Value) -> Value {
        json!({
            "id": id, "name": format!("policy {id}"), "decision": decision,
            "include": includes, "exclude": [], "require": [],
            "created_at": "2026-10-22T00:00:00Z", "updated_at": "2026-10-22T00:00:00Z",
            "reusable": true, "app_count": 2, "session_duration": "24h",
        })
    }

    fn ipr(r: &str) -> Value {
        json!({ "ip": { "ip": r } })
    }

    #[test]
    fn matching_and_mismatching_addresses_are_told_apart() {
        let reusable = vec![policy("a", "bypass", json!([ipr("203.0.113.7/32")]))];
        let held = bypass_policies(&reusable, &[]);
        assert_eq!(judge(Some(ip("203.0.113.7")), Some(&held)), State::Match);
        assert_eq!(judge(Some(ip("198.51.100.9")), Some(&held)), State::Differs);
        assert_eq!(judge(None, Some(&held)), State::Unknown);
        assert_eq!(judge(Some(ip("203.0.113.7")), None), State::Unknown);
        assert_eq!(judge(Some(ip("203.0.113.7")), Some(&[])), State::NoPolicy);
        // One policy current and one stale still differs.
        let two = vec![
            policy("a", "bypass", json!([ipr("203.0.113.7/32")])),
            policy("b", "bypass", json!([ipr("198.51.100.9/32")])),
        ];
        let held = bypass_policies(&two, &[]);
        assert_eq!(judge(Some(ip("203.0.113.7")), Some(&held)), State::Differs);
    }

    #[test]
    fn only_bypass_ip_includes_holding_the_old_address_are_rewritten() {
        let old = "203.0.113.7";
        let reusable = vec![
            // The house's policy: rewritten.
            policy(
                "home",
                "bypass",
                json!([ipr("203.0.113.7/32"), { "email": { "email": "a@b.c" } }]),
            ),
            // The same address in an allow policy: never touched.
            policy("allow", "allow", json!([ipr("203.0.113.7/32")])),
            // A wider range holding the old address' network: never touched.
            policy("wide", "bypass", json!([ipr("203.0.113.0/24")])),
            // A bypass for someone else's address: not the old one.
            policy("other", "bypass", json!([ipr("198.51.100.1/32")])),
        ];
        let held = bypass_policies(&reusable, &[]);
        // Two different single addresses besides the new one: refused, not guessed.
        let err = plan(&held, ip("192.0.2.55")).unwrap_err();
        assert!(err.contains("198.51.100.1") && err.contains(old), "{err}");
        // With the other address already current, only the stale one moves.
        let changes = plan(&held, ip("198.51.100.1")).unwrap();
        assert_eq!(changes.len(), 1);
        let c = &changes[0];
        assert_eq!(c.id, "home");
        assert_eq!(c.from, ip(old));
        assert_eq!(
            c.body["include"],
            json!([ipr("198.51.100.1/32"), { "email": { "email": "a@b.c" } }])
        );
        // Read-only fields are not sent back.
        for key in ["id", "created_at", "updated_at", "reusable", "app_count"] {
            assert!(c.body.get(key).is_none(), "{key} must not be written");
        }
        assert_eq!(c.body["decision"], "bypass");
        assert_eq!(c.body["session_duration"], "24h");
    }

    #[test]
    fn a_wider_range_or_an_allow_policy_is_never_rewritten() {
        let allow = policy("x", "allow", json!([ipr("203.0.113.7/32")]));
        assert!(bypass_policies(&[allow], &[]).is_empty());
        let wide = policy("y", "bypass", json!([ipr("203.0.113.0/24")]));
        assert!(bypass_policies(std::slice::from_ref(&wide), &[]).is_empty());
        assert!(rewritten(&wide, ip("203.0.113.7"), ip("192.0.2.1")).is_none());
        // Everyone is never produced from a rewrite.
        let rule = policy("z", "bypass", json!([ipr("203.0.113.7/32")]));
        let out = rewritten(&rule, ip("203.0.113.7"), ip("192.0.2.1")).unwrap();
        assert_eq!(out["include"], json!([ipr("192.0.2.1/32")]));
        assert!(!out.to_string().contains("everyone"));
    }

    #[test]
    fn an_application_policy_and_the_reusable_one_it_references_count_once() {
        let reusable = vec![policy("home", "bypass", json!([ipr("203.0.113.7/32")]))];
        let apps = vec![
            // References the reusable policy by id, also as a full object.
            json!({ "id": "app1", "name": "Homelab", "policies": [
                { "id": "home", "decision": "bypass", "include": [ipr("203.0.113.7/32")], "reusable": true },
                { "id": "g", "decision": "allow", "include": [{"email_domain": {"domain": "x.y"}}] },
            ]}),
            // A policy written inside the application.
            json!({ "id": "app2", "name": "eten", "policies": [
                { "id": "own", "name": "eten home", "decision": "bypass", "include": [ipr("203.0.113.7/32")] },
                "just-an-id",
            ]}),
        ];
        let held = bypass_policies(&reusable, &apps);
        assert_eq!(held.len(), 2);
        assert_eq!(held[0].place, Place::Reusable);
        assert_eq!(
            held[1].place,
            Place::App {
                app_id: "app2".into(),
                app_name: "eten".into()
            }
        );
        let changes = plan(&held, ip("192.0.2.1")).unwrap();
        assert_eq!(changes.len(), 2);
    }

    #[test]
    fn nothing_to_do_when_the_policies_already_hold_the_address() {
        let reusable = vec![policy("a", "bypass", json!([ipr("203.0.113.7/32")]))];
        let held = bypass_policies(&reusable, &[]);
        assert!(plan(&held, ip("203.0.113.7")).unwrap().is_empty());
    }

    #[test]
    fn a_missing_token_says_what_to_configure() {
        let both = config_problem(None, None).unwrap();
        assert!(both.contains(ENV_ACCOUNT) && both.contains(ENV_TOKEN));
        let token = config_problem(Some("abc"), Some("  ")).unwrap();
        assert!(token.contains(ENV_TOKEN) && !token.contains(ENV_ACCOUNT));
        assert!(token.contains("admin.env"));
        assert!(config_problem(Some("abc"), Some("tok")).is_none());
    }

    #[test]
    fn only_a_public_address_is_accepted_as_the_houses() {
        assert_eq!(
            parse_public_address(" 203.0.113.7\n").unwrap(),
            ip("203.0.113.7")
        );
        assert!(parse_public_address("<html>").is_err());
        for bad in [
            "10.0.0.5",
            "192.168.1.9",
            "127.0.0.1",
            "0.0.0.0",
            "169.254.1.1",
            "::1",
        ] {
            assert!(parse_public_address(bad).is_err(), "{bad}");
        }
        assert_eq!(range_of(ip("203.0.113.7")), "203.0.113.7/32");
        assert_eq!(range_of(ip("2001:db8::1")), "2001:db8::1/128");
        assert_eq!(host_of("203.0.113.7"), Some(ip("203.0.113.7")));
        assert_eq!(host_of("203.0.113.7/24"), None);
    }

    #[test]
    fn the_status_says_what_is_the_case_and_whether_the_button_works() {
        let reusable = vec![policy("a", "bypass", json!([ipr("203.0.113.7/32")]))];
        let held = bypass_policies(&reusable, &[]);
        let same = status(Ok(ip("203.0.113.7")), Ok(held.clone()), None, "u", 5);
        assert_eq!(same.state, State::Match);
        assert!(!same.can_update);
        let diff = status(Ok(ip("192.0.2.1")), Ok(held), None, "u", 5);
        assert_eq!(diff.state, State::Differs);
        assert!(diff.can_update && diff.why_not.is_none());
        assert!(diff.message.contains("192.0.2.1") && diff.message.contains("203.0.113.7"));
        let none = status(
            Ok(ip("192.0.2.1")),
            Err("no token".into()),
            config_problem(None, None),
            "u",
            5,
        );
        assert_eq!(none.state, State::NotConfigured);
        assert!(!none.can_update);
        assert!(none.why_not.unwrap().contains(ENV_TOKEN));
    }

    /// eten-home-ip-1: measured 2026-10-23, a bad token answers HTTP 400 with
    /// code 9106; the message names the cause and the fix, not only a status.
    #[test]
    fn a_refused_token_names_the_cause_and_the_fix() {
        let m = cloudflare_failure(
            "list the Access policies",
            400,
            &[9106],
            "Authentication failed",
        );
        assert!(m.contains("not accepted"), "{m}");
        assert!(m.contains("HOMELAB_ADMIN_CLOUDFLARE_ACCESS_TOKEN"), "{m}");
        let m = cloudflare_failure("list the Access policies", 404, &[], "");
        assert!(m.contains("HOMELAB_ADMIN_CLOUDFLARE_ACCOUNT_ID"), "{m}");
        assert!(cloudflare_failure("x", 503, &[], "").contains("Check again"));
    }

    #[test]
    fn the_token_note_warns_before_and_after_expiry() {
        // 2026-12-20 00:00 UTC.
        let dec20 = 1_797_724_800;
        assert_eq!(
            token_note("active", "2026-12-31T23:59:59Z", dec20 - 30 * 86_400),
            None
        );
        assert!(
            token_note("active", "2026-12-31T23:59:59Z", dec20)
                .unwrap()
                .contains("in 11 day(s)")
        );
        assert!(
            token_note("active", "2026-12-31T23:59:59Z", dec20 + 20 * 86_400)
                .unwrap()
                .contains("expired on 2026-12-31")
        );
        assert!(
            token_note("expired", "", dec20)
                .unwrap()
                .contains("is expired")
        );
    }
}
