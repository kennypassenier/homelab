//! A4 hardening (H11): the pinning verifier is the heart of the link
//! security — a silent regression here would make every client accept any
//! certificate. These tests pin its behavior.

use homelab_client::tls::{fingerprint, PinnedVerifier};
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
