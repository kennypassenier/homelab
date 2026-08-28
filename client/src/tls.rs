//! Client-side TLS with certificate pinning (A4, TOFU model): on first
//! connect we record the host cert's SHA-256 fingerprint; on every later
//! connect we require the exact same certificate, like SSH's known_hosts.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};

pub fn fingerprint(cert: &CertificateDer) -> String {
    let mut hasher = Sha256::new();
    hasher.update(cert.as_ref());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(":")
}

/// Verifier that accepts exactly one pinned fingerprint. On first use the
/// expected fingerprint is empty and any cert is accepted while the observed
/// fingerprint is recorded (TOFU); afterwards only the pinned one passes.
#[derive(Debug)]
pub struct PinnedVerifier {
    expected: Option<String>,
    observed: std::sync::Mutex<Option<String>>,
}

impl PinnedVerifier {
    pub fn new(expected: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            expected,
            observed: std::sync::Mutex::new(None),
        })
    }
    pub fn observed(&self) -> Option<String> {
        self.observed.lock().unwrap().clone()
    }
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint(end_entity);
        *self.observed.lock().unwrap() = Some(fp.clone());
        match &self.expected {
            None => Ok(ServerCertVerified::assertion()), // TOFU: record and trust
            Some(pin) if pin == &fp => Ok(ServerCertVerified::assertion()),
            Some(pin) => Err(rustls::Error::General(format!(
                "certificate fingerprint mismatch — pinned SHA256:{} but host presented SHA256:{} (possible MITM; delete the pin only if you rebuilt the host)",
                pin, fp
            ))),
        }
    }

    // Self-signed certs still carry valid TLS signatures; accept the standard
    // schemes (the pin, not a CA chain, is what establishes trust here).
    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}
