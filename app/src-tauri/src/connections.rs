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

use crate::secrets::{secrets_get, SecretStore};

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

        // `CREATE TABLE IF NOT EXISTS` above is a no-op against a DB that
        // already has a `connections` table -- notably A7's pre-B7b schema,
        // which only had `id`/`provider_kind`/`base_url`/`enabled_models`.
        // Without this, every B7b query against `api_key`/`available_models`
        // fails with "no such column" on any DB created during Phase A.
        // Migrate such tables in place by adding whichever of B7b's columns
        // are missing; already-migrated (or freshly created) DBs already
        // have both, so this is a no-op for them.
        let existing_columns: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(connections)")?;
            let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
            names.collect::<rusqlite::Result<_>>()?
        };
        if !existing_columns.iter().any(|c| c == "api_key") {
            conn.execute("ALTER TABLE connections ADD COLUMN api_key TEXT", [])?;
        }
        if !existing_columns.iter().any(|c| c == "available_models") {
            conn.execute(
                "ALTER TABLE connections ADD COLUMN available_models TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }
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
///
/// The models `list_models` returns here are also persisted as the
/// connection's `available_models`, not just mined for a default. Without
/// that, a connection added through the first-run chooser (`FirstRun.tsx`,
/// which never calls `connection_refresh_models`) came back with
/// `available_models: []`, so the Connections screen's "Discovered models"
/// checklist showed "No models discovered yet." until the user happened to
/// press "Refresh models" — the one enabled default was curatable nowhere.
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
        .first()
        .cloned()
        .unwrap_or_else(|| FALLBACK_MODEL.to_string());

    let connection = store.add(provider_kind, base_url, api_key, &[default_model])?;
    if models.is_empty() {
        return Ok(connection);
    }
    store.set_available_models(&connection.id, &models)
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
        // The Claude Code login: same Messages API, but its OAuth token
        // authenticates as `Authorization: Bearer` rather than `x-api-key`
        // (the latter is rejected 401). `api_key` here carries that token,
        // resolved fresh from Claude Code's own store at call time rather
        // than copied into ours -- see `claude_code.rs`.
        "claude-code" => Box::new(AnthropicProvider::with_auth(
            base_url,
            llm_provider::AnthropicAuth::OAuth(api_key.to_string()),
        )),
        "anthropic" => Box::new(AnthropicProvider::new(base_url, api_key)),
        "gemini" => Box::new(GeminiProvider::new(base_url, api_key)),
        _ => Box::new(OpenAiCompatProvider::new(base_url, api_key)),
    }
}

/// Resolves the API key to use when building a provider for an already-
/// stored connection (B23 reconciliation): prefers the secure
/// [`SecretStore`] (mirrored there by `connection_add`/`connection_edit`,
/// see [`mirror_api_key`]), falling back to `ConnectionStore`'s own
/// plaintext `api_key` column when secure storage has nothing for this id
/// (e.g. a connection added before this reconciliation, or secure storage
/// itself failing open). Empty (no key either place, e.g. a keyless Ollama
/// endpoint) resolves to `""`, same as `provider_for`'s keyless callers
/// already expect.
pub fn resolve_api_key(
    connections: &ConnectionStore,
    secrets: &SecretStore,
    connection_id: &str,
) -> Result<String> {
    if let Some(key) = secrets_get(secrets, connection_id)? {
        if !key.is_empty() {
            return Ok(key);
        }
    }
    Ok(connections.api_key(connection_id)?.unwrap_or_default())
}

/// Best-effort: mirrors `key` into secure storage for `connection_id`
/// (`Some(non-empty)` -> set, `None`/empty -> clear), so [`resolve_api_key`]
/// can read a real cloud-provider key back later. Logs rather than fails
/// the caller on error -- the plaintext `ConnectionStore::api_key` column is
/// still the connection's durable record either way (`resolve_api_key`'s
/// fallback), so a mirroring hiccup here shouldn't turn a successful
/// add/edit into a failure.
fn mirror_api_key(secrets: &SecretStore, connection_id: &str, key: Option<&str>) {
    let result = match key.filter(|k| !k.is_empty()) {
        Some(key) => secrets.set(connection_id, key),
        None => secrets.delete(connection_id),
    };
    if let Err(e) = result {
        eprintln!("[connections] failed to sync secure-storage key for {connection_id}: {e}");
    }
}

/// Connects and persists a connection (see [`connect_and_store`]), then
/// mirrors a non-empty `api_key` into secure storage (B23 -- see
/// [`mirror_api_key`]) so `resolve_api_key` can hand it back to a real
/// cloud-provider call later. Takes `&dyn LlmProvider` directly (like
/// `connect_and_store`) so the whole "connect, persist, mirror" sequence is
/// unit-testable with a fake, independent of a live HTTP endpoint; the
/// `connection_add` command below is the thin production wrapper that
/// builds the real provider via [`provider_for`].
pub async fn connect_store_and_mirror_key(
    store: &ConnectionStore,
    secrets: &SecretStore,
    provider: &dyn LlmProvider,
    provider_kind: &str,
    base_url: &str,
    api_key: &str,
) -> Result<Connection> {
    let connection =
        connect_and_store(store, provider, provider_kind, base_url, Some(api_key)).await?;
    mirror_api_key(secrets, &connection.id, Some(api_key));
    Ok(connection)
}

/// Tauri command: connects a provider (verifying reachability through the
/// vendor-appropriate `llm-provider` implementation, see [`provider_for`])
/// and persists it with a default enabled model, mirroring a non-empty key
/// into secure storage (see [`connect_store_and_mirror_key`]). Registered by
/// A14/B23 (`app/src-tauri/src/lib.rs`), which manages a `ConnectionStore`/
/// `SecretStore` as state.
#[tauri::command]
pub async fn connection_add(
    state: tauri::State<'_, ConnectionStore>,
    secrets: tauri::State<'_, SecretStore>,
    provider_kind: String,
    base_url: String,
    api_key: String,
) -> Result<Connection, String> {
    let provider = provider_for(&provider_kind, &base_url, &api_key);
    connect_store_and_mirror_key(
        &state,
        &secrets,
        provider.as_ref(),
        &provider_kind,
        &base_url,
        &api_key,
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

/// Edits a connection (via [`connection_edit_impl`]) and, whenever the key
/// actually changed, mirrors the new value into secure storage too (B23 --
/// see [`mirror_api_key`]), keeping it in sync with the plaintext DB column
/// `connection_edit_impl` writes. `api_key: None` means "leave the key
/// unchanged"; `Some(None)` clears it.
///
/// The full logic behind the `connection_edit` command, taking `&SecretStore`
/// directly (rather than `tauri::State`) so it's unit-testable with a
/// directly-constructed store instead of a built `tauri::App` (a real app
/// build initializes the tray-icon plugin, which requires the main thread).
pub fn connection_edit_and_mirror_impl(
    store: &ConnectionStore,
    secrets: &SecretStore,
    id: &str,
    base_url: Option<&str>,
    api_key: Option<Option<&str>>,
    enabled_models: Option<&[String]>,
) -> Result<Connection> {
    let connection = connection_edit_impl(store, id, base_url, api_key, enabled_models)?;
    if let Some(key) = api_key {
        mirror_api_key(secrets, id, key);
    }
    Ok(connection)
}

/// Tauri command wrapping [`connection_edit_and_mirror_impl`].
#[tauri::command]
pub fn connection_edit(
    state: tauri::State<'_, ConnectionStore>,
    secrets: tauri::State<'_, SecretStore>,
    id: String,
    base_url: Option<String>,
    api_key: Option<String>,
    enabled_models: Option<Vec<String>>,
) -> Result<Connection, String> {
    connection_edit_and_mirror_impl(
        &state,
        &secrets,
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

/// Removes a connection (via [`connection_remove_impl`]) and also clears its
/// secure-storage key, if any (B23), so a removed connection doesn't leave
/// an orphaned secret behind. The full logic behind the `connection_remove`
/// command, taking `&SecretStore` directly for the same app-free-testability
/// reason as [`connection_edit_and_mirror_impl`].
pub fn connection_remove_and_clear_impl(
    store: &ConnectionStore,
    secrets: &SecretStore,
    id: &str,
) -> Result<()> {
    connection_remove_impl(store, id)?;
    mirror_api_key(secrets, id, None);
    Ok(())
}

/// Tauri command wrapping [`connection_remove_and_clear_impl`].
#[tauri::command]
pub fn connection_remove(
    state: tauri::State<'_, ConnectionStore>,
    secrets: tauri::State<'_, SecretStore>,
    id: String,
) -> Result<(), String> {
    connection_remove_and_clear_impl(&state, &secrets, &id).map_err(|e| e.to_string())
}

/// Probes reachability without persisting anything -- lets the Connections
/// screen show a pass/fail result before the user commits to saving a
/// connection (or to confirm a saved one is still reachable).
pub async fn connection_test_impl(provider: &dyn LlmProvider, base_url: &str) -> Result<(), String> {
    // `availability` rather than `is_available`: the latter answers only
    // yes/no, so an invalid key, an out-of-credit account and an unreachable
    // host all rendered as the same "could not connect to anthropic at
    // https://api.anthropic.com" — which is what a user hits when they paste
    // a key that doesn't work and have no way to find out why.
    provider.availability().await.map_err(|reason| {
        format!(
            "could not connect to {} at {base_url} — {reason}",
            provider.provider_name()
        )
    })
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
/// connection's stored kind/base URL to rebuild its provider, resolving the
/// key via [`resolve_api_key`] (B23: secure storage first, the plaintext DB
/// column as fallback) so a cloud connection's refresh actually
/// authenticates. Returns `Err` both when the connection is unknown and
/// when discovery degrades to manual entry -- the frontend distinguishes
/// the latter by falling back to the manual-id control (`model_add_manual`),
/// per B7's plan.
#[tauri::command]
pub async fn connection_refresh_models(
    state: tauri::State<'_, ConnectionStore>,
    secrets: tauri::State<'_, SecretStore>,
    id: String,
) -> Result<Connection, String> {
    let existing = state
        .get(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no connection with id {id}"))?;
    let api_key = resolve_api_key(&state, &secrets, &id).map_err(|e| e.to_string())?;
    let provider = provider_for(&existing.provider_kind, &existing.base_url, &api_key);
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

    /// Regression: `connect_and_store` used to mine `list_models` for a
    /// default and throw the rest away, leaving `available_models` empty. A
    /// connection added from the first-run chooser (which never calls
    /// `connection_refresh_models`) therefore had nothing for the Connections
    /// screen's "Discovered models" checklist to show -- it read "No models
    /// discovered yet." with no way to enable anything.
    #[tokio::test]
    async fn connect_and_store_persists_every_discovered_model_as_available() {
        let store = new_store();
        let provider = FakeProvider {
            available: true,
            models: vec!["qwen3-coder:latest".to_string(), "qwen3:32b".to_string()],
        };

        let connection =
            connect_and_store(&store, &provider, "ollama", "http://localhost:11434", None)
                .await
                .unwrap();

        assert_eq!(
            connection.available_models,
            vec!["qwen3-coder:latest".to_string(), "qwen3:32b".to_string()]
        );
        // Still exactly one model enabled by default, the first discovered.
        assert_eq!(
            connection.enabled_models,
            vec!["qwen3-coder:latest".to_string()]
        );
        // And it's persisted, not just in the returned value.
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
        // Nothing discovered means nothing to offer for curation -- the
        // manual model-id fallback covers this case.
        assert!(connection.available_models.is_empty());
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

    // ── resolve_api_key / mirror_api_key (B23 reconciliation) ──

    /// Minimal RAII temp-dir guard for a `SecretStore` (mirrors
    /// `TempDbPath` above / `secrets.rs`'s own `TempDir`).
    struct TempSecretsDir(std::path::PathBuf);

    impl TempSecretsDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "redrafter_connections_secrets_{label}_{}_{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            Self(path)
        }
    }

    impl Drop for TempSecretsDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn new_secret_store(label: &str) -> (SecretStore, TempSecretsDir) {
        let dir = TempSecretsDir::new(label);
        let store = SecretStore::open(&dir.0).expect("failed to open secret store");
        (store, dir)
    }

    #[test]
    fn resolve_api_key_prefers_secure_storage_over_the_plaintext_db_column() {
        let store = new_store();
        let (secrets, _dir) = new_secret_store("prefers-secure");
        let added = store
            .add("openai", "https://api.openai.com", Some("sk-db"), &[])
            .unwrap();
        secrets.set(&added.id, "sk-secure").unwrap();

        assert_eq!(
            resolve_api_key(&store, &secrets, &added.id).unwrap(),
            "sk-secure"
        );
    }

    #[test]
    fn resolve_api_key_falls_back_to_the_db_column_when_secure_storage_has_nothing() {
        let store = new_store();
        let (secrets, _dir) = new_secret_store("falls-back");
        let added = store
            .add("openai", "https://api.openai.com", Some("sk-db-only"), &[])
            .unwrap();

        assert_eq!(
            resolve_api_key(&store, &secrets, &added.id).unwrap(),
            "sk-db-only"
        );
    }

    #[test]
    fn resolve_api_key_is_empty_for_a_keyless_connection() {
        let store = new_store();
        let (secrets, _dir) = new_secret_store("keyless");
        let added = store
            .add("ollama", "http://localhost:11434", None, &[])
            .unwrap();

        assert_eq!(resolve_api_key(&store, &secrets, &added.id).unwrap(), "");
    }

    #[test]
    fn mirror_api_key_sets_a_non_empty_key_and_clears_a_missing_one() {
        let (secrets, _dir) = new_secret_store("mirror");

        mirror_api_key(&secrets, "conn-1", Some("sk-mirrored"));
        assert_eq!(secrets.get("conn-1").unwrap(), Some("sk-mirrored".to_string()));

        mirror_api_key(&secrets, "conn-1", None);
        assert_eq!(secrets.get("conn-1").unwrap(), None);
    }

    // ── connection_add / connection_edit / connection_remove / \
    //    connection_refresh_models command wrappers mirror into \
    //    SecretStore (B23) ──
    //
    // `connection_edit`/`connection_remove`'s full logic (persisted edit/
    // removal plus the secure-storage mirror) lives in
    // `connection_edit_and_mirror_impl`/`connection_remove_and_clear_impl`,
    // exercised directly below against directly-constructed stores -- no
    // `tauri::App`/`State` needed (a real app build initializes the
    // tray-icon plugin, which requires the main thread).

    #[tokio::test]
    async fn connect_store_and_mirror_key_persists_the_key_into_secure_storage() {
        let store = new_store();
        let (secrets, _dir) = new_secret_store("connect-mirror");
        let provider = FakeProvider {
            available: true,
            models: vec!["gpt-4o-mini".to_string()],
        };

        let connection = connect_store_and_mirror_key(
            &store,
            &secrets,
            &provider,
            "openai",
            "https://api.openai.com",
            "sk-add",
        )
        .await
        .unwrap();

        assert_eq!(
            secrets.get(&connection.id).unwrap(),
            Some("sk-add".to_string())
        );
        // The plaintext DB column is still written too (the fallback
        // `resolve_api_key` relies on).
        assert_eq!(
            store.api_key(&connection.id).unwrap(),
            Some("sk-add".to_string())
        );
    }

    #[tokio::test]
    async fn connect_store_and_mirror_key_mirrors_nothing_for_a_keyless_connection() {
        let store = new_store();
        let (secrets, _dir) = new_secret_store("connect-mirror-keyless");
        let provider = FakeProvider {
            available: true,
            models: vec![],
        };

        let connection = connect_store_and_mirror_key(
            &store,
            &secrets,
            &provider,
            "ollama",
            "http://localhost:11434",
            "",
        )
        .await
        .unwrap();

        assert_eq!(secrets.get(&connection.id).unwrap(), None);
    }

    #[test]
    fn connection_edit_and_mirror_impl_mirrors_a_changed_key_but_leaves_it_when_unspecified() {
        let store = new_store();
        let (secrets, _dir) = new_secret_store("cmd-edit");
        let added = store
            .add("openai", "https://api.openai.com", Some("sk-old"), &[])
            .unwrap();
        secrets.set(&added.id, "sk-old").unwrap();

        // Editing without an `api_key` at all must not touch secure storage.
        connection_edit_and_mirror_impl(
            &store,
            &secrets,
            &added.id,
            Some("https://api.example.com"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(secrets.get(&added.id).unwrap(), Some("sk-old".to_string()));

        // An explicit new key mirrors into secure storage too.
        connection_edit_and_mirror_impl(
            &store,
            &secrets,
            &added.id,
            None,
            Some(Some("sk-new")),
            None,
        )
        .unwrap();
        assert_eq!(secrets.get(&added.id).unwrap(), Some("sk-new".to_string()));
    }

    #[test]
    fn connection_remove_and_clear_impl_clears_the_secure_storage_key() {
        let store = new_store();
        let (secrets, _dir) = new_secret_store("cmd-remove");
        let added = store
            .add("openai", "https://api.openai.com", Some("sk-doomed"), &[])
            .unwrap();
        secrets.set(&added.id, "sk-doomed").unwrap();

        connection_remove_and_clear_impl(&store, &secrets, &added.id).unwrap();

        assert_eq!(secrets.get(&added.id).unwrap(), None);
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

    // ── schema migration (A7's 4-column DB -> B7b's 6-column DB) ──

    /// Regression test for the upgrade path: A7 shipped `connections.sqlite3`
    /// with only 4 columns (id, provider_kind, base_url, enabled_models).
    /// `CREATE TABLE IF NOT EXISTS` is a no-op against that pre-existing
    /// table, so opening a store on an A7-era DB must not simply skip
    /// schema setup -- it must add the missing `api_key`/`available_models`
    /// columns via migration.
    /// Minimal RAII temp-file guard (no `tempfile` crate dependency): picks
    /// a process- and test-unique path under the OS temp dir and removes it
    /// (plus SQLite's `-wal`/`-shm` siblings) on drop.
    struct TempDbPath(std::path::PathBuf);

    impl TempDbPath {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "redrafter_connections_{label}_{}_{:?}.sqlite3",
                std::process::id(),
                std::thread::current().id()
            ));
            Self(path)
        }
    }

    impl Drop for TempDbPath {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
            }
        }
    }

    #[test]
    fn open_migrates_an_a7_era_four_column_db_to_add_the_missing_columns() {
        let temp = TempDbPath::new("migration");
        let path = &temp.0;

        // Simulate the pre-upgrade (A7) DB: create the old 4-column table
        // directly and seed it with a row, exactly as A7's `init_schema`
        // and `add` did (see commit 3316da3).
        {
            let raw = SqliteConnection::open(path).unwrap();
            raw.execute(
                "CREATE TABLE IF NOT EXISTS connections (
                    id             INTEGER PRIMARY KEY AUTOINCREMENT,
                    provider_kind  TEXT NOT NULL,
                    base_url       TEXT NOT NULL,
                    enabled_models TEXT NOT NULL
                )",
                [],
            )
            .unwrap();
            raw.execute(
                "INSERT INTO connections (provider_kind, base_url, enabled_models)
                 VALUES ('ollama', 'http://localhost:11434', '[\"llama3\"]')",
                [],
            )
            .unwrap();
        }

        // Now open it through B7b's store, triggering migration.
        let store = ConnectionStore::open(path).expect("open should migrate, not fail");

        let listed = store.list().expect("list should succeed on a migrated DB");
        assert_eq!(listed.len(), 1);
        let seeded = &listed[0];
        assert_eq!(seeded.provider_kind, "ollama");
        assert_eq!(seeded.base_url, "http://localhost:11434");
        assert_eq!(seeded.enabled_models, vec!["llama3".to_string()]);
        assert_eq!(seeded.available_models, Vec::<String>::new());
        assert_eq!(seeded.key_ref, None);

        let fetched = store
            .get(&seeded.id)
            .expect("get should succeed on a migrated DB")
            .expect("seeded row should exist");
        assert_eq!(fetched, *seeded);
        assert_eq!(store.api_key(&seeded.id).unwrap(), None);
    }

    #[test]
    fn open_on_a_fresh_file_backed_db_still_works() {
        let temp = TempDbPath::new("fresh");
        let path = &temp.0;

        let store = ConnectionStore::open(path).expect("open should succeed on a fresh DB");
        let added = store
            .add("openai", "https://api.openai.com", Some("sk-test"), &["gpt-4o".to_string()])
            .unwrap();

        assert_eq!(store.list().unwrap(), vec![added]);
    }
}
