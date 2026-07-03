// History store: records every completed refine (original, refined text,
// the model used, when, and — if present — the inline command/preset that
// drove it) so the History screen (`app/src/screens/History.tsx`,
// `controls/history.json`, `wireframes/history.html`) can list, restore, and
// re-refine past results. Same SQLite-backed pattern as `connections.rs`/
// `settings.rs`.
//
// C4 built the store itself plus `history_list`/`history_get`/
// `history_restore`/`history_rerefine` and the `append` fn every successful
// refine should call. C12-C15 (this file's remaining commands) add
// `history_copy`/`history_clear`, backing the History screen's copy and
// clear-all controls (search and the detail view reuse the already-loaded
// list, no new commands needed). C17 is the one that actually calls
// `append` (and, right after, `prune`) from `lib.rs`'s `execute_refine` (so
// every real refine gets recorded and the store never grows past the
// Behavior screen's configured retention cap) and registers every command
// here in the invoke handler/ACL.

use anyhow::Result;
use rusqlite::{Connection as SqliteConnection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use llm_provider::LlmProvider;
use tokio_util::sync::CancellationToken;

use crate::connections::{provider_for, resolve_api_key, Connection, ConnectionStore};
use crate::orchestrator::{Orchestrator, SystemTextIo, TextCapture, TextInjector};
use crate::prompt_builder::BuildOptions;
use crate::secrets::SecretStore;
use crate::settings::SettingsStore;

/// Milliseconds in a day, for converting the Behavior screen's
/// `history.retention_days` (whole days) into a `created_at` cutoff.
const MILLIS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

/// Settings key for the max number of entries to keep (`Behavior.tsx`'s
/// `retention-count` control, C5/C11) -- a plain numeric string (e.g.
/// `"50"`), or the non-numeric `"Unlimited"` for "never prune by count".
const RETENTION_COUNT_KEY: &str = "history.retention_count";
/// Settings key for the max age (in whole days) to keep an entry
/// (`Behavior.tsx`'s `retention-days` control, C5/C11) -- `"0"` is the
/// screen's "session only" option (see [`prune`]'s doc comment for how a
/// per-append prune approximates that without a dedicated app-quit hook).
const RETENTION_DAYS_KEY: &str = "history.retention_days";

/// One recorded refine: the original selection, the model's output, which
/// model produced it, when, and (if the original carried one) the inline
/// command/preset trigger that drove it -- e.g. `/formal` or `/rd`.
///
/// `rename_all = "camelCase"` only affects the JSON wire format the
/// frontend's `app/src/lib/ipc.ts` sees; the Rust field names stay
/// snake_case per convention (mirrors `connections.rs`'s `Connection`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub original: String,
    pub refined: String,
    pub model: String,
    /// Unix epoch milliseconds.
    pub created_at: i64,
    #[serde(default)]
    pub command: Option<String>,
}

pub struct HistoryStore {
    conn: Mutex<SqliteConnection>,
}

impl HistoryStore {
    /// Opens a file-backed SQLite database at the given path, creating the
    /// history table if this is the first run.
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
            "CREATE TABLE IF NOT EXISTS history (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                original   TEXT NOT NULL,
                refined    TEXT NOT NULL,
                model      TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                command    TEXT
            )",
            [],
        )?;
        Ok(())
    }

    /// Records a completed refine, returning the stored entry (including
    /// its generated id and a `created_at` timestamp taken at insert time).
    pub fn append(
        &self,
        original: &str,
        refined: &str,
        model: &str,
        command: Option<&str>,
    ) -> Result<HistoryEntry> {
        self.append_with_timestamp(original, refined, model, command, now_millis())
    }

    /// [`HistoryStore::append`]'s actual body, taking an explicit timestamp
    /// so tests can assert ordering/values deterministically instead of
    /// racing the wall clock.
    fn append_with_timestamp(
        &self,
        original: &str,
        refined: &str,
        model: &str,
        command: Option<&str>,
        created_at: i64,
    ) -> Result<HistoryEntry> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO history (original, refined, model, created_at, command)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (original, refined, model, created_at, command),
        )?;
        let id = conn.last_insert_rowid();
        Ok(HistoryEntry {
            id: id.to_string(),
            original: original.to_string(),
            refined: refined.to_string(),
            model: model.to_string(),
            created_at,
            command: command.map(|s| s.to_string()),
        })
    }

    /// Returns every recorded entry, most recent first.
    pub fn list(&self) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, original, refined, model, created_at, command
             FROM history ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], Self::row_to_entry)?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Returns a single entry by id, or `None` if it doesn't exist.
    pub fn get(&self, id: &str) -> Result<Option<HistoryEntry>> {
        let row_id: i64 = id.parse()?;
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, original, refined, model, created_at, command
             FROM history WHERE id = ?1",
            [row_id],
            Self::row_to_entry,
        )
        .optional()
        .map_err(Into::into)
    }

    /// Deletes every recorded entry. Backs the History screen's clear-all
    /// control (C12-C15) — irreversible, so the frontend gates this behind
    /// a confirmation dialog before calling it.
    pub fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM history", [])?;
        Ok(())
    }

    /// Deletes every entry except the `keep` most recent (`None` prunes
    /// nothing -- the "Unlimited" retention-count option keeps everything
    /// regardless of count).
    pub fn prune_by_count(&self, keep: Option<usize>) -> Result<()> {
        let Some(keep) = keep else {
            return Ok(());
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM history WHERE id NOT IN (
                SELECT id FROM history ORDER BY id DESC LIMIT ?1
            )",
            [keep as i64],
        )?;
        Ok(())
    }

    /// Deletes every entry whose `created_at` is strictly before `cutoff`
    /// (Unix epoch milliseconds). The timestamp-explicit half of
    /// [`HistoryStore::prune_by_age`], so a test can assert pruning
    /// deterministically instead of racing the wall clock (mirrors
    /// `append`/`append_with_timestamp`).
    fn prune_before(&self, cutoff: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM history WHERE created_at < ?1", [cutoff])?;
        Ok(())
    }

    /// Deletes every entry older than `max_age_days` days, measured from
    /// `now` (`None` prunes nothing -- an unset `history.retention_days`
    /// keeps everything regardless of age).
    fn prune_by_age_at(&self, max_age_days: Option<i64>, now: i64) -> Result<()> {
        let Some(days) = max_age_days else {
            return Ok(());
        };
        self.prune_before(now - days * MILLIS_PER_DAY)
    }

    /// [`HistoryStore::prune_by_age_at`] against the real wall clock --
    /// what [`prune`] actually calls.
    pub fn prune_by_age(&self, max_age_days: Option<i64>) -> Result<()> {
        self.prune_by_age_at(max_age_days, now_millis())
    }

    fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
        let id: i64 = row.get(0)?;
        Ok(HistoryEntry {
            id: id.to_string(),
            original: row.get(1)?,
            refined: row.get(2)?,
            model: row.get(3)?,
            created_at: row.get(4)?,
            command: row.get(5)?,
        })
    }
}

/// Current time as Unix epoch milliseconds. Falls back to `0` in the
/// (practically unreachable) case the system clock is set before the epoch,
/// rather than panicking a refine over a timestamp.
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A [`TextCapture`] that always returns a fixed, already-known string
/// rather than reading the current OS selection. Lets [`history_rerefine_impl`]
/// reuse `Orchestrator`'s prompt-build/model-call/inject pipeline against a
/// *past* entry's original text, instead of whatever happens to be selected
/// right now.
struct FixedCapture(String);

impl TextCapture for FixedCapture {
    fn capture(&self) -> Result<String> {
        Ok(self.0.clone())
    }
}

/// Tauri command: lists every recorded refine, most recent first.
#[tauri::command]
pub fn history_list(state: tauri::State<'_, HistoryStore>) -> Result<Vec<HistoryEntry>, String> {
    state.list().map_err(|e| e.to_string())
}

/// The plain logic behind the `history_get` command, taking `&HistoryStore`
/// directly so it's unit-testable without a `tauri::State` harness (mirrors
/// `connections.rs`'s `*_impl` split).
pub fn history_get_impl(store: &HistoryStore, id: &str) -> Result<HistoryEntry, String> {
    store
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no history entry with id {id}"))
}

/// Tauri command: returns the full detail of a single past refine.
#[tauri::command]
pub fn history_get(
    state: tauri::State<'_, HistoryStore>,
    id: String,
) -> Result<HistoryEntry, String> {
    history_get_impl(&state, &id)
}

/// The plain logic behind the `history_copy` command: looks a past entry up
/// so the frontend can write its refined text to the OS clipboard (via the
/// browser Clipboard API) after a successful call. Identical lookup to
/// [`history_get_impl`], but kept as its own named entry point since it
/// backs a distinct control (`history-copy`/`history-detail-copy`, C12) with
/// its own `controls/history.json` contract.
pub fn history_copy_impl(store: &HistoryStore, id: &str) -> Result<HistoryEntry, String> {
    store
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no history entry with id {id}"))
}

/// Tauri command: looks up the entry the History screen's copy control
/// (`history-copy`/`history-detail-copy`) is copying, for the frontend to
/// write to the clipboard.
#[tauri::command]
pub fn history_copy(state: tauri::State<'_, HistoryStore>, id: String) -> Result<HistoryEntry, String> {
    history_copy_impl(&state, &id)
}

/// The plain logic behind the `history_clear` command: empties the store.
/// Kept `&HistoryStore`-generic (mirrors `history_get_impl`) so it's
/// unit-testable without a `tauri::State` harness.
pub fn history_clear_impl(store: &HistoryStore) -> Result<(), String> {
    store.clear().map_err(|e| e.to_string())
}

/// Tauri command: deletes every recorded history entry (the History
/// screen's clear-all control, `history-clear-confirm`, C15).
#[tauri::command]
pub fn history_clear(state: tauri::State<'_, HistoryStore>) -> Result<(), String> {
    history_clear_impl(&state)
}

/// Prunes `store` per the Behavior screen's persisted retention settings
/// (`history.retention_count`/`history.retention_days`, C5/C11): an unset or
/// non-numeric `history.retention_count` (the "Unlimited" option) never
/// prunes by count, and an unset `history.retention_days` never prunes by
/// age. Called by `lib.rs`'s `execute_refine` right after every successful
/// [`HistoryStore::append`], so the store never grows past the configured
/// cap mid-session.
///
/// `history.retention_days = "0"` (the screen's "session only" option,
/// which reads "history cleared on quit" in the UI copy) has no dedicated
/// app-shutdown hook to clear on here -- pruning by a zero-day age cutoff
/// right after every append is the closest a per-append prune gets to that:
/// in practice, every entry from *before* the one just appended is already
/// "older than now" and gets pruned, so the store never accumulates past
/// the latest refine within a session either.
pub fn prune(store: &HistoryStore, settings: &SettingsStore) -> Result<(), String> {
    let retention_count = settings
        .get(RETENTION_COUNT_KEY)
        .map_err(|e| e.to_string())?
        .and_then(|raw| raw.parse::<usize>().ok());
    let retention_days = settings
        .get(RETENTION_DAYS_KEY)
        .map_err(|e| e.to_string())?
        .and_then(|raw| raw.parse::<i64>().ok());

    store.prune_by_count(retention_count).map_err(|e| e.to_string())?;
    store.prune_by_age(retention_days).map_err(|e| e.to_string())
}

/// Restores a past entry's *original* text: looks it up and injects it back
/// into the focused app, reusing the same [`TextInjector`] seam
/// `restore_original`/`inject_text` (`lib.rs`) use — unlike those two, this
/// is one call rather than a fetch-then-inject pair, matching
/// `wireframes/history.html`'s single "Restore original" action. Generic
/// over the injector (mirrors `lib.rs`'s `inject_text_with`) so it's
/// unit-testable with a fake on any platform.
pub fn history_restore_impl<I: TextInjector>(
    store: &HistoryStore,
    injector: &I,
    id: &str,
) -> Result<String, String> {
    let entry = store
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no history entry with id {id}"))?;
    injector.inject(&entry.original).map_err(|e| e.to_string())?;
    Ok(entry.original)
}

/// Tauri command wrapping [`history_restore_impl`] with the real
/// `text-inject`-backed injector.
#[tauri::command]
pub fn history_restore(
    state: tauri::State<'_, HistoryStore>,
    id: String,
) -> Result<String, String> {
    history_restore_impl(&state, &SystemTextIo, &id)
}

/// Re-runs refine on a past entry's *original* text (optionally with a
/// different model than the one it originally used), injects the new
/// result, and records it as a fresh history entry -- the past entry itself
/// is left untouched. Reuses `Orchestrator::refine` (via [`FixedCapture`])
/// rather than re-implementing prompt-build/model-call, so a re-refine goes
/// through the exact same pipeline a live refine does. Generic over the
/// injector (mirrors [`history_restore_impl`]) so it's unit-testable with a
/// fake provider/injector, independent of a live network call or OS inject.
pub async fn history_rerefine_impl<I: TextInjector>(
    store: &HistoryStore,
    provider: Arc<dyn LlmProvider>,
    model: String,
    injector: I,
    id: &str,
) -> Result<HistoryEntry, String> {
    let entry = store
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no history entry with id {id}"))?;

    let opts = BuildOptions {
        model: model.clone(),
        ..BuildOptions::default()
    };
    let orch = Orchestrator::new(FixedCapture(entry.original.clone()), injector, provider);
    let outcome = orch
        .refine(&opts, CancellationToken::new())
        .await
        .map_err(|e| e.to_string())?;

    store
        .append(
            &outcome.original,
            &outcome.refined,
            &outcome.model,
            entry.command.as_deref(),
        )
        .map_err(|e| e.to_string())
}

/// Finds the first stored connection that has `model_id` enabled -- the
/// same resolution shape `lib.rs`'s `resolve_fallback_targets` uses for its
/// configured fallback chain, reused here to pick which connection/provider
/// a re-refine's (possibly overridden) model should run against.
fn resolve_connection_for_model(
    connections: &ConnectionStore,
    model_id: &str,
) -> Result<Connection, String> {
    let stored = connections.list().map_err(|e| e.to_string())?;
    stored
        .into_iter()
        .find(|c| c.enabled_models.iter().any(|m| m == model_id))
        .ok_or_else(|| format!("no enabled connection found for model {model_id}"))
}

/// Tauri command wrapping [`history_rerefine_impl`]: resolves `model`
/// (falling back to the entry's own model when omitted) to an enabled
/// connection (see [`resolve_connection_for_model`]), builds its
/// vendor-native provider (resolving the API key via
/// `connections::resolve_api_key`, secure storage first -- same as
/// `lib.rs`'s `build_provider`), and runs the re-refine through the real
/// `text-inject`-backed injector.
#[tauri::command]
pub async fn history_rerefine(
    history: tauri::State<'_, HistoryStore>,
    connections: tauri::State<'_, ConnectionStore>,
    secrets: tauri::State<'_, SecretStore>,
    id: String,
    model: Option<String>,
) -> Result<HistoryEntry, String> {
    let entry = history
        .get(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no history entry with id {id}"))?;
    let model_id = model.unwrap_or_else(|| entry.model.clone());

    let connection = resolve_connection_for_model(&connections, &model_id)?;
    let api_key =
        resolve_api_key(&connections, &secrets, &connection.id).map_err(|e| e.to_string())?;
    let provider: Arc<dyn LlmProvider> = Arc::from(provider_for(
        &connection.provider_kind,
        &connection.base_url,
        &api_key,
    ));

    history_rerefine_impl(&history, provider, model_id, SystemTextIo, &id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use llm_provider::{LlmRequest, LlmResponse};
    use std::sync::Mutex as StdMutex;

    fn new_store() -> HistoryStore {
        HistoryStore::open_in_memory().expect("failed to open in-memory store")
    }

    // ── HistoryStore: append / list / get ──

    #[test]
    fn list_is_empty_for_a_fresh_store() {
        let store = new_store();
        assert_eq!(store.list().unwrap(), vec![]);
    }

    #[test]
    fn append_then_list_returns_the_stored_entry() {
        let store = new_store();
        let added = store
            .append("original text", "refined text", "gpt-5.1", None)
            .unwrap();

        assert_eq!(added.original, "original text");
        assert_eq!(added.refined, "refined text");
        assert_eq!(added.model, "gpt-5.1");
        assert_eq!(added.command, None);

        assert_eq!(store.list().unwrap(), vec![added]);
    }

    #[test]
    fn append_records_an_optional_command_trigger() {
        let store = new_store();
        let added = store
            .append("original", "refined", "gpt-5.1", Some("/formal"))
            .unwrap();

        assert_eq!(added.command, Some("/formal".to_string()));
    }

    #[test]
    fn list_returns_entries_most_recent_first() {
        let store = new_store();
        let first = store
            .append_with_timestamp("first original", "first refined", "gpt-5.1", None, 100)
            .unwrap();
        let second = store
            .append_with_timestamp("second original", "second refined", "gpt-5.1", None, 200)
            .unwrap();

        assert_eq!(store.list().unwrap(), vec![second, first]);
    }

    #[test]
    fn get_returns_none_for_an_unknown_id() {
        let store = new_store();
        assert_eq!(store.get("999").unwrap(), None);
    }

    #[test]
    fn get_returns_the_matching_entry() {
        let store = new_store();
        let added = store.append("original", "refined", "gpt-5.1", None).unwrap();

        assert_eq!(store.get(&added.id).unwrap(), Some(added));
    }

    // ── history_get_impl ──

    #[test]
    fn history_get_impl_errors_for_an_unknown_id() {
        let store = new_store();
        let err = history_get_impl(&store, "999").unwrap_err();
        assert!(err.contains("999"), "got: {err}");
    }

    #[test]
    fn history_get_impl_returns_the_full_entry() {
        let store = new_store();
        let added = store.append("original", "refined", "gpt-5.1", None).unwrap();

        assert_eq!(history_get_impl(&store, &added.id).unwrap(), added);
    }

    // ── history_restore_impl ──

    #[derive(Clone, Default)]
    struct FakeInjector {
        injected: Arc<StdMutex<Vec<String>>>,
    }

    impl FakeInjector {
        fn injected(&self) -> Vec<String> {
            self.injected.lock().unwrap().clone()
        }
    }

    impl TextInjector for FakeInjector {
        fn inject(&self, text: &str) -> Result<()> {
            self.injected.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }

    #[test]
    fn history_restore_impl_injects_the_original_and_returns_it() {
        let store = new_store();
        let added = store
            .append("the original selection", "the refined text", "gpt-5.1", None)
            .unwrap();
        let injector = FakeInjector::default();

        let restored = history_restore_impl(&store, &injector, &added.id).unwrap();

        assert_eq!(restored, "the original selection");
        assert_eq!(injector.injected(), vec!["the original selection".to_string()]);
    }

    #[test]
    fn history_restore_impl_errors_for_an_unknown_id_without_injecting() {
        let store = new_store();
        let injector = FakeInjector::default();

        let err = history_restore_impl(&store, &injector, "999").unwrap_err();

        assert!(err.contains("999"), "got: {err}");
        assert!(injector.injected().is_empty());
    }

    // ── history_rerefine_impl ──

    struct FakeProvider(String);

    #[async_trait]
    impl LlmProvider for FakeProvider {
        async fn chat(
            &self,
            _request: &LlmRequest,
            _cancel: CancellationToken,
        ) -> Result<LlmResponse> {
            Ok(LlmResponse {
                text: self.0.clone(),
                model: "fake-model".to_string(),
                usage_tokens: None,
            })
        }

        async fn list_models(&self) -> Result<Vec<String>> {
            Ok(vec!["fake-model".to_string()])
        }

        async fn is_available(&self) -> bool {
            true
        }

        fn provider_name(&self) -> &'static str {
            "fake"
        }
    }

    #[tokio::test]
    async fn history_rerefine_impl_reruns_refine_on_the_original_and_records_a_new_entry() {
        let store = new_store();
        let entry = store
            .append("the original selection", "the old refined text", "old-model", Some("/formal"))
            .unwrap();
        let injector = FakeInjector::default();

        let new_entry = history_rerefine_impl(
            &store,
            Arc::new(FakeProvider("brand new refined text".to_string())),
            "fake-model".to_string(),
            injector.clone(),
            &entry.id,
        )
        .await
        .unwrap();

        // Re-refines the *original*, not the old refined output.
        assert_eq!(new_entry.original, "the original selection");
        assert_eq!(new_entry.refined, "brand new refined text");
        assert_eq!(new_entry.model, "fake-model");
        // Carries the past entry's command trigger forward.
        assert_eq!(new_entry.command, Some("/formal".to_string()));
        // The old entry itself is untouched -- a fresh entry was recorded.
        assert_ne!(new_entry.id, entry.id);
        assert_eq!(store.get(&entry.id).unwrap(), Some(entry));

        assert_eq!(injector.injected(), vec!["brand new refined text".to_string()]);

        // Both entries now show up in the list, most recent first.
        assert_eq!(store.list().unwrap()[0], new_entry);
    }

    #[tokio::test]
    async fn history_rerefine_impl_errors_for_an_unknown_id() {
        let store = new_store();
        let injector = FakeInjector::default();

        let err = history_rerefine_impl(
            &store,
            Arc::new(FakeProvider("x".to_string())),
            "fake-model".to_string(),
            injector.clone(),
            "999",
        )
        .await
        .unwrap_err();

        assert!(err.contains("999"), "got: {err}");
        assert!(injector.injected().is_empty());
    }

    #[tokio::test]
    async fn history_rerefine_impl_propagates_a_provider_failure_without_injecting_or_recording() {
        struct FailingProvider;
        #[async_trait]
        impl LlmProvider for FailingProvider {
            async fn chat(
                &self,
                _request: &LlmRequest,
                _cancel: CancellationToken,
            ) -> Result<LlmResponse> {
                anyhow::bail!("model call failed")
            }
            async fn list_models(&self) -> Result<Vec<String>> {
                Ok(vec![])
            }
            async fn is_available(&self) -> bool {
                false
            }
            fn provider_name(&self) -> &'static str {
                "failing"
            }
        }

        let store = new_store();
        let entry = store
            .append("original", "old refined", "old-model", None)
            .unwrap();
        let injector = FakeInjector::default();

        let result = history_rerefine_impl(
            &store,
            Arc::new(FailingProvider),
            "fake-model".to_string(),
            injector.clone(),
            &entry.id,
        )
        .await;

        assert!(result.is_err());
        assert!(injector.injected().is_empty());
        // No new entry was recorded -- only the original is still there.
        assert_eq!(store.list().unwrap(), vec![entry]);
    }

    // ── HistoryStore::clear / history_clear_impl ──

    #[test]
    fn clear_empties_a_populated_store() {
        let store = new_store();
        store.append("original one", "refined one", "gpt-5.1", None).unwrap();
        store.append("original two", "refined two", "gpt-5.1", None).unwrap();
        assert_eq!(store.list().unwrap().len(), 2);

        store.clear().unwrap();

        assert_eq!(store.list().unwrap(), vec![]);
    }

    #[test]
    fn clear_is_a_no_op_on_an_already_empty_store() {
        let store = new_store();

        store.clear().unwrap();

        assert_eq!(store.list().unwrap(), vec![]);
    }

    #[test]
    fn history_clear_impl_empties_the_store() {
        let store = new_store();
        store.append("original", "refined", "gpt-5.1", None).unwrap();

        history_clear_impl(&store).unwrap();

        assert_eq!(store.list().unwrap(), vec![]);
    }

    // ── history_copy_impl ──

    #[test]
    fn history_copy_impl_returns_the_full_entry() {
        let store = new_store();
        let added = store.append("original", "refined", "gpt-5.1", None).unwrap();

        assert_eq!(history_copy_impl(&store, &added.id).unwrap(), added);
    }

    #[test]
    fn history_copy_impl_errors_for_an_unknown_id() {
        let store = new_store();
        let err = history_copy_impl(&store, "999").unwrap_err();
        assert!(err.contains("999"), "got: {err}");
    }

    // ── prune_by_count / prune_by_age (C11's retention cap, wired by C17) ──

    #[test]
    fn prune_by_count_keeps_only_the_n_most_recent_entries() {
        let store = new_store();
        store.append_with_timestamp("one", "one refined", "gpt-5.1", None, 100).unwrap();
        let second = store
            .append_with_timestamp("two", "two refined", "gpt-5.1", None, 200)
            .unwrap();
        let third = store
            .append_with_timestamp("three", "three refined", "gpt-5.1", None, 300)
            .unwrap();

        store.prune_by_count(Some(2)).unwrap();

        assert_eq!(store.list().unwrap(), vec![third, second]);
    }

    #[test]
    fn prune_by_count_none_keeps_everything() {
        let store = new_store();
        store.append("one", "one refined", "gpt-5.1", None).unwrap();
        store.append("two", "two refined", "gpt-5.1", None).unwrap();

        store.prune_by_count(None).unwrap();

        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn prune_by_count_is_a_no_op_when_there_are_fewer_entries_than_the_cap() {
        let store = new_store();
        store.append("one", "one refined", "gpt-5.1", None).unwrap();

        store.prune_by_count(Some(50)).unwrap();

        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn prune_by_age_at_deletes_entries_older_than_the_cutoff() {
        let store = new_store();
        let one_day_old = store
            .append_with_timestamp("stale", "stale refined", "gpt-5.1", None, 0)
            .unwrap();
        let fresh = store
            .append_with_timestamp("fresh", "fresh refined", "gpt-5.1", None, 2 * MILLIS_PER_DAY)
            .unwrap();

        // "now" is exactly 2 days after the epoch used above; a 1-day
        // retention window prunes the entry created at t=0 but keeps the
        // one created at t=2 days.
        store.prune_by_age_at(Some(1), 2 * MILLIS_PER_DAY).unwrap();

        let remaining = store.list().unwrap();
        assert_eq!(remaining, vec![fresh]);
        assert!(!remaining.contains(&one_day_old));
    }

    #[test]
    fn prune_by_age_at_none_keeps_everything_regardless_of_age() {
        let store = new_store();
        store
            .append_with_timestamp("ancient", "ancient refined", "gpt-5.1", None, 0)
            .unwrap();

        store.prune_by_age_at(None, 100 * MILLIS_PER_DAY).unwrap();

        assert_eq!(store.list().unwrap().len(), 1);
    }

    // ── prune (reads the Behavior screen's retention settings) ──

    fn new_settings() -> SettingsStore {
        SettingsStore::open_in_memory().expect("failed to open in-memory settings")
    }

    #[test]
    fn prune_with_no_settings_configured_keeps_everything() {
        let store = new_store();
        let settings = new_settings();
        store.append("one", "one refined", "gpt-5.1", None).unwrap();
        store.append("two", "two refined", "gpt-5.1", None).unwrap();

        prune(&store, &settings).unwrap();

        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn prune_applies_the_configured_retention_count() {
        let store = new_store();
        let settings = new_settings();
        settings.set(RETENTION_COUNT_KEY, "1").unwrap();
        store
            .append_with_timestamp("one", "one refined", "gpt-5.1", None, 100)
            .unwrap();
        let second = store
            .append_with_timestamp("two", "two refined", "gpt-5.1", None, 200)
            .unwrap();

        prune(&store, &settings).unwrap();

        assert_eq!(store.list().unwrap(), vec![second]);
    }

    #[test]
    fn prune_treats_unlimited_as_no_count_cap() {
        let store = new_store();
        let settings = new_settings();
        settings.set(RETENTION_COUNT_KEY, "Unlimited").unwrap();
        store.append("one", "one refined", "gpt-5.1", None).unwrap();
        store.append("two", "two refined", "gpt-5.1", None).unwrap();

        prune(&store, &settings).unwrap();

        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn prune_applies_the_configured_retention_days() {
        let store = new_store();
        let settings = new_settings();
        settings.set(RETENTION_DAYS_KEY, "1").unwrap();
        store
            .append_with_timestamp("stale", "stale refined", "gpt-5.1", None, 0)
            .unwrap();
        let fresh = store
            .append_with_timestamp("fresh", "fresh refined", "gpt-5.1", None, now_millis())
            .unwrap();

        prune(&store, &settings).unwrap();

        assert_eq!(store.list().unwrap(), vec![fresh]);
    }

    // ── resolve_connection_for_model ──

    #[test]
    fn resolve_connection_for_model_finds_the_enabled_connection() {
        let connections = ConnectionStore::open_in_memory().unwrap();
        connections
            .add("openai", "https://api.openai.com", None, &["gpt-5.1".to_string()])
            .unwrap();
        let anthropic = connections
            .add(
                "anthropic",
                "https://api.anthropic.com",
                None,
                &["claude-opus-4-6".to_string()],
            )
            .unwrap();

        let resolved = resolve_connection_for_model(&connections, "claude-opus-4-6").unwrap();

        assert_eq!(resolved.id, anthropic.id);
    }

    #[test]
    fn resolve_connection_for_model_errors_when_no_connection_has_it_enabled() {
        let connections = ConnectionStore::open_in_memory().unwrap();
        connections
            .add("openai", "https://api.openai.com", None, &["gpt-5.1".to_string()])
            .unwrap();

        let err = resolve_connection_for_model(&connections, "no-such-model").unwrap_err();
        assert!(err.contains("no-such-model"), "got: {err}");
    }
}
