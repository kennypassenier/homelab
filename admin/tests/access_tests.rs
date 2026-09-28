//! arch-exposure: the two locks before the login, decided without I/O.

use homelab_admin::core::access::{
    check_claims, from_home, parse_jwks, parse_jwt, Claims, Refusal,
};

fn b64(s: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s)
}

fn token(claims: &str) -> String {
    format!(
        "{}.{}.{}",
        b64(r#"{"alg":"RS256","kid":"k1","typ":"JWT"}"#),
        b64(claims),
        b64("sig")
    )
}

fn claims(json: &str) -> Claims {
    parse_jwt(&token(json)).unwrap().claims
}

const AUD: &str = "4714c1358e65fe4b408ad6d432a5f878f08194bdb4752441fd56faefa9b2b6f2";
const TEAM: &str = "kp-soft.cloudflareaccess.com";

#[test]
fn arch_exposure_a_token_reads_into_kid_alg_claims_and_signed_bytes() {
    let t = parse_jwt(&token(&format!(
        r#"{{"aud":["{AUD}"],"iss":"https://{TEAM}","exp":2000,"email":"k@example.org"}}"#
    )))
    .unwrap();
    assert_eq!((t.kid.as_str(), t.alg.as_str()), ("k1", "RS256"));
    assert_eq!(t.signature, b"sig");
    assert_eq!(t.signed.matches('.').count(), 1);
    assert!(matches!(parse_jwt("a.b"), Err(Refusal::Malformed(_))));
}

#[test]
fn arch_exposure_audience_issuer_and_time_are_all_checked() {
    let ok = format!(r#"{{"aud":["{AUD}"],"iss":"https://{TEAM}","exp":2000,"nbf":900}}"#);
    assert_eq!(check_claims(&claims(&ok), AUD, TEAM, 1000, 60), Ok(()));
    assert_eq!(
        check_claims(&claims(&ok), "other", TEAM, 1000, 60),
        Err(Refusal::WrongAudience)
    );
    assert_eq!(
        check_claims(&claims(&ok), AUD, "evil.cloudflareaccess.com", 1000, 60),
        Err(Refusal::WrongIssuer)
    );
    assert_eq!(
        check_claims(&claims(&ok), AUD, TEAM, 2100, 60),
        Err(Refusal::Expired)
    );
    assert_eq!(
        check_claims(&claims(&ok), AUD, TEAM, 2050, 60),
        Ok(()),
        "within the leeway"
    );
    assert_eq!(
        check_claims(&claims(&ok), AUD, TEAM, 800, 60),
        Err(Refusal::NotYetValid)
    );
    let single = format!(r#"{{"aud":"{AUD}","iss":"https://{TEAM}/","exp":2000}}"#);
    assert_eq!(
        check_claims(&claims(&single), AUD, TEAM, 1000, 0),
        Ok(()),
        "aud as a string, iss with a slash"
    );
}

#[test]
fn arch_exposure_only_the_house_address_passes_lock_two() {
    assert_eq!(
        from_home(Some("62.235.8.143"), Some("62.235.8.143")),
        Ok(())
    );
    assert_eq!(
        from_home(Some("178.1.2.3"), Some("62.235.8.143")),
        Err(Refusal::NotFromHome {
            from: "178.1.2.3".into()
        })
    );
    assert_eq!(
        from_home(Some("62.235.8.143"), None),
        Err(Refusal::HomeUnknown),
        "unknown home refuses, never waves through"
    );
    assert!(matches!(
        from_home(None, Some("62.235.8.143")),
        Err(Refusal::NotFromHome { .. })
    ));
}

#[test]
fn arch_exposure_the_key_set_keeps_rsa_keys_only() {
    let jwks = r#"{"keys":[{"kid":"k1","kty":"RSA","alg":"RS256","n":"AQAB","e":"AQAB"},{"kid":"k2","kty":"EC","crv":"P-256"}]}"#;
    let keys = parse_jwks(jwks).unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].kid, "k1");
    assert_eq!(keys[0].e, vec![1, 0, 1]);
}

/// Lock 1's signature check against a real RS256 signature: the right key
/// passes, a changed payload and a different key fail.
#[test]
fn arch_exposure_the_signature_is_checked_against_the_key() {
    use aws_lc_rs::encoding::AsDer;
    use aws_lc_rs::rsa::{KeyPair, KeySize};
    use aws_lc_rs::signature::{KeyPair as _, RSA_PKCS1_SHA256};
    use homelab_admin::core::access::{verify_rs256, RsaKey};

    fn public(kp: &KeyPair) -> RsaKey {
        // The SubjectPublicKeyInfo DER holds the modulus and exponent; the
        // components are what a JWKS carries.
        let pk = kp.public_key();
        let _ = pk.as_der();
        let comps = aws_lc_rs::rsa::PublicKeyComponents::<Vec<u8>>::from(pk);
        RsaKey {
            kid: "k1".into(),
            n: comps.n,
            e: comps.e,
        }
    }
    let kp = KeyPair::generate(KeySize::Rsa2048).unwrap();
    let other = KeyPair::generate(KeySize::Rsa2048).unwrap();
    let signed = b"header.payload";
    let rng = aws_lc_rs::rand::SystemRandom::new();
    let mut sig = vec![0u8; kp.public_modulus_len()];
    kp.sign(&RSA_PKCS1_SHA256, &rng, signed, &mut sig).unwrap();
    assert_eq!(verify_rs256(&public(&kp), signed, &sig), Ok(()));
    assert_eq!(
        verify_rs256(&public(&kp), b"header.changed", &sig),
        Err(Refusal::BadSignature)
    );
    assert_eq!(
        verify_rs256(&public(&other), signed, &sig),
        Err(Refusal::BadSignature)
    );
}

/// Kenny, 2026-09-28: a refusal is the kp-themes Alarm. The page carries the
/// alarm's words as data for the module and as plain text without it, all
/// escaped.
#[test]
fn arch_exposure_a_refusal_is_an_alarm_page_with_escaped_words() {
    use homelab_admin::core::access::refusal_page;
    let page = refusal_page(&Refusal::NotFromHome {
        from: "178.1.2.3<script>".into(),
    });
    assert!(page.contains("/static/kp/dist/kp-themes.css"));
    assert!(page.contains("src=\"/refused.js\""));
    assert!(page.contains("data-title=\"Not from home\""));
    assert!(page.contains("178.1.2.3&lt;script&gt;") && !page.contains("<script>\""));
    assert!(
        !page.contains("178.1.2.3<script>"),
        "the address is escaped"
    );
}
