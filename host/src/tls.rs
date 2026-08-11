//! TLS (A4): a self-signed certificate generated once at install and reused
//! thereafter. The client pins its fingerprint on first connect (TOFU). We
//! print the fingerprint on every boot so it can be verified out of band.

use std::path::Path;

use sha2::{Digest, Sha256};

pub struct CertPaths {
    pub cert_pem: String,
    pub key_pem: String,
}

/// Ensure a cert/key pair exists under `dir`; generate on first run.
/// Returns the paths and the SHA-256 fingerprint of the DER cert.
pub fn ensure_cert(dir: &str, hostname: &str) -> std::io::Result<(CertPaths, String)> {
    std::fs::create_dir_all(dir)?;
    let cert_path = format!("{}/tls-cert.pem", dir);
    let key_path = format!("{}/tls-key.pem", dir);

    if !Path::new(&cert_path).exists() || !Path::new(&key_path).exists() {
        let mut params =
            rcgen::CertificateParams::new(vec![hostname.to_string(), "localhost".to_string()])
                .map_err(std::io::Error::other)?;
        params.distinguished_name = rcgen::DistinguishedName::new();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "homelab-host");
        let key = rcgen::KeyPair::generate().map_err(std::io::Error::other)?;
        let cert = params.self_signed(&key).map_err(std::io::Error::other)?;
        std::fs::write(&cert_path, cert.pem())?;
        std::fs::write(&key_path, key.serialize_pem())?;
        // Key is private material — lock it down.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
        }
    }

    let fingerprint = fingerprint_of(&cert_path)?;
    Ok((
        CertPaths {
            cert_pem: cert_path,
            key_pem: key_path,
        },
        fingerprint,
    ))
}

/// SHA-256 fingerprint (hex, colon-separated) of the DER form of a PEM cert.
pub fn fingerprint_of(cert_pem_path: &str) -> std::io::Result<String> {
    let pem = std::fs::read_to_string(cert_pem_path)?;
    let der = pem_to_der(&pem)?;
    let mut hasher = Sha256::new();
    hasher.update(&der);
    let digest = hasher.finalize();
    Ok(digest
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(":"))
}

fn pem_to_der(pem: &str) -> std::io::Result<Vec<u8>> {
    let body: String = pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect::<String>();
    base64_decode(&body).ok_or_else(|| std::io::Error::other("invalid cert PEM"))
}

/// Minimal standard-base64 decoder (avoids pulling a crate for one use).
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let v = val(c)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}
