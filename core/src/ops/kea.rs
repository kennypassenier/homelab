//! OPNsense Kea DHCP reservations (H2). Static IPs in the manifests remain
//! the source of truth — this registers the same IP+MAC as a Kea reservation
//! so the DHCP server never hands the address to anyone else. Ported from
//! the old project's proven flow: search_subnet → find uuid → search
//! existing reservation → add/set → reconfigure.
//!
//! Fail-open by design: a Kea failure warns loudly but never blocks a
//! deploy (the container has a static IP either way).

use crate::error::CoreError;
use crate::executor::{Cmd, Executor};

#[derive(Debug, Clone)]
pub struct KeaCfg {
    /// e.g. "https://10.10.10.1" (the OPNsense web UI base).
    pub base_url: String,
    /// Host-side file containing `key:secret` for basic auth. Read inside
    /// the curl shell so the secret never appears in argv.
    pub cred_file: String,
}

/// `10.10.10.8` in `10.10.10.0/24`?
fn ip_in_cidr(ip: &str, cidr: &str) -> bool {
    fn v4(s: &str) -> Option<u32> {
        let mut out = 0u32;
        let mut parts = 0;
        for p in s.split('.') {
            out = (out << 8) | p.parse::<u32>().ok().filter(|v| *v < 256)?;
            parts += 1;
        }
        (parts == 4).then_some(out)
    }
    let (net, bits) = match cidr.split_once('/') {
        Some((n, b)) => (n, b),
        None => return false,
    };
    let (Some(ip), Some(net), Ok(bits)) = (v4(ip), v4(net), bits.parse::<u32>()) else {
        return false;
    };
    if bits == 0 || bits > 32 {
        return false;
    }
    let mask = u32::MAX << (32 - bits);
    (ip & mask) == (net & mask)
}

async fn api(
    exec: &dyn Executor,
    cfg: &KeaCfg,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<serde_json::Value, CoreError> {
    let data = body
        .map(|b| format!(" -H 'Content-Type: application/json' -d '{}'", b))
        .unwrap_or_default();
    let script = format!(
        "curl -sk -m 20 -u \"$(cat {cred})\" -X {method}{data} {url}{path}",
        cred = cfg.cred_file,
        url = cfg.base_url.trim_end_matches('/'),
    );
    let out = exec.run(&Cmd::new("sh", &["-c", &script], 30)).await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "opnsense {} {}: {}",
            method,
            path,
            out.stderr.trim()
        )));
    }
    serde_json::from_str(out.stdout.trim())
        .map_err(|e| CoreError::Other(format!("opnsense {}: bad json: {}", path, e)))
}

/// Register (or update) the reservation for `ip` (no CIDR suffix) + `mac`.
pub async fn reserve(
    exec: &dyn Executor,
    cfg: &KeaCfg,
    ip: &str,
    mac: &str,
    hostname: &str,
) -> Result<(), CoreError> {
    // 1. Which Kea subnet holds this IP?
    let subnets = api(exec, cfg, "GET", "/api/kea/dhcpv4/search_subnet", None).await?;
    let subnet_uuid = subnets["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| {
            r["subnet"]
                .as_str()
                .map(|c| ip_in_cidr(ip, c))
                .unwrap_or(false)
        })
        .and_then(|r| r["uuid"].as_str())
        .map(String::from)
        .ok_or_else(|| CoreError::Other(format!("no Kea subnet matches {}", ip)))?;

    // 2. Existing reservation for this IP?
    let existing = api(exec, cfg, "GET", "/api/kea/dhcpv4/search_reservation", None).await?;
    let existing_uuid = existing["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["ip_address"].as_str() == Some(ip))
        .and_then(|r| r["uuid"].as_str())
        .map(String::from);

    // 3. Add or update.
    let body = serde_json::json!({
        "reservation": {
            "subnet": subnet_uuid,
            "ip_address": ip,
            "hw_address": mac,
            "hostname": hostname,
        }
    })
    .to_string();
    match existing_uuid {
        Some(uuid) => {
            api(
                exec,
                cfg,
                "POST",
                &format!("/api/kea/dhcpv4/set_reservation/{}", uuid),
                Some(&body),
            )
            .await?;
        }
        None => {
            api(
                exec,
                cfg,
                "POST",
                "/api/kea/dhcpv4/add_reservation",
                Some(&body),
            )
            .await?;
        }
    }

    // 4. Apply.
    api(
        exec,
        cfg,
        "POST",
        "/api/kea/service/reconfigure",
        Some("{}"),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ip_in_cidr;

    #[test]
    fn cidr_matching() {
        assert!(ip_in_cidr("10.10.10.8", "10.10.10.0/24"));
        assert!(!ip_in_cidr("10.10.11.8", "10.10.10.0/24"));
        assert!(ip_in_cidr("10.10.11.8", "10.10.0.0/16"));
        assert!(!ip_in_cidr("garbage", "10.10.10.0/24"));
        assert!(!ip_in_cidr("10.10.10.8", "not-a-cidr"));
    }
}
