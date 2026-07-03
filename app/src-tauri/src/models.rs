//! The curated cross-connection model layer (B8): aggregates every
//! connection's *enabled* models into one flat, provider-agnostic list (the
//! Models screen's single source of truth), lets the user pick the one
//! global **active** model that `refine` uses, disable a model from the
//! enabled set, and star a favorite (surfaced in the tray quick-switch,
//! B9/B20).
//!
//! Built on top of `connections.rs`'s `ConnectionStore` (B7b) -- this module
//! owns no storage of its own beyond two keys in `settings.rs`'s
//! `SettingsStore`: the active-model reference and the favorite set. Neither
//! duplicates `ConnectionStore`'s `enabled_models`/`available_models` --
//! curation (what's enabled) still lives there; this module only adds the
//! "which one is active" and "which are favorited" layers on top, plus the
//! `ollama_pull` command that makes a freshly pulled model available to
//! curate (closing B1's `OllamaProvider::pull` off to the UI).
//!
//! (Plain `//` module doc rather than `//!` for every item below, matching
//! `connections.rs`/`settings.rs`'s convention of `include!`-ing this file's
//! body into `tests/core_test.rs`'s inline `mod` block.)

use serde::{Deserialize, Serialize};

use crate::connections::ConnectionStore;
use crate::settings::SettingsStore;
use llm_provider::ollama::PullProgress;
use llm_provider::OllamaProvider;

/// Settings key backing the persisted active-model reference. Its value is
/// a JSON-encoded [`ActiveModelRef`]; nothing outside this module parses it
/// apart, so the encoding is an implementation detail.
const ACTIVE_MODEL_KEY: &str = "active_model";
/// Settings key backing the favorited-model set: a JSON array of the same
/// opaque key [`model_key`] produces.
const FAVORITE_MODELS_KEY: &str = "model_favorites";

/// Points at one specific (connection, model) pair -- the unit `active` and
/// `favorite` both key off of, since the same model id can be enabled on
/// more than one connection (e.g. two Ollama endpoints).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ActiveModelRef {
    connection_id: String,
    model_id: String,
}

/// One row of the Models screen's curated table: an enabled model, which
/// connection it came from, and its active/favorite state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CuratedModel {
    pub connection_id: String,
    pub model_id: String,
    pub provider_kind: String,
    pub active: bool,
    pub favorite: bool,
}

/// The full response every model-curation command resolves with, so the
/// frontend never has to make a second round trip to see the effect of an
/// action (mirrors `connections.rs`'s pattern of every mutating command
/// returning the refreshed `Connection`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsListResult {
    pub models: Vec<CuratedModel>,
    /// Whether some model in `models` is currently the active one.
    pub has_active: bool,
    /// True when an active model *was* chosen but no longer appears in
    /// `models` (its connection was removed, or the model itself was
    /// disabled) -- the Models screen's "active model unavailable" banner
    /// (S26, B21) and General/tray's routing (S19) both key off this.
    pub active_unavailable: bool,
    /// The stale active model's id, for the unavailable banner's copy
    /// ("Your active model (<id>) is no longer available"). Only set
    /// alongside `active_unavailable`.
    pub stale_active_model_id: Option<String>,
}

/// The opaque per-(connection, model) key used for both the active
/// reference and the favorite set -- never parsed apart, just compared for
/// equality, so it's safe even when a model id itself contains punctuation
/// (e.g. Ollama's `qwen3:8b`).
fn model_key(connection_id: &str, model_id: &str) -> String {
    // A 2-tuple JSON array round-trips both fields losslessly regardless of
    // what characters `model_id` contains, without needing a delimiter that
    // could collide with the id itself.
    serde_json::to_string(&(connection_id, model_id)).unwrap_or_default()
}

fn load_active_ref(settings: &SettingsStore) -> anyhow::Result<Option<ActiveModelRef>> {
    match settings.get(ACTIVE_MODEL_KEY)? {
        Some(raw) => Ok(serde_json::from_str(&raw).ok()),
        None => Ok(None),
    }
}

fn save_active_ref(settings: &SettingsStore, reference: &ActiveModelRef) -> anyhow::Result<()> {
    settings.set(ACTIVE_MODEL_KEY, &serde_json::to_string(reference)?)
}

/// Resolves the persisted active-model reference (set by
/// [`model_set_active_impl`]/`tray::tray_set_active_model`) into a plain
/// `(connection_id, model_id)` pair for the refine command layer
/// (`lib.rs`'s `active_provider`), or `None` when the user has never picked
/// one. Exposed -- unlike the private [`ActiveModelRef`]/[`load_active_ref`]
/// -- so `active_provider` can honor the user's active-model choice without
/// duplicating the settings-key/JSON encoding this module owns.
pub fn active_model_ref(settings: &SettingsStore) -> anyhow::Result<Option<(String, String)>> {
    Ok(load_active_ref(settings)?.map(|r| (r.connection_id, r.model_id)))
}

fn load_favorites(settings: &SettingsStore) -> anyhow::Result<std::collections::HashSet<String>> {
    match settings.get(FAVORITE_MODELS_KEY)? {
        Some(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
        None => Ok(Default::default()),
    }
}

fn save_favorites(
    settings: &SettingsStore,
    favorites: &std::collections::HashSet<String>,
) -> anyhow::Result<()> {
    let list: Vec<&String> = favorites.iter().collect();
    settings.set(FAVORITE_MODELS_KEY, &serde_json::to_string(&list)?)
}

/// Builds the curated cross-connection model list: every enabled model on
/// every stored connection, with its active/favorite state layered on top.
pub fn models_list_impl(
    connections: &ConnectionStore,
    settings: &SettingsStore,
) -> anyhow::Result<ModelsListResult> {
    let active_ref = load_active_ref(settings)?;
    let favorites = load_favorites(settings)?;

    let mut models = Vec::new();
    for connection in connections.list()? {
        for model_id in &connection.enabled_models {
            let is_active = active_ref
                .as_ref()
                .map(|r| r.connection_id == connection.id && r.model_id == *model_id)
                .unwrap_or(false);
            let is_favorite = favorites.contains(&model_key(&connection.id, model_id));
            models.push(CuratedModel {
                connection_id: connection.id.clone(),
                model_id: model_id.clone(),
                provider_kind: connection.provider_kind.clone(),
                active: is_active,
                favorite: is_favorite,
            });
        }
    }

    let has_active = models.iter().any(|m| m.active);
    let active_unavailable = active_ref.is_some() && !has_active;
    let stale_active_model_id = if active_unavailable {
        active_ref.map(|r| r.model_id)
    } else {
        None
    };

    Ok(ModelsListResult {
        models,
        has_active,
        active_unavailable,
        stale_active_model_id,
    })
}

/// Tauri command wrapping [`models_list_impl`]. Registered by A14/B23
/// (`app/src-tauri/src/lib.rs`), which manages `ConnectionStore`/
/// `SettingsStore` as state.
#[tauri::command]
pub fn models_list(
    connections: tauri::State<'_, ConnectionStore>,
    settings: tauri::State<'_, SettingsStore>,
) -> Result<ModelsListResult, String> {
    models_list_impl(&connections, &settings).map_err(|e| e.to_string())
}

/// Sets the single global active model, guarded so it's always one of the
/// connection's *enabled* models -- attempting to activate a model that
/// isn't currently enabled (never enabled, or disabled since) is rejected
/// rather than silently accepted. (Setting an *already*-active model
/// unavailable later, e.g. by disabling it, is allowed to happen -- that's
/// the "active unavailable" state `model_disable_impl` deliberately leaves
/// in place for B21's banner, not a guard this function needs to enforce.)
pub fn model_set_active_impl(
    connections: &ConnectionStore,
    settings: &SettingsStore,
    connection_id: &str,
    model_id: &str,
) -> Result<ModelsListResult, String> {
    let connection = connections
        .get(connection_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no connection with id {connection_id}"))?;

    if !connection.enabled_models.iter().any(|m| m == model_id) {
        return Err(format!(
            "model {model_id} is not enabled on connection {connection_id}"
        ));
    }

    let reference = ActiveModelRef {
        connection_id: connection_id.to_string(),
        model_id: model_id.to_string(),
    };
    save_active_ref(settings, &reference).map_err(|e| e.to_string())?;

    models_list_impl(connections, settings).map_err(|e| e.to_string())
}

/// Tauri command wrapping [`model_set_active_impl`].
#[tauri::command]
pub fn model_set_active(
    connections: tauri::State<'_, ConnectionStore>,
    settings: tauri::State<'_, SettingsStore>,
    connection_id: String,
    model_id: String,
) -> Result<ModelsListResult, String> {
    model_set_active_impl(&connections, &settings, &connection_id, &model_id)
}

/// Removes `model_id` from `connection_id`'s enabled set -- the Models
/// screen's per-row disable button. Deliberately does *not* clear the
/// active-model reference even if this was the active model: leaving it
/// stale is what lets [`models_list_impl`] compute `active_unavailable` (the
/// disable + "active unavailable" flow is B21/S26, driven entirely off this
/// module's already-built surface).
pub fn model_disable_impl(
    connections: &ConnectionStore,
    settings: &SettingsStore,
    connection_id: &str,
    model_id: &str,
) -> Result<ModelsListResult, String> {
    let connection = connections
        .get(connection_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no connection with id {connection_id}"))?;

    let remaining: Vec<String> = connection
        .enabled_models
        .iter()
        .filter(|m| *m != model_id)
        .cloned()
        .collect();

    connections
        .edit(connection_id, None, None, Some(&remaining))
        .map_err(|e| e.to_string())?;

    models_list_impl(connections, settings).map_err(|e| e.to_string())
}

/// Tauri command wrapping [`model_disable_impl`].
#[tauri::command]
pub fn model_disable(
    connections: tauri::State<'_, ConnectionStore>,
    settings: tauri::State<'_, SettingsStore>,
    connection_id: String,
    model_id: String,
) -> Result<ModelsListResult, String> {
    model_disable_impl(&connections, &settings, &connection_id, &model_id)
}

/// Toggles favorite status for a (connection, model) pair -- favorites
/// surface in the tray's quick-switch (B9/B20), independent of which model
/// is active.
pub fn model_toggle_favorite_impl(
    connections: &ConnectionStore,
    settings: &SettingsStore,
    connection_id: &str,
    model_id: &str,
) -> Result<ModelsListResult, String> {
    let mut favorites = load_favorites(settings).map_err(|e| e.to_string())?;
    let key = model_key(connection_id, model_id);
    if !favorites.remove(&key) {
        favorites.insert(key);
    }
    save_favorites(settings, &favorites).map_err(|e| e.to_string())?;

    models_list_impl(connections, settings).map_err(|e| e.to_string())
}

/// Tauri command wrapping [`model_toggle_favorite_impl`].
#[tauri::command]
pub fn model_toggle_favorite(
    connections: tauri::State<'_, ConnectionStore>,
    settings: tauri::State<'_, SettingsStore>,
    connection_id: String,
    model_id: String,
) -> Result<ModelsListResult, String> {
    model_toggle_favorite_impl(&connections, &settings, &connection_id, &model_id)
}

/// Drains a pull's progress stream down to its terminal line: the
/// `status: "success"` line on success, or the first line carrying an
/// `error`. Split out from [`ollama_pull_impl`] so it's unit-testable
/// against a synthetic channel, without a live Ollama server (mirrors
/// `crates/llm-provider`'s own NDJSON-parsing tests, one layer up).
async fn drain_pull_progress(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<PullProgress>,
) -> Result<PullProgress, String> {
    let mut last = PullProgress {
        status: String::new(),
        digest: None,
        total: None,
        completed: None,
        error: None,
    };
    while let Some(progress) = rx.recv().await {
        if let Some(error) = &progress.error {
            return Err(error.clone());
        }
        let done = progress.is_done();
        last = progress;
        if done {
            break;
        }
    }
    Ok(last)
}

/// Pulls `model_id` from the first stored Ollama connection, waiting for the
/// download to finish (or fail) before returning -- the "Get more Ollama
/// models" control on the Models screen (B22/S27 drives it end to end
/// against this already-built surface). On success, the model is added to
/// that connection's *available* list (not auto-enabled -- curating it onto
/// the enabled set is a separate, explicit step, same as any other
/// discovered model).
pub async fn ollama_pull_impl(
    connections: &ConnectionStore,
    model_id: &str,
) -> Result<PullProgress, String> {
    let stored = connections.list().map_err(|e| e.to_string())?;
    let connection = stored
        .into_iter()
        .find(|c| c.provider_kind == "ollama")
        .ok_or_else(|| "no Ollama connection configured".to_string())?;

    let provider = OllamaProvider::new(&connection.base_url);
    let rx = provider
        .pull(model_id, tokio_util::sync::CancellationToken::new())
        .await
        .map_err(|e| e.to_string())?;

    let result = drain_pull_progress(rx).await?;

    if result.is_done() && !connection.available_models.iter().any(|m| m == model_id) {
        let mut available = connection.available_models.clone();
        available.push(model_id.to_string());
        // Best-effort: the pull itself already succeeded, so a failure to
        // persist the newly-available model shouldn't turn into a user-
        // facing pull error.
        let _ = connections.set_available_models(&connection.id, &available);
    }

    Ok(result)
}

/// Tauri command wrapping [`ollama_pull_impl`].
#[tauri::command]
pub async fn ollama_pull(
    connections: tauri::State<'_, ConnectionStore>,
    model_id: String,
) -> Result<PullProgress, String> {
    ollama_pull_impl(&connections, &model_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stores() -> (ConnectionStore, SettingsStore) {
        (
            ConnectionStore::open_in_memory().expect("failed to open in-memory connections"),
            SettingsStore::open_in_memory().expect("failed to open in-memory settings"),
        )
    }

    // ── models_list ──

    #[test]
    fn models_list_is_empty_for_fresh_stores() {
        let (connections, settings) = stores();
        let result = models_list_impl(&connections, &settings).unwrap();
        assert_eq!(result.models, vec![]);
        assert!(!result.has_active);
        assert!(!result.active_unavailable);
        assert_eq!(result.stale_active_model_id, None);
    }

    #[test]
    fn models_list_aggregates_enabled_models_across_connections() {
        let (connections, settings) = stores();
        connections
            .add("anthropic", "https://api.anthropic.com", Some("sk"), &["claude-opus-4-6".to_string()])
            .unwrap();
        connections
            .add("ollama", "http://localhost:11434", None, &["llama3.1:8b".to_string(), "qwen3:8b".to_string()])
            .unwrap();

        let result = models_list_impl(&connections, &settings).unwrap();
        let ids: Vec<&str> = result.models.iter().map(|m| m.model_id.as_str()).collect();
        assert_eq!(ids, vec!["claude-opus-4-6", "llama3.1:8b", "qwen3:8b"]);
        assert!(result.models.iter().all(|m| !m.active && !m.favorite));
    }

    // ── model_set_active ──

    #[test]
    fn set_active_marks_exactly_that_model_active() {
        let (connections, settings) = stores();
        let anthropic = connections
            .add("anthropic", "https://api.anthropic.com", Some("sk"), &["claude-opus-4-6".to_string()])
            .unwrap();
        connections
            .add("ollama", "http://localhost:11434", None, &["llama3.1:8b".to_string()])
            .unwrap();

        let result = model_set_active_impl(&connections, &settings, &anthropic.id, "claude-opus-4-6").unwrap();

        assert!(result.has_active);
        assert!(!result.active_unavailable);
        let active: Vec<&CuratedModel> = result.models.iter().filter(|m| m.active).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].model_id, "claude-opus-4-6");
    }

    #[test]
    fn set_active_rejects_a_model_that_isnt_enabled() {
        let (connections, settings) = stores();
        let anthropic = connections
            .add("anthropic", "https://api.anthropic.com", Some("sk"), &[])
            .unwrap();

        let err = model_set_active_impl(&connections, &settings, &anthropic.id, "claude-opus-4-6").unwrap_err();
        assert!(err.contains("not enabled"), "got: {err}");

        // Nothing was persisted: models_list still shows no active model.
        let result = models_list_impl(&connections, &settings).unwrap();
        assert!(!result.has_active);
        assert!(!result.active_unavailable);
    }

    #[test]
    fn set_active_rejects_an_unknown_connection() {
        let (connections, settings) = stores();
        let err = model_set_active_impl(&connections, &settings, "999", "claude-opus-4-6").unwrap_err();
        assert!(err.contains("999"), "got: {err}");
    }

    #[test]
    fn setting_a_new_active_model_replaces_the_previous_one() {
        let (connections, settings) = stores();
        let anthropic = connections
            .add(
                "anthropic",
                "https://api.anthropic.com",
                Some("sk"),
                &["claude-opus-4-6".to_string(), "claude-sonnet-4-6".to_string()],
            )
            .unwrap();

        model_set_active_impl(&connections, &settings, &anthropic.id, "claude-opus-4-6").unwrap();
        let result =
            model_set_active_impl(&connections, &settings, &anthropic.id, "claude-sonnet-4-6").unwrap();

        let active: Vec<&str> = result
            .models
            .iter()
            .filter(|m| m.active)
            .map(|m| m.model_id.as_str())
            .collect();
        assert_eq!(active, vec!["claude-sonnet-4-6"]);
    }

    // ── model_disable ──

    #[test]
    fn disable_removes_the_model_from_the_enabled_set() {
        let (connections, settings) = stores();
        let anthropic = connections
            .add(
                "anthropic",
                "https://api.anthropic.com",
                Some("sk"),
                &["claude-opus-4-6".to_string(), "claude-sonnet-4-6".to_string()],
            )
            .unwrap();

        let result = model_disable_impl(&connections, &settings, &anthropic.id, "claude-opus-4-6").unwrap();

        let ids: Vec<&str> = result.models.iter().map(|m| m.model_id.as_str()).collect();
        assert_eq!(ids, vec!["claude-sonnet-4-6"]);
    }

    #[test]
    fn disabling_the_active_model_leaves_it_stale_so_active_unavailable_is_reported() {
        let (connections, settings) = stores();
        let anthropic = connections
            .add("anthropic", "https://api.anthropic.com", Some("sk"), &["claude-opus-4-6".to_string()])
            .unwrap();
        model_set_active_impl(&connections, &settings, &anthropic.id, "claude-opus-4-6").unwrap();

        let result = model_disable_impl(&connections, &settings, &anthropic.id, "claude-opus-4-6").unwrap();

        assert!(!result.has_active);
        assert!(result.active_unavailable);
        assert_eq!(result.stale_active_model_id.as_deref(), Some("claude-opus-4-6"));
    }

    #[test]
    fn disable_of_an_unknown_connection_errors() {
        let (connections, settings) = stores();
        let err = model_disable_impl(&connections, &settings, "999", "claude-opus-4-6").unwrap_err();
        assert!(err.contains("999"), "got: {err}");
    }

    // ── model_toggle_favorite ──

    #[test]
    fn toggle_favorite_flips_the_flag_on_and_off() {
        let (connections, settings) = stores();
        let anthropic = connections
            .add("anthropic", "https://api.anthropic.com", Some("sk"), &["claude-opus-4-6".to_string()])
            .unwrap();

        let starred = model_toggle_favorite_impl(&connections, &settings, &anthropic.id, "claude-opus-4-6").unwrap();
        assert!(starred.models[0].favorite);

        let unstarred = model_toggle_favorite_impl(&connections, &settings, &anthropic.id, "claude-opus-4-6").unwrap();
        assert!(!unstarred.models[0].favorite);
    }

    #[test]
    fn favorite_is_scoped_to_its_own_connection_and_model_pair() {
        let (connections, settings) = stores();
        let anthropic = connections
            .add("anthropic", "https://api.anthropic.com", Some("sk"), &["shared-name".to_string()])
            .unwrap();
        let ollama = connections
            .add("ollama", "http://localhost:11434", None, &["shared-name".to_string()])
            .unwrap();

        model_toggle_favorite_impl(&connections, &settings, &anthropic.id, "shared-name").unwrap();
        let result = models_list_impl(&connections, &settings).unwrap();

        let anthropic_row = result.models.iter().find(|m| m.connection_id == anthropic.id).unwrap();
        let ollama_row = result.models.iter().find(|m| m.connection_id == ollama.id).unwrap();
        assert!(anthropic_row.favorite);
        assert!(!ollama_row.favorite);
    }

    // ── ollama_pull ──

    #[tokio::test]
    async fn ollama_pull_impl_errors_without_an_ollama_connection() {
        let (connections, _settings) = stores();
        connections
            .add("anthropic", "https://api.anthropic.com", Some("sk"), &[])
            .unwrap();

        let err = ollama_pull_impl(&connections, "llama3.2").await.unwrap_err();
        assert!(err.contains("Ollama"), "got: {err}");
    }

    #[tokio::test]
    async fn drain_pull_progress_returns_the_final_success_line() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(PullProgress {
            status: "pulling manifest".to_string(),
            digest: None,
            total: Some(100),
            completed: Some(50),
            error: None,
        })
        .unwrap();
        tx.send(PullProgress {
            status: "success".to_string(),
            digest: None,
            total: None,
            completed: None,
            error: None,
        })
        .unwrap();
        drop(tx);

        let result = drain_pull_progress(rx).await.unwrap();
        assert_eq!(result.status, "success");
        assert!(result.is_done());
    }

    #[tokio::test]
    async fn drain_pull_progress_surfaces_an_error_line() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(PullProgress {
            status: String::new(),
            digest: None,
            total: None,
            completed: None,
            error: Some("disk full".to_string()),
        })
        .unwrap();
        drop(tx);

        let err = drain_pull_progress(rx).await.unwrap_err();
        assert_eq!(err, "disk full");
    }
}
