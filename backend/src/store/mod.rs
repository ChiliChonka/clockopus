//! Storage backends (ADR-0001). Both run the same operations on one user's data; the contract
//! tests below run the same scenarios against both, so they cannot drift apart.

pub mod pg;
pub mod yaml;

#[cfg(test)]
mod contract;

pub use pg::PgStore;
pub use yaml::{StoreError, YamlStore};

use crate::domain::{DomainError, UserData};

pub enum Store {
    Yaml(YamlStore),
    Postgres(PgStore),
}

impl Store {
    pub async fn read<R>(
        &self,
        user: &str,
        f: impl FnOnce(&UserData) -> R,
    ) -> Result<R, StoreError> {
        match self {
            Store::Yaml(s) => s.read(user, f).await,
            Store::Postgres(s) => s.read(user, f).await,
        }
    }

    pub async fn write<R>(
        &self,
        user: &str,
        f: impl FnOnce(&mut UserData) -> Result<R, DomainError>,
    ) -> Result<R, StoreError> {
        match self {
            Store::Yaml(s) => s.write(user, f).await,
            Store::Postgres(s) => s.write(user, f).await,
        }
    }

    pub async fn delete_user(&self, user: &str) -> Result<(), StoreError> {
        match self {
            Store::Yaml(s) => s.delete_user(user).await,
            Store::Postgres(s) => s.delete_user(user).await,
        }
    }

    /// For `/readyz`: the database answers. The YAML backend is always ready.
    pub async fn ready(&self) -> bool {
        match self {
            Store::Yaml(_) => true,
            Store::Postgres(s) => s.ready().await,
        }
    }
}
