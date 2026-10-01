//! A4 hardening (H11): the pinning verifier is the heart of the link
//! security — a silent regression here would make every client accept any
//! certificate. These tests pin its behavior.

use homelab_client::tls::{PinnedVerifier, fingerprint};
use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

fn test_cert(cn: &str) -> CertificateDer<'static> {
    let mut params = rcgen::CertificateParams::new(vec![cn.to_string()]).unwrap();
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, cn);
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    CertificateDer::from(cert.der().to_vec())
}

fn verify(verifier: &PinnedVerifier, cert: &CertificateDer<'_>) -> Result<(), rustls::Error> {
    verifier
        .verify_server_cert(
            cert,
            &[],
            &ServerName::try_from("10.10.5.250").unwrap(),
            &[],
            UnixTime::now(),
        )
        .map(|_| ())
}

#[test]
fn a4_matching_pin_accepts_mismatch_refuses() {
    let cert_a = test_cert("homelab-host");
    let cert_b = test_cert("evil-twin");
    let pin_a = fingerprint(&cert_a);
    // Correct pin → accepted.
    let v = PinnedVerifier::new(Some(pin_a.clone()));
    verify(&v, &cert_a).expect("matching fingerprint must be accepted");
    // Different cert, same pin → refused. THE core guarantee.
    let v = PinnedVerifier::new(Some(pin_a));
    verify(&v, &cert_b).expect_err("mismatched fingerprint must be refused");
}

#[test]
fn a4_tofu_records_the_observed_fingerprint() {
    let cert = test_cert("homelab-host");
    let v = PinnedVerifier::new(None); // first connect: nothing pinned yet
    verify(&v, &cert).expect("TOFU accepts the first cert");
    assert_eq!(
        v.observed(),
        Some(fingerprint(&cert)),
        "the observed fingerprint must be recorded for pinning"
    );
}

#[test]
fn a4_fingerprints_are_stable_and_distinct() {
    let a = test_cert("homelab-host");
    let b = test_cert("homelab-host");
    assert_eq!(fingerprint(&a), fingerprint(&a), "deterministic");
    assert_ne!(fingerprint(&a), fingerprint(&b), "different keys differ");
}

// fix-34: the pin compared the certificate's fingerprint, but the handshake
// signature, the only step that proves the server holds the matching private
// key, was accepted without being checked. A certificate is public: anyone
// who once connected has its bytes. So an impostor presenting the real
// certificate, signing with a key of its own, passed the pin and received
// the bearer token. These tests run a real handshake against such an
// impostor.

struct FixedCert(std::sync::Arc<rustls::sign::CertifiedKey>);

impl std::fmt::Debug for FixedCert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FixedCert")
    }
}

impl rustls::server::ResolvesServerCert for FixedCert {
    fn resolve(
        &self,
        _hello: rustls::server::ClientHello<'_>,
    ) -> Option<std::sync::Arc<rustls::sign::CertifiedKey>> {
        Some(self.0.clone())
    }
}

/// A certificate plus the private key it was issued for.
fn cert_and_key(cn: &str) -> (CertificateDer<'static>, rcgen::KeyPair) {
    let mut params = rcgen::CertificateParams::new(vec![cn.to_string()]).unwrap();
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, cn);
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    (CertificateDer::from(cert.der().to_vec()), key)
}

/// Handshake a pinned client against a server presenting `cert` and signing
/// with `signing_key`, over an in-memory pipe. Ok when the client accepted.
async fn handshake(
    pin: &str,
    cert: CertificateDer<'static>,
    signing_key: &rcgen::KeyPair,
) -> Result<(), String> {
    use std::sync::Arc;
    let key_der = rustls::pki_types::PrivateKeyDer::try_from(signing_key.serialize_der()).unwrap();
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let signer = provider.key_provider.load_private_key(key_der).unwrap();
    let certified = Arc::new(rustls::sign::CertifiedKey::new(vec![cert], signer));
    let server_cfg = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(FixedCert(certified)));
    let client_cfg = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(PinnedVerifier::new(Some(pin.to_string())))
        .with_no_client_auth();

    let (c, s) = tokio::io::duplex(64 * 1024);
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));
    let server = tokio::spawn(async move { acceptor.accept(s).await.map(|_| ()) });
    let name = ServerName::try_from("10.10.5.250").unwrap();
    let client = connector.connect(name, c).await.map(|_| ());
    let _ = server.await;
    client.map_err(|e| e.to_string())
}

#[tokio::test]
async fn fix_34_the_real_host_with_its_own_key_is_accepted() {
    let (cert, key) = cert_and_key("homelab-host");
    let pin = fingerprint(&cert);
    handshake(&pin, cert, &key)
        .await
        .expect("the pinned certificate signed with its own key must pass");
}

#[tokio::test]
async fn fix_34_an_impostor_replaying_the_pinned_certificate_is_refused() {
    let (cert, _real_key) = cert_and_key("homelab-host");
    let (_, impostor_key) = cert_and_key("impostor");
    let pin = fingerprint(&cert);
    let got = handshake(&pin, cert, &impostor_key).await;
    assert!(
        got.is_err(),
        "a server that presents the pinned certificate but signs with another key \
         must be refused; the handshake was accepted"
    );
}
