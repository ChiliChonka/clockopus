//! Storage backends. Only YAML exists so far; PostgreSQL follows in milestone M5, together with
//! the shared contract tests that keep both backends identical (ADR-0001).

pub mod yaml;

pub use yaml::{StoreError, YamlStore};
