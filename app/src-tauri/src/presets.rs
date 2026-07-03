// Built-in + user preset store: a "preset" is a named, reusable refine
// direction the `/preset` trigger (`command_parser`'s `ParsedCommand::preset`,
// B4) maps to a stored direction plus optional model/language/inject-mode
// overrides and a few before/after examples.
//
// A small set of presets ships with the app (`built_in_presets`) so there's
// something useful on first run; the user can also create their own, and can
// edit a built-in -- which doesn't mutate the shipped default in place but
// creates a *user override* row that shadows it (surfaced by `overridden` on
// the resolved `Preset`, so a caller -- the Presets screen, C8/S14 -- can
// warn before saving and offer a reset back to the default, `reset_default`).
// Import/export round-trips the user store (only) through JSON.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/presets_test.rs` via `include!` inside an inline `mod` block ahead
// of C17 wiring it into `lib.rs`'s module tree, and Rust doesn't allow an
// inner doc comment produced by macro expansion to sit at the start of that
// block -- see `settings.rs`/`connections.rs`.)

use anyhow::{bail, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

/// A single before/after pair teaching a preset's tone (few-shot examples).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetExample {
    pub before: String,
    pub after: String,
}

/// A stored preset: the direction (and optional overrides) a `/trigger`
/// resolves to.
///
/// `builtin`/`overridden` are never persisted (see [`PresetStore::save`]) --
/// they're computed fresh whenever a `Preset` is produced by
/// [`PresetStore::list`]/[`PresetStore::resolve`], so they always reflect
/// the *current* shipped built-in set rather than a stale snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    /// The trigger word (no leading `/`), matching `command_parser`'s
    /// `ParsedCommand::preset`. Always lowercase (see [`normalize_trigger`]).
    pub trigger: String,
    pub direction: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub inject: Option<String>,
    #[serde(default)]
    pub examples: Vec<PresetExample>,
    /// Whether `trigger` names one of the shipped [`built_in_presets`],
    /// regardless of whether the user has overridden it.
    #[serde(default)]
    pub builtin: bool,
    /// Whether a user-saved row shadows this built-in trigger (always
    /// `false` for a plain user preset, and for an unmodified built-in).
    /// This is the data the Presets screen (C8/S14) surfaces as its
    /// override warning / "reset to default" affordance.
    #[serde(default)]
    pub overridden: bool,
}

impl Preset {
    fn builtin(trigger: &str, direction: &str) -> Self {
        Preset {
            trigger: trigger.to_string(),
            direction: direction.to_string(),
            model: None,
            lang: None,
            inject: None,
            examples: Vec::new(),
            builtin: true,
            overridden: false,
        }
    }
}

/// The shipped built-in preset set. A small, opinionated starter set covering
/// the common refine directions (`wireframes/presets.html`'s "Built-in"
/// group) -- tone shifts (`formal`/`concise`/`friendly`), a structural
/// rewrite (`bullets`), and a context-aware one (`reply`, which drafts a
/// reply to the quoted message `quote_parser`/`prompt_builder` fold in).
fn built_in_presets() -> Vec<Preset> {
    vec![
        Preset::builtin(
            "formal",
            "Rewrite formally and professionally; no slang or emoji; keep meaning.",
        ),
        Preset::builtin(
            "concise",
            "Tighten: cut filler, shorten, keep the point.",
        ),
        Preset::builtin(
            "friendly",
            "Warmer, more personable tone; keep it natural.",
        ),
        Preset::builtin(
            "bullets",
            "Restructure into clear, scannable bullet points.",
        ),
        Preset::builtin("reply", "Draft a reply to the quoted message."),
    ]
}

fn is_builtin_trigger(trigger: &str) -> bool {
    built_in_presets().iter().any(|p| p.trigger == trigger)
}

/// Triggers are matched case-insensitively (mirroring `command_parser`'s
/// reserved-tag matching) -- normalized to lowercase everywhere a trigger is
/// stored or looked up.
fn normalize_trigger(trigger: &str) -> String {
    trigger.trim().to_ascii_lowercase()
}

/// The result of [`PresetStore::import`]: every trigger actually imported,
/// and the subset of those that already existed (as a built-in or a prior
/// user preset) and so were overwritten rather than newly added.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetImportResult {
    pub imported: Vec<String>,
    pub conflicts: Vec<String>,
}

pub struct PresetStore {
    conn: Mutex<Connection>,
}

impl PresetStore {
    /// Opens a file-backed SQLite database at the given path, creating the
    /// presets table if this is the first run.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Opens an in-memory SQLite database (used by tests and defaults).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
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
            "CREATE TABLE IF NOT EXISTS presets (
                trigger   TEXT PRIMARY KEY,
                direction TEXT NOT NULL,
                model     TEXT,
                lang      TEXT,
                inject    TEXT,
                examples  TEXT NOT NULL DEFAULT '[]'
            )",
            [],
        )?;
        Ok(())
    }

    /// Every user-saved row, unmerged with the built-in set (raw storage --
    /// what [`export`](Self::export) dumps).
    fn user_rows(&self) -> Result<Vec<Preset>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT trigger, direction, model, lang, inject, examples
             FROM presets ORDER BY trigger",
        )?;
        let rows = stmt.query_map([], Self::row_to_preset)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn user_row(&self, trigger: &str) -> Result<Option<Preset>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT trigger, direction, model, lang, inject, examples
             FROM presets WHERE trigger = ?1",
            [trigger],
            Self::row_to_preset,
        )
        .optional()
        .map_err(Into::into)
    }

    fn row_to_preset(row: &rusqlite::Row<'_>) -> rusqlite::Result<Preset> {
        let trigger: String = row.get(0)?;
        let direction: String = row.get(1)?;
        let model: Option<String> = row.get(2)?;
        let lang: Option<String> = row.get(3)?;
        let inject: Option<String> = row.get(4)?;
        let examples_json: String = row.get(5)?;
        let examples: Vec<PresetExample> = serde_json::from_str(&examples_json).unwrap_or_default();
        Ok(Preset {
            trigger,
            direction,
            model,
            lang,
            inject,
            examples,
            builtin: false,
            overridden: false,
        })
    }

    /// Every preset available for use: the built-in set (a user override, if
    /// one exists, shown in place of the shipped default) followed by the
    /// user's own presets, in trigger order.
    pub fn list(&self) -> Result<Vec<Preset>> {
        let rows = self.user_rows()?;
        let mut out = Vec::new();

        for builtin in built_in_presets() {
            match rows.iter().find(|r| r.trigger == builtin.trigger) {
                Some(row) => {
                    let mut overridden = row.clone();
                    overridden.builtin = true;
                    overridden.overridden = true;
                    out.push(overridden);
                }
                None => out.push(builtin),
            }
        }

        for row in rows
            .into_iter()
            .filter(|r| !is_builtin_trigger(&r.trigger))
        {
            out.push(row);
        }

        Ok(out)
    }

    /// Creates or updates a user preset. Saving under a built-in's trigger
    /// creates/updates the override that shadows it (see the module docs);
    /// saving under any other trigger creates/updates a plain user preset.
    pub fn save(
        &self,
        trigger: &str,
        direction: &str,
        model: Option<&str>,
        lang: Option<&str>,
        inject: Option<&str>,
        examples: &[PresetExample],
    ) -> Result<Preset> {
        let trigger = normalize_trigger(trigger);
        if trigger.is_empty() {
            bail!("preset trigger must not be empty");
        }
        let examples_json = serde_json::to_string(examples)?;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO presets (trigger, direction, model, lang, inject, examples)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(trigger) DO UPDATE SET
                direction = excluded.direction,
                model     = excluded.model,
                lang      = excluded.lang,
                inject    = excluded.inject,
                examples  = excluded.examples",
            (&trigger, direction, model, lang, inject, &examples_json),
        )?;
        drop(conn);

        self.resolve(&trigger)?
            .ok_or_else(|| anyhow::anyhow!("preset {trigger} vanished after save"))
    }

    /// Deletes a user preset (or override). Errors if `trigger` has no
    /// user-saved row -- there's nothing to delete for an unmodified
    /// built-in (see [`reset_default`](Self::reset_default), which is the
    /// operation for "go back to the shipped default").
    pub fn delete(&self, trigger: &str) -> Result<()> {
        let trigger = normalize_trigger(trigger);
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM presets WHERE trigger = ?1", [&trigger])?;
        if changed == 0 {
            bail!("no such preset: {trigger}");
        }
        Ok(())
    }

    /// Copies the resolved preset at `trigger` (built-in, override, or
    /// user) into a new user preset under `new_trigger`, leaving the
    /// original untouched -- the "Duplicate instead" alternative to
    /// overriding a built-in in place.
    pub fn duplicate(&self, trigger: &str, new_trigger: &str) -> Result<Preset> {
        let source = self
            .resolve(trigger)?
            .ok_or_else(|| anyhow::anyhow!("no such preset: {}", normalize_trigger(trigger)))?;
        let new_trigger = normalize_trigger(new_trigger);
        if new_trigger.is_empty() {
            bail!("preset trigger must not be empty");
        }
        if self.user_row(&new_trigger)?.is_some() {
            bail!("a preset with trigger {new_trigger} already exists");
        }
        self.save(
            &new_trigger,
            &source.direction,
            source.model.as_deref(),
            source.lang.as_deref(),
            source.inject.as_deref(),
            &source.examples,
        )
    }

    /// Restores a built-in preset the user overrode back to its shipped
    /// default, by discarding the override row. Errors if `trigger` isn't a
    /// built-in, or has no override to reset.
    pub fn reset_default(&self, trigger: &str) -> Result<()> {
        let trigger = normalize_trigger(trigger);
        if !is_builtin_trigger(&trigger) {
            bail!("{trigger} is not a built-in preset");
        }
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM presets WHERE trigger = ?1", [&trigger])?;
        if changed == 0 {
            bail!("{trigger} has no override to reset");
        }
        Ok(())
    }

    /// Resolves a `/trigger` (as extracted by `command_parser`, no leading
    /// `/`) to its effective preset -- a user override or plain user preset
    /// if one is saved, else the shipped built-in, else `None`. This is what
    /// the orchestrator's command pipeline (C17) folds into
    /// `prompt_builder::BuildOptions` (direction, and the model/lang/inject
    /// overrides) once a selection's `/trigger` is parsed.
    pub fn resolve(&self, trigger: &str) -> Result<Option<Preset>> {
        let trigger = normalize_trigger(trigger);
        if let Some(mut row) = self.user_row(&trigger)? {
            row.builtin = is_builtin_trigger(&trigger);
            row.overridden = row.builtin;
            return Ok(Some(row));
        }
        Ok(built_in_presets()
            .into_iter()
            .find(|p| p.trigger == trigger))
    }

    /// Dumps every user-saved preset (overrides and plain user presets --
    /// not the built-in set itself, which ships with the app and needs no
    /// export) as a portable JSON array.
    pub fn export(&self) -> Result<String> {
        let rows = self.user_rows()?;
        Ok(serde_json::to_string_pretty(&rows)?)
    }

    /// Imports a JSON array of presets (the shape [`export`](Self::export)
    /// produces), upserting each into the user store. A trigger that
    /// already resolved to something (a built-in or a prior user preset) is
    /// reported in `conflicts` -- it's still imported (overwriting/
    /// overriding), just flagged so the caller can surface it.
    pub fn import(&self, json: &str) -> Result<PresetImportResult> {
        let incoming: Vec<Preset> = serde_json::from_str(json)?;
        let mut result = PresetImportResult::default();
        for p in incoming {
            let trigger = normalize_trigger(&p.trigger);
            if trigger.is_empty() {
                continue;
            }
            let existed = self.resolve(&trigger)?.is_some();
            self.save(
                &trigger,
                &p.direction,
                p.model.as_deref(),
                p.lang.as_deref(),
                p.inject.as_deref(),
                &p.examples,
            )?;
            if existed {
                result.conflicts.push(trigger.clone());
            }
            result.imported.push(trigger);
        }
        Ok(result)
    }
}

/// Tauri command: lists every available preset (built-in + user, with
/// override status) for the Presets screen (C3/C8).
#[tauri::command]
pub fn preset_list(state: tauri::State<'_, PresetStore>) -> Result<Vec<Preset>, String> {
    state.list().map_err(|e| e.to_string())
}

/// Tauri command: creates or updates a user preset (or a built-in override
/// -- see [`PresetStore::save`]).
#[tauri::command]
pub fn preset_save(
    state: tauri::State<'_, PresetStore>,
    trigger: String,
    direction: String,
    model: Option<String>,
    lang: Option<String>,
    inject: Option<String>,
    examples: Vec<PresetExample>,
) -> Result<Preset, String> {
    state
        .save(
            &trigger,
            &direction,
            model.as_deref(),
            lang.as_deref(),
            inject.as_deref(),
            &examples,
        )
        .map_err(|e| e.to_string())
}

/// Tauri command: deletes a user preset (or override).
#[tauri::command]
pub fn preset_delete(state: tauri::State<'_, PresetStore>, trigger: String) -> Result<(), String> {
    state.delete(&trigger).map_err(|e| e.to_string())
}

/// Tauri command: duplicates a preset (built-in, override, or user) under a
/// new trigger.
#[tauri::command]
pub fn preset_duplicate(
    state: tauri::State<'_, PresetStore>,
    trigger: String,
    new_trigger: String,
) -> Result<Preset, String> {
    state
        .duplicate(&trigger, &new_trigger)
        .map_err(|e| e.to_string())
}

/// Tauri command: restores a built-in preset the user overrode back to its
/// shipped default.
#[tauri::command]
pub fn preset_reset_default(
    state: tauri::State<'_, PresetStore>,
    trigger: String,
) -> Result<(), String> {
    state.reset_default(&trigger).map_err(|e| e.to_string())
}

/// Tauri command: exports the user's saved presets as a portable JSON
/// string.
#[tauri::command]
pub fn preset_export(state: tauri::State<'_, PresetStore>) -> Result<String, String> {
    state.export().map_err(|e| e.to_string())
}

/// Tauri command: imports a JSON array of presets, merging them into the
/// user store and flagging trigger conflicts.
#[tauri::command]
pub fn preset_import(
    state: tauri::State<'_, PresetStore>,
    json: String,
) -> Result<PresetImportResult, String> {
    state.import(&json).map_err(|e| e.to_string())
}

/// Tauri command: resolves a parsed `/trigger` to its effective preset, for
/// the orchestrator's command pipeline (C17) to fold into the refine prompt.
#[tauri::command]
pub fn preset_resolve(
    state: tauri::State<'_, PresetStore>,
    trigger: String,
) -> Result<Option<Preset>, String> {
    state.resolve(&trigger).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_store() -> PresetStore {
        PresetStore::open_in_memory().expect("failed to open in-memory preset store")
    }

    // ---- Step 1: built-in seed ----

    #[test]
    fn built_ins_load_and_are_flagged() {
        let store = new_store();
        let presets = store.list().unwrap();

        assert_eq!(presets.len(), 5, "the shipped built-in set");
        assert!(presets.iter().all(|p| p.builtin));
        assert!(presets.iter().all(|p| !p.overridden));
        assert!(presets.iter().any(|p| p.trigger == "formal"));
        assert!(presets.iter().any(|p| p.trigger == "concise"));
        assert!(presets.iter().any(|p| p.trigger == "friendly"));
        assert!(presets.iter().any(|p| p.trigger == "bullets"));
        assert!(presets.iter().any(|p| p.trigger == "reply"));
    }

    // ---- Step 2: user CRUD + duplicate/reset ----

    #[test]
    fn save_then_list_shows_the_new_user_preset() {
        let store = new_store();

        let saved = store
            .save(
                "standup",
                "Reformat into Yesterday / Today / Blockers.",
                None,
                None,
                None,
                &[],
            )
            .unwrap();
        assert_eq!(saved.trigger, "standup");
        assert!(!saved.builtin);
        assert!(!saved.overridden);

        let presets = store.list().unwrap();
        assert_eq!(presets.len(), 6, "5 built-ins plus the new user preset");
        let found = presets.iter().find(|p| p.trigger == "standup").unwrap();
        assert_eq!(found.direction, "Reformat into Yesterday / Today / Blockers.");
        assert!(!found.builtin);
    }

    #[test]
    fn save_is_case_insensitive_and_trims_the_trigger() {
        let store = new_store();
        store
            .save("  ExecSummary  ", "3-bullet TL;DR.", None, None, None, &[])
            .unwrap();

        assert!(store.resolve("execsummary").unwrap().is_some());
    }

    #[test]
    fn save_over_a_builtin_trigger_creates_an_override_and_warns_via_the_flag() {
        let store = new_store();

        let overridden = store
            .save("formal", "Formal, but keep one light joke.", None, None, None, &[])
            .unwrap();
        assert!(overridden.builtin);
        assert!(overridden.overridden, "override flag is the C8 warn data");
        assert_eq!(overridden.direction, "Formal, but keep one light joke.");

        let presets = store.list().unwrap();
        assert_eq!(presets.len(), 5, "override replaces, doesn't add to, the built-in");
        let formal = presets.iter().find(|p| p.trigger == "formal").unwrap();
        assert_eq!(formal.direction, "Formal, but keep one light joke.");
        assert!(formal.overridden);
    }

    #[test]
    fn delete_removes_a_user_preset() {
        let store = new_store();
        store.save("standup", "direction", None, None, None, &[]).unwrap();

        store.delete("standup").unwrap();

        assert_eq!(store.list().unwrap().len(), 5, "back to just the built-ins");
        assert!(store.resolve("standup").unwrap().is_none());
    }

    #[test]
    fn delete_errs_for_an_unmodified_builtin_or_unknown_trigger() {
        let store = new_store();
        assert!(store.delete("formal").is_err(), "nothing user-saved to delete");
        assert!(store.delete("no-such-trigger").is_err());
    }

    #[test]
    fn duplicate_copies_a_builtin_under_a_new_trigger_without_changing_the_original() {
        let store = new_store();

        let copy = store.duplicate("formal", "formal-de").unwrap();
        assert_eq!(copy.trigger, "formal-de");
        assert_eq!(
            copy.direction,
            "Rewrite formally and professionally; no slang or emoji; keep meaning."
        );
        assert!(!copy.builtin, "the duplicate is a plain user preset");

        // The original built-in is untouched.
        let formal = store.resolve("formal").unwrap().unwrap();
        assert!(formal.builtin);
        assert!(!formal.overridden);
    }

    #[test]
    fn duplicate_errs_when_the_new_trigger_already_has_a_user_row() {
        let store = new_store();
        store.save("standup", "direction", None, None, None, &[]).unwrap();

        assert!(store.duplicate("formal", "standup").is_err());
    }

    #[test]
    fn reset_default_after_override_restores_the_shipped_direction() {
        let store = new_store();
        store
            .save("concise", "overridden direction", None, None, None, &[])
            .unwrap();
        assert!(store.resolve("concise").unwrap().unwrap().overridden);

        store.reset_default("concise").unwrap();

        let restored = store.resolve("concise").unwrap().unwrap();
        assert!(!restored.overridden);
        assert_eq!(restored.direction, "Tighten: cut filler, shorten, keep the point.");
    }

    #[test]
    fn reset_default_errs_for_a_non_builtin_or_an_unoverridden_builtin() {
        let store = new_store();
        store.save("standup", "direction", None, None, None, &[]).unwrap();

        assert!(store.reset_default("standup").is_err(), "not a built-in trigger");
        assert!(
            store.reset_default("formal").is_err(),
            "built-in but never overridden"
        );
    }

    // ---- resolve ----

    #[test]
    fn resolve_returns_none_for_an_unknown_trigger() {
        let store = new_store();
        assert!(store.resolve("does-not-exist").unwrap().is_none());
    }

    #[test]
    fn resolve_returns_the_builtin_direction_by_default() {
        let store = new_store();
        let resolved = store.resolve("bullets").unwrap().unwrap();
        assert_eq!(
            resolved.direction,
            "Restructure into clear, scannable bullet points."
        );
    }

    #[test]
    fn resolve_returns_full_overrides_for_a_saved_preset() {
        let store = new_store();
        store
            .save(
                "reply-de",
                "Reply in German, warm but concise.",
                Some("claude-sonnet-4-6"),
                Some("de"),
                Some("review"),
                &[PresetExample {
                    before: "thanks, sounds good".to_string(),
                    after: "Danke, klingt gut".to_string(),
                }],
            )
            .unwrap();

        let resolved = store.resolve("reply-de").unwrap().unwrap();
        assert_eq!(resolved.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(resolved.lang.as_deref(), Some("de"));
        assert_eq!(resolved.inject.as_deref(), Some("review"));
        assert_eq!(resolved.examples.len(), 1);
    }

    // ---- Step 3: import/export + conflict detection ----

    #[test]
    fn export_then_import_into_a_fresh_store_round_trips() {
        let store = new_store();
        store
            .save("standup", "Yesterday / Today / Blockers.", None, None, None, &[])
            .unwrap();
        store
            .save("exec-summary", "3-bullet TL;DR.", None, None, None, &[])
            .unwrap();
        let exported = store.export().unwrap();

        let fresh = new_store();
        let result = fresh.import(&exported).unwrap();

        assert_eq!(result.imported.len(), 2);
        assert!(result.conflicts.is_empty(), "a fresh store has no prior triggers");
        assert_eq!(
            fresh.resolve("standup").unwrap().unwrap().direction,
            "Yesterday / Today / Blockers."
        );
        assert_eq!(
            fresh.resolve("exec-summary").unwrap().unwrap().direction,
            "3-bullet TL;DR."
        );
    }

    #[test]
    fn import_flags_a_conflicting_trigger_but_still_applies_it() {
        let store = new_store();
        store
            .save("standup", "original direction", None, None, None, &[])
            .unwrap();

        let incoming = serde_json::to_string(&vec![
            Preset {
                trigger: "standup".to_string(),
                direction: "imported direction".to_string(),
                model: None,
                lang: None,
                inject: None,
                examples: Vec::new(),
                builtin: false,
                overridden: false,
            },
            Preset {
                trigger: "formal".to_string(),
                direction: "imported override of a built-in".to_string(),
                model: None,
                lang: None,
                inject: None,
                examples: Vec::new(),
                builtin: false,
                overridden: false,
            },
        ])
        .unwrap();

        let result = store.import(&incoming).unwrap();

        assert_eq!(result.imported.len(), 2);
        assert_eq!(
            result.conflicts,
            vec!["standup".to_string(), "formal".to_string()]
        );
        assert_eq!(
            store.resolve("standup").unwrap().unwrap().direction,
            "imported direction"
        );
        assert_eq!(
            store.resolve("formal").unwrap().unwrap().direction,
            "imported override of a built-in"
        );
    }

    #[test]
    fn import_of_a_brand_new_trigger_reports_no_conflict() {
        let store = new_store();
        let incoming = serde_json::to_string(&vec![Preset {
            trigger: "brand-new".to_string(),
            direction: "some direction".to_string(),
            model: None,
            lang: None,
            inject: None,
            examples: Vec::new(),
            builtin: false,
            overridden: false,
        }])
        .unwrap();

        let result = store.import(&incoming).unwrap();

        assert_eq!(result.imported, vec!["brand-new".to_string()]);
        assert!(result.conflicts.is_empty());
    }
}
