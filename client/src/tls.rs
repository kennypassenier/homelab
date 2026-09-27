//! Client-side TLS with certificate pinning (A4, TOFU model): on first
//! connect we record the host cert's SHA-256 fingerprint; on every later
//! connect we require the exact same certificate, like SSH's known_hosts.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
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
    algorithms: WebPkiSupportedAlgorithms,
}

impl PinnedVerifier {
    pub fn new(expected: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            expected,
            observed: std::sync::Mutex::new(None),
            algorithms: rustls::crypto::aws_lc_rs::default_provider()
                .signature_verification_algorithms,
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

    // fix-34: the handshake signature is what proves the server holds the
    // private key of the certificate it presented. The pin only says WHICH
    // certificate is trusted; a certificate is public, so without this check
    // an impostor replaying the real host's certificate passed the pin and
    // received the bearer token. Until 2026-09-27 both methods returned
    // "valid" without looking.
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}
