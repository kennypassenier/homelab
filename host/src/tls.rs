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

    if !pair_is_whole(&cert_path, &key_path) {
        // fix-128 (expert panel, tls-first-boot-power-cut, 2026-09-27): a
        // pair that is not whole is replaced whole. A power cut during the
        // first boot used to leave a key without its certificate, or an
        // empty certificate, and the daemon crash-looped on it until
        // someone deleted the files by hand. No client can hold a pin for a
        // pair that never served, and a broken pair of a host that did
        // serve fails the pin loudly, which is the safe way round.
        if Path::new(&cert_path).exists() || Path::new(&key_path).exists() {
            eprintln!(
                "WARNING: {} and {} are not a whole pair — making a new certificate; clients \
                 that pinned the old one will refuse it until their pin is updated",
                cert_path, key_path
            );
        }
        let mut params =
            rcgen::CertificateParams::new(vec![hostname.to_string(), "localhost".to_string()])
                .map_err(std::io::Error::other)?;
        params.distinguished_name = rcgen::DistinguishedName::new();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "homelab-host");
        let key = rcgen::KeyPair::generate().map_err(std::io::Error::other)?;
        let cert = params.self_signed(&key).map_err(std::io::Error::other)?;
        // The key first and the certificate last, each written whole: a
        // certificate on disk means its key already is.
        write_atomic(&key_path, key.serialize_pem().as_bytes(), 0o600)?;
        write_atomic(&cert_path, cert.pem().as_bytes(), 0o644)?;
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

/// fix-128: both files present, the certificate decodes, the key is a PEM
/// private key.
fn pair_is_whole(cert_path: &str, key_path: &str) -> bool {
    let cert_ok = std::fs::read_to_string(cert_path)
        .ok()
        .filter(|pem| pem.contains("BEGIN CERTIFICATE"))
        .and_then(|pem| pem_to_der(&pem).ok())
        .is_some_and(|der| !der.is_empty());
    let key_ok = std::fs::read_to_string(key_path)
        .is_ok_and(|pem| pem.contains("-----BEGIN") && pem.contains("PRIVATE KEY-----"));
    cert_ok && key_ok
}

/// fix-128: write `bytes` to `path` so that a power cut leaves the old file
/// or the new one, never a torn one: a temp file created with its final
/// mode (the key is never readable by others, not even for a moment), fsync,
/// rename, fsync of the directory.
fn write_atomic(path: &str, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = format!("{}.tmp", path);
    let _ = std::fs::remove_file(&tmp);
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    if let Some(parent) = Path::new(path).parent() {
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
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

    use super::*;

    fn fresh_dir(tag: &str) -> String {
        let dir = std::env::temp_dir().join(format!("homelab-tls-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().into_owned()
    }

    /// fix-128 (expert panel, tls-first-boot-power-cut, 2026-09-27): a power
    /// cut during the first boot could leave the key without the
    /// certificate. The next start then failed `create_new` on the key and
    /// the daemon crash-looped until someone deleted the file by hand.
    #[test]
    fn fix_128_a_key_without_its_certificate_is_replaced_by_a_new_pair() {
        let dir = fresh_dir("keyonly");
        std::fs::write(format!("{}/tls-key.pem", dir), "half a key").unwrap();
        let got = ensure_cert(&dir, "homelab-host");
        let cert = std::fs::read_to_string(format!("{}/tls-cert.pem", dir)).unwrap_or_default();
        let key = std::fs::read_to_string(format!("{}/tls-key.pem", dir)).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(got.is_ok(), "{:?}", got.err());
        assert!(cert.contains("BEGIN CERTIFICATE"), "{}", cert);
        assert!(key.contains("PRIVATE KEY"), "{}", key);
    }

    /// fix-128: the certificate was written with a plain `std::fs::write`, no
    /// fsync, so a power cut could leave it empty; the daemon then panicked
    /// on `load tls` at every start.
    #[test]
    fn fix_128_an_empty_certificate_is_replaced_by_a_new_pair() {
        let dir = fresh_dir("emptycert");
        ensure_cert(&dir, "homelab-host").expect("first pair");
        std::fs::write(format!("{}/tls-cert.pem", dir), "").unwrap();
        let got = ensure_cert(&dir, "homelab-host");
        let cert = std::fs::read_to_string(format!("{}/tls-cert.pem", dir)).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(got.is_ok(), "{:?}", got.err());
        assert!(cert.contains("BEGIN CERTIFICATE"));
    }

    /// A whole pair is never replaced: the fingerprint is what every client
    /// pins.
    #[test]
    fn fix_128_a_whole_pair_is_kept() {
        let dir = fresh_dir("keep");
        let (_, first) = ensure_cert(&dir, "homelab-host").expect("first pair");
        let (_, again) = ensure_cert(&dir, "homelab-host").expect("second start");
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(format!("{}/tls-key.pem", dir))
                .unwrap()
                .permissions()
                .mode()
                & 0o777
        };
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(first, again);
        assert_eq!(mode, 0o600);
    }
}
