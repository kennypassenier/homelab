//! arch-exposure (Kenny, 2026-09-28): the dashboard is reached through the
//! Cloudflare tunnel and Traefik, and answers only when two locks hold before
//! its own login:
//!
//! 1. the request carries a valid Cloudflare Access token
//!    (`Cf-Access-Jwt-Assertion`): signed by the team's key, for this
//!    application (`aud`), from this team (`iss`), not expired. This closes
//!    Traefik's forged-Host gap from the LAN: a request that did not pass
//!    Access has no token Cloudflare signed;
//! 2. the client's address (`Cf-Connecting-IP`, believed only from the
//!    trusted proxy) is the house's public address, which the host reads
//!    from the router every night.
//!
//! Everything here is pure: the claims, the address and the key set are
//! decided without I/O. Verifying the RSA signature is the shell's work.

use serde::Deserialize;

/// Why a request is refused, in words for the refusal page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    NoToken,
    Malformed(String),
    UnknownKey,
    BadSignature,
    WrongAudience,
    WrongIssuer,
    Expired,
    NotYetValid,
    NotFromHome { from: String },
    HomeUnknown,
}

impl Refusal {
    /// The alarm's three lines (Kenny, 2026-09-28: the refusal is the
    /// kp-themes Alarm, not a line of text): the small code line, the huge
    /// word or two, and what happened with what to do.
    pub fn alarm(&self) -> (&'static str, &'static str, String) {
        match self {
            Refusal::NotFromHome { from } => (
                "homelab admin · lock 2 · home only",
                "Not from home",
                format!("This dashboard only answers from the house's own connection. This request came from {from}."),
            ),
            Refusal::HomeUnknown => (
                "homelab admin · lock 2 · home only",
                "Home unknown",
                "The host has not read the house's public address yet. It asks at its start, after a gateway deploy and every night.".into(),
            ),
            Refusal::NoToken => (
                "homelab admin · lock 1 · Cloudflare Access",
                "No pass",
                "Open the dashboard through https://admin.kp-soft.dev, which signs you in with Cloudflare Access first.".into(),
            ),
            Refusal::Expired => (
                "homelab admin · lock 1 · Cloudflare Access",
                "Pass expired",
                "The Cloudflare Access sign-in has expired. Reload to sign in again.".into(),
            ),
            other => (
                "homelab admin · lock 1 · Cloudflare Access",
                "Access refused",
                other.text(),
            ),
        }
    }

    pub fn text(&self) -> String {
        match self {
            Refusal::NoToken => "refused: no Cloudflare Access token — open the dashboard through https://admin.kp-soft.dev".into(),
            Refusal::Malformed(why) => format!("refused: the Cloudflare Access token does not read ({why})"),
            Refusal::UnknownKey => "refused: the Access token is signed by a key this team does not publish".into(),
            Refusal::BadSignature => "refused: the Access token's signature does not verify".into(),
            Refusal::WrongAudience => "refused: the Access token is for another application".into(),
            Refusal::WrongIssuer => "refused: the Access token comes from another team".into(),
            Refusal::Expired => "refused: the Access token has expired — reload to sign in again".into(),
            Refusal::NotYetValid => "refused: the Access token is not valid yet (clock skew?)".into(),
            Refusal::NotFromHome { from } => format!("refused: this dashboard only answers from home; this request came from {from}"),
            Refusal::HomeUnknown => "refused: the house's public address is not known yet (the host reads it from the router)".into(),
        }
    }
}

/// The three dot-separated parts of a JWT, decoded where they are JSON.
#[derive(Debug, Clone)]
pub struct Jwt {
    pub kid: String,
    pub alg: String,
    pub claims: Claims,
    /// `header.payload`, the bytes the signature covers.
    pub signed: String,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    /// A string or a list of strings.
    pub aud: serde_json::Value,
    pub iss: String,
    pub exp: u64,
    #[serde(default)]
    pub nbf: Option<u64>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: String,
}

fn b64url(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s.trim_end_matches('='))
        .map_err(|e| e.to_string())
}

pub fn parse_jwt(token: &str) -> Result<Jwt, Refusal> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(Refusal::Malformed("not three parts".into()));
    }
    let header: Header = serde_json::from_slice(&b64url(parts[0]).map_err(Refusal::Malformed)?)
        .map_err(|e| Refusal::Malformed(e.to_string()))?;
    let claims: Claims = serde_json::from_slice(&b64url(parts[1]).map_err(Refusal::Malformed)?)
        .map_err(|e| Refusal::Malformed(e.to_string()))?;
    Ok(Jwt {
        kid: header.kid,
        alg: header.alg,
        claims,
        signed: format!("{}.{}", parts[0], parts[1]),
        signature: b64url(parts[2]).map_err(Refusal::Malformed)?,
    })
}

/// Audience, issuer and time; `leeway_s` absorbs a little clock skew.
pub fn check_claims(
    c: &Claims,
    aud: &str,
    team_domain: &str,
    now: u64,
    leeway_s: u64,
) -> Result<(), Refusal> {
    let aud_ok = match &c.aud {
        serde_json::Value::String(s) => s == aud,
        serde_json::Value::Array(a) => a.iter().any(|v| v.as_str() == Some(aud)),
        _ => false,
    };
    if !aud_ok {
        return Err(Refusal::WrongAudience);
    }
    let want_iss = format!(
        "https://{}",
        team_domain
            .trim_start_matches("https://")
            .trim_end_matches('/')
    );
    if c.iss.trim_end_matches('/') != want_iss {
        return Err(Refusal::WrongIssuer);
    }
    if c.exp + leeway_s < now {
        return Err(Refusal::Expired);
    }
    if c.nbf.is_some_and(|nbf| nbf > now + leeway_s) {
        return Err(Refusal::NotYetValid);
    }
    Ok(())
}

/// One RSA key from the team's key set (`/cdn-cgi/access/certs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsaKey {
    pub kid: String,
    /// Big-endian modulus and exponent.
    pub n: Vec<u8>,
    pub e: Vec<u8>,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<JwkRaw>,
}
#[derive(Deserialize)]
struct JwkRaw {
    kid: String,
    kty: String,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    e: Option<String>,
}

/// The RSA keys of a JWKS document; other key types are skipped.
pub fn parse_jwks(json: &str) -> Result<Vec<RsaKey>, String> {
    let set: Jwks =
        serde_json::from_str(json).map_err(|e| format!("the Access key set does not read: {e}"))?;
    let mut out = Vec::new();
    for k in set.keys {
        if k.kty != "RSA" {
            continue;
        }
        let (Some(n), Some(e)) = (k.n, k.e) else {
            continue;
        };
        out.push(RsaKey {
            kid: k.kid,
            n: b64url(&n)?,
            e: b64url(&e)?,
        });
    }
    Ok(out)
}

/// Lock 2: the client's address, as the trusted proxy reported it, is the
/// house's public address.
pub fn from_home(client: Option<&str>, home: Option<&str>) -> Result<(), Refusal> {
    let home = home.filter(|h| !h.is_empty()).ok_or(Refusal::HomeUnknown)?;
    let client = client.map(str::trim).unwrap_or("");
    if client == home {
        Ok(())
    } else {
        Err(Refusal::NotFromHome {
            from: if client.is_empty() {
                "an unknown address".into()
            } else {
                client.into()
            },
        })
    }
}

/// The RS256 signature of `signed` under `key` (PKCS#1 v1.5, SHA-256,
/// 2048-8192 bit keys). Pure: aws-lc-rs computes, nothing is fetched.
pub fn verify_rs256(key: &RsaKey, signed: &[u8], signature: &[u8]) -> Result<(), Refusal> {
    aws_lc_rs::signature::RsaPublicKeyComponents {
        n: &key.n,
        e: &key.e,
    }
    .verify(
        &aws_lc_rs::signature::RSA_PKCS1_2048_8192_SHA256,
        signed,
        signature,
    )
    .map_err(|_| Refusal::BadSignature)
}

/// Text safe inside an HTML attribute or element.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// The refusal page: kp-themes' stylesheet and the Alarm, raised by a
/// module the guard itself serves at [`REFUSAL_SCRIPT`] (everything under
/// `/app` sits behind the login a refused visitor never reaches), with the
/// plain words in the page as well for a browser without scripts.
/// Where the refusal page's module lives: answered by the guard, before any
/// lock, because the page that loads it is shown to a refused visitor.
pub const REFUSAL_SCRIPT: &str = "/refused.js";

pub fn refusal_page(r: &Refusal) -> String {
    let (code, title, detail) = r.alarm();
    let (code, title, detail) = (html_escape(code), html_escape(title), html_escape(&detail));
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title} · homelab admin</title>\n\
         <link rel=\"stylesheet\" href=\"/static/kp/dist/kp-themes.css\">\n\
         <script type=\"module\" src=\"{REFUSAL_SCRIPT}\"></script>\n</head>\n\
         <body>\n<main id=\"refusal\" data-code=\"{code}\" data-title=\"{title}\" data-detail=\"{detail}\" \
         style=\"max-width:40rem;margin:4rem auto;padding-inline:16px\">\n\
         <p>{code}</p>\n<h1>{title}</h1>\n<p>{detail}</p>\n</main>\n</body>\n</html>\n"
    )
}
