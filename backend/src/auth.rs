//! OpenID Connect sign-in (ADR-0001): the backend checks every bearer token itself. Nothing here
//! is specific to one provider; any issuer with discovery and JWKS works.
//!
//! - Keys come from the issuer's discovery document (`jwks_uri`) and are cached. A token signed
//!   with an unknown key id triggers one reload (key rotation), at most every 30 seconds.
//! - Only asymmetric algorithms are accepted, so a token cannot be "signed" with the public key
//!   as an HMAC secret, and `alg: none` never passes.
//! - Checked: signature, `iss`, `exp`/`nbf` (30 s leeway), `sub`, and that the token was issued
//!   for clockopus: `aud` contains the audience or `azp` equals it (Keycloak access tokens carry
//!   the client in `azp` and `account` in `aud`).

use std::time::{Duration, Instant};

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::{debug, warn};
use uuid::Uuid;

const ALLOWED: [Algorithm; 9] = [
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::PS256,
    Algorithm::PS384,
    Algorithm::PS512,
    Algorithm::ES256,
    Algorithm::ES384,
    Algorithm::EdDSA,
];
const MIN_REFRESH: Duration = Duration::from_secs(30);
const LEEWAY_SECONDS: u64 = 30;

/// The signed-in person as the rest of the app sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Names the user's data in the store; stable as long as `sub` is.
    pub key: String,
    pub name: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    Missing,
    /// The reason is logged, never sent to the client.
    Invalid(&'static str),
    /// The issuer cannot be reached and no keys are cached.
    Unavailable,
}

#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    jwks_uri: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    aud: Option<Audience>,
    #[serde(default)]
    azp: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

#[derive(Default)]
struct Keys {
    jwks_uri: Option<String>,
    set: Option<JwkSet>,
    fetched: Option<Instant>,
}

pub struct OidcVerifier {
    issuer: String,
    audience: String,
    http: reqwest::Client,
    keys: RwLock<Keys>,
}

impl OidcVerifier {
    pub fn new(issuer: &str, audience: &str) -> anyhow::Result<Self> {
        // reqwest is built without a crypto provider; use ring like sqlx (one crypto stack).
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        Ok(Self {
            issuer: issuer.trim_end_matches('/').to_owned(),
            audience: audience.to_owned(),
            http,
            keys: RwLock::new(Keys::default()),
        })
    }

    /// The key set; `force` reloads it unless that happened within [`MIN_REFRESH`].
    async fn key_set(&self, force: bool) -> Result<JwkSet, AuthError> {
        {
            let keys = self.keys.read().await;
            if let Some(set) = &keys.set {
                let recent = keys.fetched.is_some_and(|t| t.elapsed() < MIN_REFRESH);
                if !force || recent {
                    return Ok(set.clone());
                }
            }
        }
        let mut keys = self.keys.write().await;
        if let Some(set) = &keys.set
            && keys.fetched.is_some_and(|t| t.elapsed() < MIN_REFRESH)
        {
            return Ok(set.clone());
        }
        match self.fetch(&mut keys).await {
            Ok(set) => Ok(set),
            Err(e) => {
                warn!(error = format!("{e:#}"), issuer = %self.issuer, "cannot load signing keys");
                // Keep serving with the keys we have, if any.
                keys.set.clone().ok_or(AuthError::Unavailable)
            }
        }
    }

    async fn fetch(&self, keys: &mut Keys) -> anyhow::Result<JwkSet> {
        let uri = match &keys.jwks_uri {
            Some(uri) => uri.clone(),
            None => {
                let url = format!("{}/.well-known/openid-configuration", self.issuer);
                let doc: Discovery = self
                    .http
                    .get(&url)
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                if doc.issuer.trim_end_matches('/') != self.issuer {
                    anyhow::bail!(
                        "discovery document names issuer {}, expected {}",
                        doc.issuer,
                        self.issuer
                    );
                }
                keys.jwks_uri = Some(doc.jwks_uri.clone());
                doc.jwks_uri
            }
        };
        let set: JwkSet = self
            .http
            .get(&uri)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        keys.set = Some(set.clone());
        keys.fetched = Some(Instant::now());
        Ok(set)
    }

    pub async fn verify(&self, token: &str) -> Result<Identity, AuthError> {
        let header = decode_header(token).map_err(|_| AuthError::Invalid("malformed token"))?;
        if !ALLOWED.contains(&header.alg) {
            return Err(AuthError::Invalid("algorithm not allowed"));
        }
        let mut set = self.key_set(false).await?;
        let mut jwk = find_key(&set, header.kid.as_deref());
        if jwk.is_none() {
            // Unknown key id: the issuer may have rotated its keys.
            set = self.key_set(true).await?;
            jwk = find_key(&set, header.kid.as_deref());
        }
        let jwk = jwk.ok_or(AuthError::Invalid("unknown signing key"))?;
        let key =
            DecodingKey::from_jwk(&jwk).map_err(|_| AuthError::Invalid("unusable signing key"))?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&self.issuer]);
        validation.validate_aud = false; // checked below, together with azp
        validation.validate_nbf = true;
        validation.leeway = LEEWAY_SECONDS;
        validation.set_required_spec_claims(&["exp", "iss", "sub"]);
        let claims = decode::<Claims>(token, &key, &validation)
            .map_err(|e| {
                debug!(error = %e, "token rejected");
                AuthError::Invalid("token rejected")
            })?
            .claims;

        let for_us = match &claims.aud {
            Some(Audience::One(a)) => *a == self.audience,
            Some(Audience::Many(list)) => list.contains(&self.audience),
            None => false,
        } || claims.azp.as_deref() == Some(self.audience.as_str());
        if !for_us {
            return Err(AuthError::Invalid("token issued for another client"));
        }
        if claims.sub.is_empty() {
            return Err(AuthError::Invalid("empty subject"));
        }
        let name = [claims.name, claims.preferred_username, claims.email]
            .into_iter()
            .flatten()
            .find(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Signed-in user".to_owned());
        Ok(Identity {
            key: user_key(&claims.sub),
            name,
        })
    }
}

fn find_key(set: &JwkSet, kid: Option<&str>) -> Option<jsonwebtoken::jwk::Jwk> {
    match kid {
        Some(kid) => set.find(kid).cloned(),
        // Without a key id only an unambiguous set works.
        None if set.keys.len() == 1 => set.keys.first().cloned(),
        None => None,
    }
}

/// `sub` itself when it is safe as a directory name (Keycloak: a UUID), otherwise a stable UUID
/// derived from it, so no subject can reach outside the data directory.
pub fn user_key(sub: &str) -> String {
    let safe = sub.len() <= 64
        && sub
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if safe {
        sub.to_owned()
    } else {
        Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("oidc-sub:{sub}").as_bytes()).to_string()
    }
}

#[cfg(test)]
pub mod testing {
    //! A tiny OpenID provider for tests: discovery + JWKS on 127.0.0.1, Ed25519 keys from fixed
    //! seeds (test keys only), tokens signed on demand.

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use axum::routing::get;
    use axum::{Json, Router};
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use ed25519_dalek::SigningKey;
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde_json::{Value, json};

    /// Key id and Ed25519 seed of a published test key.
    pub type PublishedKey = (String, [u8; 32]);

    pub struct TestIssuer {
        pub issuer: String,
        /// Published keys (kid, seed); change it to rotate.
        pub published: Arc<Mutex<Vec<PublishedKey>>>,
        pub jwks_fetches: Arc<AtomicUsize>,
    }

    fn jwk(kid: &str, seed: &[u8; 32]) -> Value {
        let public = SigningKey::from_bytes(seed).verifying_key().to_bytes();
        json!({"kty": "OKP", "crv": "Ed25519", "alg": "EdDSA", "use": "sig",
               "kid": kid, "x": URL_SAFE_NO_PAD.encode(public)})
    }

    impl TestIssuer {
        pub async fn start() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let issuer = format!("http://{}/realms/test", listener.local_addr().unwrap());
            let published = Arc::new(Mutex::new(vec![("key-1".to_owned(), [1u8; 32])]));
            let jwks_fetches = Arc::new(AtomicUsize::new(0));
            let (iss, keys, fetches) = (issuer.clone(), published.clone(), jwks_fetches.clone());
            let app =
                Router::new()
                    .route(
                        "/realms/test/.well-known/openid-configuration",
                        get(move || {
                            let iss = iss.clone();
                            async move {
                                Json(json!({"issuer": iss, "jwks_uri": format!("{iss}/certs")}))
                            }
                        }),
                    )
                    .route(
                        "/realms/test/certs",
                        get(move || {
                            let keys = keys.clone();
                            let fetches = fetches.clone();
                            async move {
                                fetches.fetch_add(1, Ordering::SeqCst);
                                let list: Vec<Value> = keys
                                    .lock()
                                    .unwrap()
                                    .iter()
                                    .map(|(k, s)| jwk(k, s))
                                    .collect();
                                Json(json!({ "keys": list }))
                            }
                        }),
                    );
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            Self {
                issuer,
                published,
                jwks_fetches,
            }
        }

        pub fn fetches(&self) -> usize {
            self.jwks_fetches.load(Ordering::SeqCst)
        }

        /// A token signed with `seed` under `kid`, with the given claims.
        pub fn sign(&self, kid: &str, seed: &[u8; 32], claims: &Value) -> String {
            let der = SigningKey::from_bytes(seed).to_pkcs8_der().unwrap();
            let mut header = Header::new(jsonwebtoken::Algorithm::EdDSA);
            header.kid = Some(kid.to_owned());
            encode(&header, claims, &EncodingKey::from_ed_der(der.as_bytes())).unwrap()
        }

        /// Claims a provider would issue for client `clockopus`, valid for 5 minutes.
        pub fn claims(&self, sub: &str) -> Value {
            let now = jiff::Timestamp::now().as_second();
            json!({"iss": self.issuer, "sub": sub, "aud": "account", "azp": "clockopus",
                   "exp": now + 300, "iat": now, "name": "Anna Example"})
        }

        pub fn token(&self, sub: &str) -> String {
            self.sign("key-1", &[1u8; 32], &self.claims(sub))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::TestIssuer;
    use super::*;
    use serde_json::json;

    async fn setup() -> (TestIssuer, OidcVerifier) {
        let idp = TestIssuer::start().await;
        let verifier = OidcVerifier::new(&idp.issuer, "clockopus").unwrap();
        (idp, verifier)
    }

    #[tokio::test]
    async fn a_valid_token_names_the_user() {
        let (idp, v) = setup().await;
        let id = v
            .verify(&idp.token("6f1c2b8e-1111-4222-8333-944455556666"))
            .await
            .unwrap();
        assert_eq!(id.key, "6f1c2b8e-1111-4222-8333-944455556666");
        assert_eq!(id.name, "Anna Example");
        // Keys are cached: a second token does not fetch again.
        v.verify(&idp.token("other")).await.unwrap();
        assert_eq!(idp.fetches(), 1);
    }

    #[tokio::test]
    async fn rejected_tokens() {
        let (idp, v) = setup().await;
        let now = jiff::Timestamp::now().as_second();
        let mut cases = Vec::new();
        let base = idp.claims("u1");
        let with = |patch: serde_json::Value| {
            let mut c = base.clone();
            for (k, val) in patch.as_object().unwrap() {
                if val.is_null() {
                    c.as_object_mut().unwrap().remove(k);
                } else {
                    c[k] = val.clone();
                }
            }
            c
        };
        cases.push((
            "expired",
            idp.sign("key-1", &[1; 32], &with(json!({"exp": now - 120}))),
        ));
        cases.push((
            "not yet valid",
            idp.sign("key-1", &[1; 32], &with(json!({"nbf": now + 600}))),
        ));
        cases.push((
            "other issuer",
            idp.sign(
                "key-1",
                &[1; 32],
                &with(json!({"iss": "https://evil.example.com/realms/test"})),
            ),
        ));
        cases.push((
            "other client",
            idp.sign(
                "key-1",
                &[1; 32],
                &with(json!({"azp": "grafana", "aud": "account"})),
            ),
        ));
        cases.push((
            "no audience at all",
            idp.sign("key-1", &[1; 32], &with(json!({"azp": null, "aud": null}))),
        ));
        cases.push((
            "no subject",
            idp.sign("key-1", &[1; 32], &with(json!({"sub": null}))),
        ));
        cases.push((
            "empty subject",
            idp.sign("key-1", &[1; 32], &with(json!({"sub": ""}))),
        ));
        cases.push((
            "wrong key under a known kid",
            idp.sign("key-1", &[9; 32], &base),
        ));
        cases.push(("unknown kid", idp.sign("key-9", &[9; 32], &base)));
        // HS256 with the public key as secret (algorithm confusion) and alg "none".
        let hs = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(Algorithm::HS256),
            &base,
            &jsonwebtoken::EncodingKey::from_secret(b"anything").unwrap(),
        )
        .unwrap();
        cases.push(("HS256", hs));
        cases.push((
            "alg none",
            "eyJhbGciOiJub25lIn0.eyJzdWIiOiJ1MSJ9.".to_owned(),
        ));
        cases.push(("garbage", "not-a-token".to_owned()));

        let mut rejected = 0;
        for (name, token) in &cases {
            let result = v.verify(token).await;
            assert!(
                matches!(result, Err(AuthError::Invalid(_))),
                "{name}: {result:?}"
            );
            rejected += 1;
        }
        assert_eq!(rejected, 12);
        // The good token still passes: the rejections were for the reasons above.
        v.verify(&idp.token("u1")).await.unwrap();
    }

    #[tokio::test]
    async fn audience_alone_is_enough_without_azp() {
        let (idp, v) = setup().await;
        let mut c = idp.claims("u1");
        c["aud"] = json!(["clockopus", "account"]);
        c.as_object_mut().unwrap().remove("azp");
        v.verify(&idp.sign("key-1", &[1; 32], &c)).await.unwrap();
    }

    #[tokio::test]
    async fn rotated_keys_are_picked_up_once() {
        let (idp, v) = setup().await;
        v.verify(&idp.token("u1")).await.unwrap();
        assert_eq!(idp.fetches(), 1);
        // The issuer starts signing with a new key.
        idp.published
            .lock()
            .unwrap()
            .push(("key-2".to_owned(), [2; 32]));
        // Right after the first fetch a reload is not allowed yet (at most every 30 s) ...
        let new = idp.sign("key-2", &[2; 32], &idp.claims("u1"));
        assert!(v.verify(&new).await.is_err());
        assert_eq!(idp.fetches(), 1, "no reload storm from unknown key ids");
        // ... once the cache is older, the unknown kid reloads the keys.
        v.keys.write().await.fetched = Some(Instant::now() - MIN_REFRESH);
        v.verify(&new).await.unwrap();
        assert_eq!(idp.fetches(), 2);
    }

    #[tokio::test]
    async fn unreachable_issuer_is_unavailable_not_invalid() {
        let v = OidcVerifier::new("http://127.0.0.1:9/realms/none", "clockopus").unwrap();
        let idp = TestIssuer::start().await;
        let result = v.verify(&idp.token("u1")).await;
        assert_eq!(result, Err(AuthError::Unavailable));
    }

    #[test]
    fn user_keys_are_safe_directory_names() {
        assert_eq!(
            user_key("6f1c2b8e-1111-4222-8333-944455556666"),
            "6f1c2b8e-1111-4222-8333-944455556666"
        );
        assert_eq!(user_key("104857600123456789012"), "104857600123456789012");
        let odd = user_key("../../etc|passwd");
        assert_eq!(odd.len(), 36);
        assert!(odd.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        assert_eq!(odd, user_key("../../etc|passwd"), "stable");
        assert_ne!(odd, user_key("../../etc|passwd2"));
    }
}
