//! The bearer-token list in `host.toml`: validation, issuing and revoking.

use super::*;

/// arch-tokens: one `[[tokens]]` entry in host.toml.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct TokenEntry {
    /// Who holds it, e.g. "admin" or "wsl"; named in every audit line.
    pub(crate) name: String,
    pub(crate) scope: homelab_proto::Scope,
    /// Lowercase hex SHA-256 of the token itself.
    pub(crate) sha256: String,
}

/// arch-tokens: a token list the host can trust, or the reason it cannot.
pub(crate) fn validate_tokens(tokens: &[TokenEntry]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for t in tokens {
        if t.name.trim().is_empty() {
            return Err("a [[tokens]] entry has an empty name".into());
        }
        if t.name == "legacy" {
            return Err(
                "\"legacy\" is the name of the single `token` key; pick another name".into(),
            );
        }
        if !seen.insert(t.name.as_str()) {
            return Err(format!("two [[tokens]] entries are named {:?}", t.name));
        }
        let hex = t.sha256.len() == 64
            && t.sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !hex {
            return Err(format!(
                "[[tokens]] {:?}: sha256 must be 64 lowercase hex characters (sha256sum of the token)",
                t.name
            ));
        }
    }
    Ok(())
}

/// fix-120: a fresh per-machine bearer, 32 random bytes as lowercase hex.
/// `/dev/urandom` rather than a new crate dependency: this is the one place
/// the host needs cryptographic randomness.
pub(crate) fn random_token() -> Result<String, String> {
    use std::io::Read as _;
    let mut f = std::fs::File::open("/dev/urandom").map_err(|e| format!("/dev/urandom: {}", e))?;
    let mut buf = [0u8; 32];
    f.read_exact(&mut buf)
        .map_err(|e| format!("/dev/urandom: {}", e))?;
    Ok(buf.iter().map(|b| format!("{:02x}", b)).collect())
}

/// fix-120: the `[[tokens]]` list out of raw host.toml text, read-modify-
/// write helpers share this so neither duplicates the parse.
pub(crate) fn read_tokens(raw: &str) -> Result<(toml::Table, Vec<TokenEntry>), String> {
    let table: toml::Table = if raw.trim().is_empty() {
        toml::Table::new()
    } else {
        toml::from_str(raw).map_err(|e| format!("host.toml does not parse as TOML: {}", e))?
    };
    let file: FileConfig = table
        .clone()
        .try_into()
        .map_err(|e| format!("host.toml is not a valid host config: {}", e))?;
    Ok((table, file.tokens.unwrap_or_default()))
}

/// fix-120: `tokens` written back into `table` and the whole file
/// re-rendered, after `validate_tokens` has already passed.
pub(crate) fn write_tokens(
    mut table: toml::Table,
    tokens: &[TokenEntry],
) -> Result<String, String> {
    if tokens.is_empty() {
        table.remove("tokens");
    } else {
        table.insert(
            "tokens".into(),
            toml::Value::try_from(tokens).map_err(|e| e.to_string())?,
        );
    }
    toml::to_string_pretty(&table).map_err(|e| e.to_string())
}

/// fix-120 (per-machine tokens, owner decision 2026-10-01): the current
/// `[[tokens]]` list plus a new entry named `name` at `scope`, whose SHA-256
/// is of `plain_token` — pure, so the random token itself is handed in and
/// tested separately. Refuses an empty or duplicate name, `"legacy"`, and
/// anything `validate_tokens` would refuse at start.
pub(crate) fn issue_token_in(
    raw: &str,
    name: &str,
    scope: homelab_proto::Scope,
    plain_token: &str,
) -> Result<(String, Vec<TokenEntry>), String> {
    let (table, mut tokens) = read_tokens(raw)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("a token needs a name".into());
    }
    use sha2::{Digest, Sha256};
    let sha256: String = Sha256::digest(plain_token.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    tokens.push(TokenEntry {
        name: name.to_string(),
        scope,
        sha256,
    });
    validate_tokens(&tokens)?;
    let text = write_tokens(table, &tokens)?;
    Ok((text, tokens))
}

/// fix-120: the current `[[tokens]]` list with `name` removed, or the
/// reason it is refused: `"legacy"` is the single `token` key, cleared over
/// ssh instead (OPERATIONS_RUNBOOK's migration note), and a name that is
/// not there is refused rather than silently a no-op.
pub(crate) fn revoke_token_in(raw: &str, name: &str) -> Result<(String, Vec<TokenEntry>), String> {
    if name == "legacy" {
        return Err(
            "\"legacy\" is the single `token` key, not a [[tokens]] entry — clear `token` over \
             ssh once every machine has its own named token"
                .into(),
        );
    }
    let (table, mut tokens) = read_tokens(raw)?;
    let before = tokens.len();
    tokens.retain(|t| t.name != name);
    if tokens.len() == before {
        return Err(format!("no token named {:?}", name));
    }
    let text = write_tokens(table, &tokens)?;
    Ok((text, tokens))
}
