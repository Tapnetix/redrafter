// Connection store: backs both the first-run provider chooser
// (`FirstRun.tsx`, Phase A) and the full Connections screen (`B7`).
//
// Persists the provider connections the user adds (a cloud API key or a
// local Ollama endpoint) using the same SQLite-backed pattern as
// `settings.rs`. A7 seeded this file with a minimal `connection_add`/
// `connection_list` (always talking OpenAI-compatible, regardless of
// vendor); `B7b` is the sole owner of everything else here: the richer
// `Connection` shape (`key_ref`, `available_models`), the full CRUD surface
// (`connection_edit`/`connection_remove`), reachability probing without
// persisting (`connection_test`), re-running discovery
// (`connection_refresh_models`), manual model-id entry
// (`model_add_manual`), and `provider_for` -- the per-vendor factory that
// closes A7's handoff by selecting the right `llm-provider` implementation
// (native Anthropic/Gemini/Ollama, or the OpenAI-compatible shape) instead
// of defaulting every kind to OpenAI-compatible.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/core_test.rs` via `include!` inside an inline `mod` block, and
// Rust doesn't allow an inner doc comment produced by macro expansion to
// sit at the start of that block.)

use anyhow::{bail, Result};
use rusqlite::{Connection as SqliteConnection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

use llm_provider::discover::accept_manual_model_id;
use llm_provider::{
    discover, AnthropicProvider, DiscoveryResult, GeminiProvider, LlmProvider, OllamaProvider,
    OpenAiCompatProvider,
};

/// A stored provider connection.
///
/// `rename_all = "camelCase"` only affects the JSON wire format (the shape
/// the frontend's `app/src/lib/ipc.ts` sees); the Rust field names stay
/// snake_case per convention.
///
/// `key_ref` is additive over Phase A's shape: `None` when no API key has
/// been set (e.g. a keyless local Ollama endpoint), `Some(id)` once one
/// has. It deliberately never carries the key material itself -- the
/// actual secret lives in the `connections` table's `api_key` column
/// (never serialized) until B10 swaps that storage for its encrypted-file
/// / keychain backend; `key_ref` is the stable handle B10's `secrets_set`
/// will key off of, so this field can stay put across that swap.
///
/// `available_models` holds the last set of model ids `connection_refresh_models`
/// discovered (or a manually-entered id from `model_add_manual`); `enabled_models`
/// is the user-curated subset of those actually usable for refining.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub provider_kind: String,
    pub base_url: String,
    pub enabled_models: Vec<String>,
    #[serde(default)]
    pub available_models: Vec<String>,
    #[serde(default)]
    pub key_ref: Option<String>,
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
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                provider_kind     TEXT NOT NULL,
                base_url          TEXT NOT NULL,
                api_key           TEXT,
                enabled_models    TEXT NOT NULL,
                available_models  TEXT NOT NULL DEFAULT '[]'
            )",
            [],
        )?;
        Ok(())
    }

    /// Persists a new connection with the given enabled models, returning
    /// the stored record (including its generated id). `api_key` is
    /// `None`/empty for keyless endpoints (e.g. local Ollama).
    pub fn add(
        &self,
        provider_kind: &str,
        base_url: &str,
        api_key: Option<&str>,
        enabled_models: &[String],
    ) -> Result<Connection> {
        let conn = self.conn.lock().unwrap();
        let models_json = serde_json::to_string(enabled_models)?;
        let api_key = api_key.filter(|k| !k.is_empty());
        conn.execute(
            "INSERT INTO connections (provider_kind, base_url, api_key, enabled_models, available_models)
             VALUES (?1, ?2, ?3, ?4, '[]')",
            (provider_kind, base_url, api_key, &models_json),
        )?;
        let id = conn.last_insert_rowid();
        Ok(Connection {
            id: id.to_string(),
            provider_kind: provider_kind.to_string(),
            base_url: base_url.to_string(),
            enabled_models: enabled_models.to_vec(),
            available_models: Vec::new(),
            key_ref: api_key.map(|_| id.to_string()),
        })
    }

    /// Returns every stored connection, oldest first.
    pub fn list(&self) -> Result<Vec<Connection>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, provider_kind, base_url, api_key, enabled_models, available_models
             FROM connections ORDER BY id",
        )?;
        let rows = stmt.query_map([], Self::row_to_connection)?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Returns a single connection by id, or `None` if it doesn't exist.
    pub fn get(&self, id: &str) -> Result<Option<Connection>> {
        let row_id: i64 = id.parse()?;
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, provider_kind, base_url, api_key, enabled_models, available_models
             FROM connections WHERE id = ?1",
            [row_id],
            Self::row_to_connection,
        )
        .optional()
        .map_err(Into::into)
    }

    /// Returns the stored API key for a connection, if any (`None` for
    /// keyless connections or an unknown id). Never exposed to the
    /// frontend directly -- only used internally to build a provider.
    pub fn api_key(&self, id: &str) -> Result<Option<String>> {
        let row_id: i64 = id.parse()?;
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT api_key FROM connections WHERE id = ?1",
            [row_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map(|opt| opt.flatten())
        .map_err(Into::into)
    }

    /// Updates a connection's editable fields. Each parameter left `None`
    /// leaves the existing value unchanged (`api_key: Some(None)` clears a
    /// previously-set key). Fails if the connection doesn't exist.
    pub fn edit(
        &self,
        id: &str,
        base_url: Option<&str>,
        api_key: Option<Option<&str>>,
        enabled_models: Option<&[String]>,
    ) -> Result<Connection> {
        let existing = self
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("no connection with id {id}"))?;

        let row_id: i64 = id.parse()?;
        let new_base_url = base_url.unwrap_or(&existing.base_url).to_string();
        let new_enabled_models = enabled_models
            .map(|m| m.to_vec())
            .unwrap_or(existing.enabled_models);
        let models_json = serde_json::to_string(&new_enabled_models)?;

        let conn = self.conn.lock().unwrap();
        match api_key {
            Some(key) => {
                let key = key.filter(|k| !k.is_empty());
                conn.execute(
                    "UPDATE connections SET base_url = ?1, api_key = ?2, enabled_models = ?3 WHERE id = ?4",
                    (&new_base_url, key, &models_json, row_id),
                )?;
            }
            None => {
                conn.execute(
                    "UPDATE connections SET base_url = ?1, enabled_models = ?2 WHERE id = ?3",
                    (&new_base_url, &models_json, row_id),
                )?;
            }
        }
        drop(conn);

        self.get(id)?
            .ok_or_else(|| anyhow::anyhow!("connection {id} vanished after edit"))
    }

    /// Deletes a connection (and, implicitly, its enabled/available
    /// models -- there's no separate models table to detach from).
    /// Succeeds even if the id doesn't exist (idempotent remove).
    pub fn remove(&self, id: &str) -> Result<()> {
        let row_id: i64 = id.parse()?;
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM connections WHERE id = ?1", [row_id])?;
        Ok(())
    }

    /// Overwrites the available (discovered) model list for a connection.
    pub fn set_available_models(&self, id: &str, models: &[String]) -> Result<Connection> {
        let row_id: i64 = id.parse()?;
        let models_json = serde_json::to_string(models)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET available_models = ?1 WHERE id = ?2",
            (&models_json, row_id),
        )?;
        drop(conn);
        self.get(id)?
            .ok_or_else(|| anyhow::anyhow!("no connection with id {id}"))
    }

    /// Adds `model_id` to both the available and enabled lists if not
    /// already present (used by manual model entry). Idempotent.
    pub fn add_model(&self, id: &str, model_id: &str) -> Result<Connection> {
        let existing = self
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("no connection with id {id}"))?;

        let mut available = existing.available_models;
        if !available.iter().any(|m| m == model_id) {
            available.push(model_id.to_string());
        }
        let mut enabled = existing.enabled_models;
        if !enabled.iter().any(|m| m == model_id) {
            enabled.push(model_id.to_string());
        }

        let row_id: i64 = id.parse()?;
        let available_json = serde_json::to_string(&available)?;
        let enabled_json = serde_json::to_string(&enabled)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET available_models = ?1, enabled_models = ?2 WHERE id = ?3",
            (&available_json, &enabled_json, row_id),
        )?;
        drop(conn);
        self.get(id)?
            .ok_or_else(|| anyhow::anyhow!("connection {id} vanished after add_model"))
    }

    fn row_to_connection(row: &rusqlite::Row<'_>) -> rusqlite::Result<Connection> {
        let id: i64 = row.get(0)?;
        let provider_kind: String = row.get(1)?;
        let base_url: String = row.get(2)?;
        let api_key: Option<String> = row.get(3)?;
        let enabled_json: String = row.get(4)?;
        let available_json: String = row.get(5)?;
        let enabled_models: Vec<String> = serde_json::from_str(&enabled_json).unwrap_or_default();
        let available_models: Vec<String> =
            serde_json::from_str(&available_json).unwrap_or_default();
        Ok(Connection {
            id: id.to_string(),
            provider_kind,
            base_url,
            enabled_models,
            available_models,
            key_ref: api_key.filter(|k| !k.is_empty()).map(|_| id.to_string()),
        })
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
    api_key: Option<&str>,
) -> Result<Connection> {
    if !provider.is_available().await {
        bail!("could not connect to {provider_kind} at {base_url}");
    }

    let models = provider.list_models().await.unwrap_or_default();
    let default_model = models
        .into_iter()
        .next()
        .unwrap_or_else(|| FALLBACK_MODEL.to_string());

    store.add(provider_kind, base_url, api_key, &[default_model])
}

/// Builds the [`LlmProvider`] implementation matching `provider_kind`,
/// closing the handoff A7 left open (its `connection_add` always used
/// [`OpenAiCompatProvider`], which silently mis-talks to Anthropic/Gemini's
/// native APIs). `anthropic`/`gemini`/`ollama` get their native
/// implementation; everything else (`openai`, `openai-compat`, and any
/// unrecognized kind) falls back to the OpenAI-compatible shape, which
/// covers OpenAI itself plus self-hosted/compatible servers.
///
/// Ollama takes no API key (`OllamaProvider::new` has no such parameter):
/// local endpoints are keyless, so any provided key is simply unused for
/// that branch.
pub fn provider_for(provider_kind: &str, base_url: &str, api_key: &str) -> Box<dyn LlmProvider> {
    match provider_kind {
        "ollama" => Box::new(OllamaProvider::new(base_url)),
        "anthropic" => Box::new(AnthropicProvider::new(base_url, api_key)),
        "gemini" => Box::new(GeminiProvider::new(base_url, api_key)),
        _ => Box::new(OpenAiCompatProvider::new(base_url, api_key)),
    }
}

/// Tauri command: connects a provider (verifying reachability through the
/// vendor-appropriate `llm-provider` implementation, see [`provider_for`])
/// and persists it with a default enabled model. Registered by A14
/// (`app/src-tauri/src/lib.rs`), which manages a `ConnectionStore` as state.
#[tauri::command]
pub async fn connection_add(
    state: tauri::State<'_, ConnectionStore>,
    provider_kind: String,
    base_url: String,
    api_key: String,
) -> Result<Connection, String> {
    let provider = provider_for(&provider_kind, &base_url, &api_key);
    connect_and_store(
        &state,
        provider.as_ref(),
        &provider_kind,
        &base_url,
        Some(api_key.as_str()),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Tauri command: lists every stored connection. Registered by A14
/// (`app/src-tauri/src/lib.rs`), which manages a `ConnectionStore` as state.
#[tauri::command]
pub fn connection_list(state: tauri::State<'_, ConnectionStore>) -> Result<Vec<Connection>, String> {
    state.list().map_err(|e| e.to_string())
}

/// Updates an existing connection's base URL, API key, and/or enabled
/// models. Unlike `connection_add`, this does not re-verify reachability
/// (that's `connection_test`'s job) -- it's a plain persisted edit.
pub fn connection_edit_impl(
    store: &ConnectionStore,
    id: &str,
    base_url: Option<&str>,
    api_key: Option<Option<&str>>,
    enabled_models: Option<&[String]>,
) -> Result<Connection> {
    store.edit(id, base_url, api_key, enabled_models)
}

/// Tauri command wrapping [`connection_edit_impl`]. `api_key: None` means
/// "leave the key unchanged"; `Some(String::new())` clears it.
#[tauri::command]
pub fn connection_edit(
    state: tauri::State<'_, ConnectionStore>,
    id: String,
    base_url: Option<String>,
    api_key: Option<String>,
    enabled_models: Option<Vec<String>>,
) -> Result<Connection, String> {
    connection_edit_impl(
        &state,
        &id,
        base_url.as_deref(),
        api_key.as_deref().map(Some),
        enabled_models.as_deref(),
    )
    .map_err(|e| e.to_string())
}

/// Deletes a connection, detaching whatever enabled/available models it
/// had (there's no separate models table -- they live on the row that's
/// being deleted).
pub fn connection_remove_impl(store: &ConnectionStore, id: &str) -> Result<()> {
    store.remove(id)
}

/// Tauri command wrapping [`connection_remove_impl`].
#[tauri::command]
pub fn connection_remove(
    state: tauri::State<'_, ConnectionStore>,
    id: String,
) -> Result<(), String> {
    connection_remove_impl(&state, &id).map_err(|e| e.to_string())
}

/// Probes reachability without persisting anything -- lets the Connections
/// screen show a pass/fail result before the user commits to saving a
/// connection (or to confirm a saved one is still reachable).
pub async fn connection_test_impl(provider: &dyn LlmProvider, base_url: &str) -> Result<(), String> {
    if provider.is_available().await {
        Ok(())
    } else {
        Err(format!(
            "could not connect to {} at {base_url}",
            provider.provider_name()
        ))
    }
}

/// Tauri command wrapping [`connection_test_impl`]; builds the provider via
/// [`provider_for`] rather than persisting a connection first.
#[tauri::command]
pub async fn connection_test(
    provider_kind: String,
    base_url: String,
    api_key: String,
) -> Result<(), String> {
    let provider = provider_for(&provider_kind, &base_url, &api_key);
    connection_test_impl(provider.as_ref(), &base_url).await
}

/// Re-runs discovery for an existing connection. On
/// [`DiscoveryResult::Discovered`], persists the newly discovered models as
/// the connection's available list and returns the refreshed connection.
/// On [`DiscoveryResult::ManualEntryRequired`], the store is left
/// untouched and the reason is returned so the caller can prompt for a
/// manually-entered model id via [`model_add_manual_impl`].
pub async fn connection_refresh_models_impl(
    store: &ConnectionStore,
    provider: &dyn LlmProvider,
    id: &str,
) -> Result<Result<Connection, String>> {
    match discover(provider).await {
        DiscoveryResult::Discovered(models) => {
            Ok(Ok(store.set_available_models(id, &models)?))
        }
        DiscoveryResult::ManualEntryRequired { reason } => Ok(Err(reason)),
    }
}

/// Tauri command wrapping [`connection_refresh_models_impl`]. Looks up the
/// connection's stored kind/base URL/key to rebuild its provider, then
/// re-runs discovery. Returns `Err` both when the connection is unknown
/// and when discovery degrades to manual entry -- the frontend
/// distinguishes the latter by falling back to the manual-id control
/// (`model_add_manual`), per B7's plan.
#[tauri::command]
pub async fn connection_refresh_models(
    state: tauri::State<'_, ConnectionStore>,
    id: String,
) -> Result<Connection, String> {
    let existing = state
        .get(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no connection with id {id}"))?;
    let api_key = state.api_key(&id).map_err(|e| e.to_string())?;
    let provider = provider_for(
        &existing.provider_kind,
        &existing.base_url,
        api_key.as_deref().unwrap_or(""),
    );
    connection_refresh_models_impl(&state, provider.as_ref(), &id)
        .await
        .map_err(|e| e.to_string())?
}

/// Accepts a manually-entered model id for a connection whose discovery
/// degraded to [`DiscoveryResult::ManualEntryRequired`] (e.g. an endpoint
/// with no `/models` listing). Delegates the id-format check to
/// `llm-provider`'s `accept_manual_model_id` (a "trust on entry" check,
/// see that function's docs) before adding it to the connection.
pub fn model_add_manual_impl(
    store: &ConnectionStore,
    id: &str,
    model_id: &str,
) -> Result<Connection, String> {
    let accepted = accept_manual_model_id(model_id).map_err(|e| e.to_string())?;
    store.add_model(id, &accepted).map_err(|e| e.to_string())
}

/// Tauri command wrapping [`model_add_manual_impl`].
#[tauri::command]
pub fn model_add_manual(
    state: tauri::State<'_, ConnectionStore>,
    id: String,
    model_id: String,
) -> Result<Connection, String> {
    model_add_manual_impl(&state, &id, &model_id)
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
            .add(
                "openai",
                "https://api.openai.com",
                Some("sk-test"),
                &["gpt-4o-mini".to_string()],
            )
            .unwrap();

        assert_eq!(added.provider_kind, "openai");
        assert_eq!(added.base_url, "https://api.openai.com");
        assert_eq!(added.enabled_models, vec!["gpt-4o-mini".to_string()]);
        assert_eq!(added.key_ref, Some(added.id.clone()));

        let listed = store.list().unwrap();
        assert_eq!(listed, vec![added]);
    }

    #[test]
    fn add_without_a_key_leaves_key_ref_none() {
        let store = new_store();
        let added = store
            .add("ollama", "http://localhost:11434", None, &[])
            .unwrap();

        assert_eq!(added.key_ref, None);
    }

    #[test]
    fn add_assigns_distinct_ids_in_insertion_order() {
        let store = new_store();
        let first = store
            .add("anthropic", "https://api.anthropic.com", None, &[])
            .unwrap();
        let second = store
            .add("ollama", "http://localhost:11434", None, &[])
            .unwrap();

        assert_ne!(first.id, second.id);
        let listed = store.list().unwrap();
        assert_eq!(listed, vec![first, second]);
    }

    #[test]
    fn api_key_returns_the_stored_key_but_never_appears_on_the_connection() {
        let store = new_store();
        let added = store
            .add("openai", "https://api.openai.com", Some("sk-secret"), &[])
            .unwrap();

        assert_eq!(store.api_key(&added.id).unwrap(), Some("sk-secret".to_string()));
        assert_eq!(added.key_ref, Some(added.id.clone()));
    }

    #[test]
    fn edit_updates_base_url_and_enabled_models_without_touching_the_key() {
        let store = new_store();
        let added = store
            .add("openai", "https://api.openai.com", Some("sk-secret"), &["gpt-4o".to_string()])
            .unwrap();

        let edited = store
            .edit(
                &added.id,
                Some("https://api.example.com"),
                None,
                Some(&["gpt-4o-mini".to_string()]),
            )
            .unwrap();

        assert_eq!(edited.base_url, "https://api.example.com");
        assert_eq!(edited.enabled_models, vec!["gpt-4o-mini".to_string()]);
        assert_eq!(store.api_key(&added.id).unwrap(), Some("sk-secret".to_string()));
    }

    #[test]
    fn edit_can_replace_the_key() {
        let store = new_store();
        let added = store
            .add("openai", "https://api.openai.com", Some("sk-old"), &[])
            .unwrap();

        store.edit(&added.id, None, Some(Some("sk-new")), None).unwrap();

        assert_eq!(store.api_key(&added.id).unwrap(), Some("sk-new".to_string()));
    }

    #[test]
    fn edit_can_clear_the_key() {
        let store = new_store();
        let added = store
            .add("openai", "https://api.openai.com", Some("sk-old"), &[])
            .unwrap();

        let edited = store.edit(&added.id, None, Some(None), None).unwrap();

        assert_eq!(store.api_key(&added.id).unwrap(), None);
        assert_eq!(edited.key_ref, None);
    }

    #[test]
    fn edit_of_an_unknown_id_errors() {
        let store = new_store();
        assert!(store.edit("999", Some("https://x"), None, None).is_err());
    }

    #[test]
    fn remove_deletes_the_connection() {
        let store = new_store();
        let first = store
            .add("openai", "https://api.openai.com", None, &[])
            .unwrap();
        let second = store
            .add("ollama", "http://localhost:11434", None, &[])
            .unwrap();

        store.remove(&first.id).unwrap();

        assert_eq!(store.list().unwrap(), vec![second]);
    }

    #[test]
    fn remove_of_an_unknown_id_is_a_no_op() {
        let store = new_store();
        assert!(store.remove("999").is_ok());
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

        let connection = connect_and_store(
            &store,
            &provider,
            "openai",
            "https://api.openai.com",
            Some("sk-test"),
        )
        .await
        .unwrap();

        assert_eq!(connection.provider_kind, "openai");
        assert_eq!(connection.enabled_models, vec!["gpt-4o-mini".to_string()]);
        assert_eq!(connection.key_ref, Some(connection.id.clone()));
        assert_eq!(store.list().unwrap(), vec![connection]);
    }

    #[tokio::test]
    async fn connect_and_store_falls_back_to_a_default_model_when_discovery_is_empty() {
        let store = new_store();
        let provider = FakeProvider {
            available: true,
            models: vec![],
        };

        let connection = connect_and_store(
            &store,
            &provider,
            "ollama",
            "http://localhost:11434",
            None,
        )
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

        let result = connect_and_store(
            &store,
            &provider,
            "openai",
            "https://api.openai.com",
            None,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(store.list().unwrap(), vec![]);
    }

    // ── provider_for (closing the A7 handoff) ──

    #[test]
    fn provider_for_selects_ollama() {
        let provider = provider_for("ollama", "http://localhost:11434", "");
        assert_eq!(provider.provider_name(), OllamaProvider::new("x").provider_name());
    }

    #[test]
    fn provider_for_selects_anthropic() {
        let provider = provider_for("anthropic", "https://api.anthropic.com", "sk-ant");
        assert_eq!(
            provider.provider_name(),
            AnthropicProvider::new("x", "y").provider_name()
        );
    }

    #[test]
    fn provider_for_selects_gemini() {
        let provider = provider_for("gemini", "https://generativelanguage.googleapis.com", "key");
        assert_eq!(
            provider.provider_name(),
            GeminiProvider::new("x", "y").provider_name()
        );
    }

    #[test]
    fn provider_for_falls_back_to_openai_compat_for_openai_and_openai_compat_and_unknown() {
        let openai_compat_name = OpenAiCompatProvider::new("x", "y").provider_name();
        for kind in ["openai", "openai-compat", "some-unknown-kind"] {
            let provider = provider_for(kind, "https://api.openai.com", "sk-test");
            assert_eq!(provider.provider_name(), openai_compat_name, "kind={kind}");
        }
    }

    // ── connection_test ──

    #[tokio::test]
    async fn connection_test_impl_ok_when_provider_is_available() {
        let provider = FakeProvider {
            available: true,
            models: vec![],
        };
        assert!(connection_test_impl(&provider, "https://api.openai.com").await.is_ok());
    }

    #[tokio::test]
    async fn connection_test_impl_returns_a_clear_error_when_unavailable() {
        let provider = FakeProvider {
            available: false,
            models: vec![],
        };
        let err = connection_test_impl(&provider, "https://api.openai.com")
            .await
            .unwrap_err();
        assert!(err.contains("https://api.openai.com"), "got: {err}");
    }

    // ── connection_refresh_models ──

    #[tokio::test]
    async fn refresh_models_persists_discovered_models() {
        let store = new_store();
        let added = store
            .add("openai", "https://api.openai.com", None, &[])
            .unwrap();
        let provider = FakeProvider {
            available: true,
            models: vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()],
        };

        let result = connection_refresh_models_impl(&store, &provider, &added.id)
            .await
            .unwrap();

        let connection = result.expect("discovery should have succeeded");
        assert_eq!(
            connection.available_models,
            vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()]
        );
    }

    #[tokio::test]
    async fn refresh_models_surfaces_manual_entry_required_without_touching_the_store() {
        let store = new_store();
        let added = store
            .add("openai", "https://api.openai.com", None, &[])
            .unwrap();
        let provider = FakeProvider {
            available: true,
            models: vec![],
        };

        let result = connection_refresh_models_impl(&store, &provider, &added.id)
            .await
            .unwrap();

        assert!(result.is_err(), "empty discovery should require manual entry");
        let refreshed = store.get(&added.id).unwrap().unwrap();
        assert_eq!(refreshed.available_models, Vec::<String>::new());
    }

    // ── model_add_manual ──

    #[test]
    fn model_add_manual_adds_the_typed_id() {
        let store = new_store();
        let added = store
            .add("openai", "https://api.openai.com", None, &[])
            .unwrap();

        let connection = model_add_manual_impl(&store, &added.id, "  my-custom-model  ").unwrap();

        assert_eq!(connection.enabled_models, vec!["my-custom-model".to_string()]);
        assert_eq!(connection.available_models, vec!["my-custom-model".to_string()]);
    }

    #[test]
    fn model_add_manual_rejects_a_blank_id() {
        let store = new_store();
        let added = store
            .add("openai", "https://api.openai.com", None, &[])
            .unwrap();

        let err = model_add_manual_impl(&store, &added.id, "   ").unwrap_err();

        assert!(err.contains("empty"), "got: {err}");
        assert_eq!(store.get(&added.id).unwrap().unwrap().enabled_models, Vec::<String>::new());
    }
}
