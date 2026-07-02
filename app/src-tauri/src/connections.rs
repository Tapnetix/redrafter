// Minimal connection store: the Phase-A seed for the first-run provider
// chooser (`FirstRun.tsx`).
//
// Persists the provider connections the user adds (a cloud API key or a
// local Ollama endpoint) using the same SQLite-backed pattern as
// `settings.rs`. Phase B's `B7b` is the sole owner of extending this file
// with edit/remove/test/refresh/manual-model-id commands and a richer
// `Connection` shape (`key_ref`, discovered models, etc.) for the full
// Connections screen — this task only needs enough to add a connection and
// list what has been added, verifying reachability through llm-provider's
// OpenAI-compatible provider before persisting.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/core_test.rs` via `include!` inside an inline `mod` block, and
// Rust doesn't allow an inner doc comment produced by macro expansion to
// sit at the start of that block.)

use anyhow::{bail, Result};
use rusqlite::Connection as SqliteConnection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

use llm_provider::{LlmProvider, OpenAiCompatProvider};

/// A stored provider connection.
///
/// `rename_all = "camelCase"` only affects the JSON wire format (the shape
/// the frontend's `app/src/lib/ipc.ts` sees); the Rust field names stay
/// snake_case per convention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub provider_kind: String,
    pub base_url: String,
    pub enabled_models: Vec<String>,
}

pub struct ConnectionStore {
    conn: Mutex<SqliteConnection>,
}

impl ConnectionStore {
    /// Opens a file-backed SQLite database at the given path, creating the
    /// connections table if this is the first run.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = SqliteConnection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Opens an in-memory SQLite database (used by tests and defaults).
    pub fn open_in_memory() -> Result<Self> {
        let conn = SqliteConnection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS connections (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                provider_kind  TEXT NOT NULL,
                base_url       TEXT NOT NULL,
                enabled_models TEXT NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    /// Persists a new connection with the given enabled models, returning
    /// the stored record (including its generated id).
    pub fn add(
        &self,
        provider_kind: &str,
        base_url: &str,
        enabled_models: &[String],
    ) -> Result<Connection> {
        let conn = self.conn.lock().unwrap();
        let models_json = serde_json::to_string(enabled_models)?;
        conn.execute(
            "INSERT INTO connections (provider_kind, base_url, enabled_models)
             VALUES (?1, ?2, ?3)",
            (provider_kind, base_url, &models_json),
        )?;
        let id = conn.last_insert_rowid();
        Ok(Connection {
            id: id.to_string(),
            provider_kind: provider_kind.to_string(),
            base_url: base_url.to_string(),
            enabled_models: enabled_models.to_vec(),
        })
    }

    /// Returns every stored connection, oldest first.
    pub fn list(&self) -> Result<Vec<Connection>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, provider_kind, base_url, enabled_models FROM connections ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let provider_kind: String = row.get(1)?;
            let base_url: String = row.get(2)?;
            let models_json: String = row.get(3)?;
            Ok((id, provider_kind, base_url, models_json))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, provider_kind, base_url, models_json) = row?;
            let enabled_models: Vec<String> =
                serde_json::from_str(&models_json).unwrap_or_default();
            out.push(Connection {
                id: id.to_string(),
                provider_kind,
                base_url,
                enabled_models,
            });
        }
        Ok(out)
    }
}

/// The default enabled model when a reachable provider's discovery
/// (`list_models`) returns nothing. Real per-vendor discovery/defaults are
/// Phase B's job (`B7b`); this is just a placeholder so the first
/// connection always has an active model.
const FALLBACK_MODEL: &str = "default";

/// Verifies a provider is reachable, then persists it with a default
/// enabled model — the first discovered model, or [`FALLBACK_MODEL`] if
/// discovery comes back empty. Nothing is persisted if the provider is
/// unreachable. Takes `&dyn LlmProvider` so it is unit-testable with a fake
/// (see `tests::FakeProvider`) instead of a live HTTP endpoint.
pub async fn connect_and_store(
    store: &ConnectionStore,
    provider: &dyn LlmProvider,
    provider_kind: &str,
    base_url: &str,
) -> Result<Connection> {
    if !provider.is_available().await {
        bail!("could not connect to {provider_kind} at {base_url}");
    }

    let models = provider.list_models().await.unwrap_or_default();
    let default_model = models
        .into_iter()
        .next()
        .unwrap_or_else(|| FALLBACK_MODEL.to_string());

    store.add(provider_kind, base_url, &[default_model])
}

/// Tauri command: connects a provider (verifying reachability through the
/// OpenAI-compatible `llm-provider` implementation) and persists it with a
/// default enabled model. Registered by A14 (`app/src-tauri/src/lib.rs`),
/// which manages a `ConnectionStore` as state.
#[tauri::command]
pub async fn connection_add(
    state: tauri::State<'_, ConnectionStore>,
    provider_kind: String,
    base_url: String,
    api_key: String,
) -> Result<Connection, String> {
    let provider = OpenAiCompatProvider::new(&base_url, &api_key);
    connect_and_store(&state, &provider, &provider_kind, &base_url)
        .await
        .map_err(|e| e.to_string())
}

/// Tauri command: lists every stored connection. Registered by A14
/// (`app/src-tauri/src/lib.rs`), which manages a `ConnectionStore` as state.
#[tauri::command]
pub fn connection_list(state: tauri::State<'_, ConnectionStore>) -> Result<Vec<Connection>, String> {
    state.list().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_store() -> ConnectionStore {
        ConnectionStore::open_in_memory().expect("failed to open in-memory store")
    }

    #[test]
    fn list_is_empty_for_a_fresh_store() {
        let store = new_store();
        assert_eq!(store.list().unwrap(), vec![]);
    }

    #[test]
    fn add_then_list_returns_the_stored_connection() {
        let store = new_store();
        let added = store
            .add("openai", "https://api.openai.com", &["gpt-4o-mini".to_string()])
            .unwrap();

        assert_eq!(added.provider_kind, "openai");
        assert_eq!(added.base_url, "https://api.openai.com");
        assert_eq!(added.enabled_models, vec!["gpt-4o-mini".to_string()]);

        let listed = store.list().unwrap();
        assert_eq!(listed, vec![added]);
    }

    #[test]
    fn add_assigns_distinct_ids_in_insertion_order() {
        let store = new_store();
        let first = store.add("anthropic", "https://api.anthropic.com", &[]).unwrap();
        let second = store.add("ollama", "http://localhost:11434", &[]).unwrap();

        assert_ne!(first.id, second.id);
        let listed = store.list().unwrap();
        assert_eq!(listed, vec![first, second]);
    }

    /// A fake `LlmProvider` so `connect_and_store` is unit-testable without a
    /// live HTTP endpoint (mirrors `permission.rs`'s fake `AccessibilityChecker`).
    struct FakeProvider {
        available: bool,
        models: Vec<String>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for FakeProvider {
        async fn chat(
            &self,
            _request: &llm_provider::LlmRequest,
            _cancel: tokio_util::sync::CancellationToken,
        ) -> Result<llm_provider::LlmResponse> {
            unimplemented!("connect_and_store never calls chat")
        }

        async fn list_models(&self) -> Result<Vec<String>> {
            Ok(self.models.clone())
        }

        async fn is_available(&self) -> bool {
            self.available
        }

        fn provider_name(&self) -> &'static str {
            "fake"
        }
    }

    #[tokio::test]
    async fn connect_and_store_persists_the_first_discovered_model() {
        let store = new_store();
        let provider = FakeProvider {
            available: true,
            models: vec!["gpt-4o-mini".to_string(), "gpt-4o".to_string()],
        };

        let connection = connect_and_store(&store, &provider, "openai", "https://api.openai.com")
            .await
            .unwrap();

        assert_eq!(connection.provider_kind, "openai");
        assert_eq!(connection.enabled_models, vec!["gpt-4o-mini".to_string()]);
        assert_eq!(store.list().unwrap(), vec![connection]);
    }

    #[tokio::test]
    async fn connect_and_store_falls_back_to_a_default_model_when_discovery_is_empty() {
        let store = new_store();
        let provider = FakeProvider {
            available: true,
            models: vec![],
        };

        let connection = connect_and_store(&store, &provider, "ollama", "http://localhost:11434")
            .await
            .unwrap();

        assert_eq!(connection.enabled_models, vec!["default".to_string()]);
    }

    #[tokio::test]
    async fn connect_and_store_errors_and_persists_nothing_when_unreachable() {
        let store = new_store();
        let provider = FakeProvider {
            available: false,
            models: vec![],
        };

        let result = connect_and_store(&store, &provider, "openai", "https://api.openai.com").await;

        assert!(result.is_err());
        assert_eq!(store.list().unwrap(), vec![]);
    }
}
