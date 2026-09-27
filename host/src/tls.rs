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
        // Key is private material — create it 0600 from the first byte
        // (write-then-chmod leaves a world-readable window; hardening H21).
        {
            use std::io::Write as _;
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&key_path)?;
            f.write_all(key.serialize_pem().as_bytes())?;
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
    // rust-code-hygiene (2026-09-27): the `base64` crate the host already
    // depends on, not a hand-written decoder kept "to avoid a crate".
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(body.trim())
        .map_err(|e| std::io::Error::other(format!("invalid cert PEM: {e}")))
}

#[cfg(test)]
mod tests {
    /// The PEM a host writes decodes to exactly the DER its fingerprint is
    /// taken over (rust-code-hygiene, 2026-09-27: guards the move from a
    /// hand-written decoder to the `base64` crate).
    #[test]
    fn a_generated_certificate_decodes_to_its_own_der() {
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(vec!["localhost".to_string()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        assert_eq!(super::pem_to_der(&cert.pem()).unwrap(), cert.der().to_vec());
        assert!(super::pem_to_der("-----BEGIN X-----\n#!\n-----END X-----\n").is_err());
    }
}
