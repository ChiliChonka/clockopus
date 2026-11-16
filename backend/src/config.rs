//! Runtime configuration, read from environment variables (see docs/SPEC.md, section 8).

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMode {
    /// One built-in user `local`. For local trials only.
    None,
    /// OpenID Connect; the frontend signs in as a public client with PKCE.
    Oidc {
        issuer: String,
        client_id: String,
        audience: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Storage {
    Yaml { data_dir: PathBuf },
    Postgres { database_url: String },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub listen: SocketAddr,
    pub auth: AuthMode,
    pub storage: Storage,
    /// Directory with the built Angular app.
    pub static_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Reads the configuration through `get`, so tests do not touch the process environment.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let value = |key: &str| get(key).filter(|v| !v.trim().is_empty());

        let listen = value("CLOCKOPUS_LISTEN")
            .unwrap_or_else(|| "0.0.0.0:8080".to_owned())
            .parse()
            .context("CLOCKOPUS_LISTEN must be an address like 0.0.0.0:8080 or [::]:8080")?;

        let auth = match value("CLOCKOPUS_AUTH").as_deref() {
            Some("none") => AuthMode::None,
            Some("oidc") => {
                let issuer = value("CLOCKOPUS_OIDC_ISSUER")
                    .context("CLOCKOPUS_AUTH=oidc needs CLOCKOPUS_OIDC_ISSUER")?;
                let client_id = value("CLOCKOPUS_OIDC_CLIENT_ID")
                    .context("CLOCKOPUS_AUTH=oidc needs CLOCKOPUS_OIDC_CLIENT_ID")?;
                let audience =
                    value("CLOCKOPUS_OIDC_AUDIENCE").unwrap_or_else(|| client_id.clone());
                AuthMode::Oidc {
                    issuer: issuer.trim_end_matches('/').to_owned(),
                    client_id,
                    audience,
                }
            }
            Some(other) => bail!("CLOCKOPUS_AUTH must be `oidc` or `none`, got `{other}`"),
            // No default on purpose: running without sign-in has to be a conscious choice.
            None => bail!("CLOCKOPUS_AUTH is required: `oidc`, or `none` for a local trial"),
        };

        let storage = match value("CLOCKOPUS_STORAGE").as_deref().unwrap_or("yaml") {
            "yaml" => Storage::Yaml {
                data_dir: value("CLOCKOPUS_DATA_DIR")
                    .unwrap_or_else(|| "/data".to_owned())
                    .into(),
            },
            "postgres" => Storage::Postgres {
                database_url: value("DATABASE_URL")
                    .context("CLOCKOPUS_STORAGE=postgres needs DATABASE_URL")?,
            },
            other => bail!("CLOCKOPUS_STORAGE must be `yaml` or `postgres`, got `{other}`"),
        };

        let static_dir = value("CLOCKOPUS_STATIC_DIR")
            .unwrap_or_else(|| "/app/static".to_owned())
            .into();

        Ok(Self {
            listen,
            auth,
            storage,
            static_dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg(pairs: &[(&str, &str)]) -> anyhow::Result<Config> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Config::from_lookup(|k| map.get(k).cloned())
    }

    #[test]
    fn auth_is_required() {
        let err = cfg(&[]).unwrap_err().to_string();
        assert!(err.contains("CLOCKOPUS_AUTH is required"), "{err}");
    }

    #[test]
    fn defaults_for_a_local_trial() {
        let c = cfg(&[("CLOCKOPUS_AUTH", "none")]).unwrap();
        assert_eq!(c.listen, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(c.auth, AuthMode::None);
        assert_eq!(
            c.storage,
            Storage::Yaml {
                data_dir: "/data".into()
            }
        );
    }

    #[test]
    fn ipv6_listen_address_is_accepted() {
        let c = cfg(&[
            ("CLOCKOPUS_AUTH", "none"),
            ("CLOCKOPUS_LISTEN", "[::]:9000"),
        ])
        .unwrap();
        assert_eq!(c.listen, "[::]:9000".parse().unwrap());
    }

    #[test]
    fn oidc_needs_issuer_and_client_and_defaults_audience() {
        let err = cfg(&[("CLOCKOPUS_AUTH", "oidc")]).unwrap_err().to_string();
        assert!(err.contains("CLOCKOPUS_OIDC_ISSUER"), "{err}");

        let c = cfg(&[
            ("CLOCKOPUS_AUTH", "oidc"),
            (
                "CLOCKOPUS_OIDC_ISSUER",
                "https://auth.example.com/realms/x/",
            ),
            ("CLOCKOPUS_OIDC_CLIENT_ID", "clockopus"),
        ])
        .unwrap();
        assert_eq!(
            c.auth,
            AuthMode::Oidc {
                issuer: "https://auth.example.com/realms/x".into(),
                client_id: "clockopus".into(),
                audience: "clockopus".into(),
            }
        );
    }

    #[test]
    fn postgres_needs_a_database_url() {
        assert!(
            cfg(&[
                ("CLOCKOPUS_AUTH", "none"),
                ("CLOCKOPUS_STORAGE", "postgres")
            ])
            .is_err()
        );
        let c = cfg(&[
            ("CLOCKOPUS_AUTH", "none"),
            ("CLOCKOPUS_STORAGE", "postgres"),
            ("DATABASE_URL", "postgres://x"),
        ])
        .unwrap();
        assert_eq!(
            c.storage,
            Storage::Postgres {
                database_url: "postgres://x".into()
            }
        );
    }

    #[test]
    fn unknown_values_are_rejected() {
        assert!(cfg(&[("CLOCKOPUS_AUTH", "basic")]).is_err());
        assert!(cfg(&[("CLOCKOPUS_AUTH", "none"), ("CLOCKOPUS_STORAGE", "sqlite")]).is_err());
        assert!(cfg(&[("CLOCKOPUS_AUTH", "none"), ("CLOCKOPUS_LISTEN", "8080")]).is_err());
    }
}
