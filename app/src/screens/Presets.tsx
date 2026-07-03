'use client';

// Presets settings screen (wireframes/presets.html, controls/presets.json).
// The sole owner of this file (C3): the built-in/user preset list, the
// create/edit form (trigger, direction, model/lang/inject overrides,
// few-shot examples), row actions (duplicate/reset-to-default/delete), and
// the import/export dialogs. C3b (app/src-tauri/src/presets.rs) built the
// backend this calls through `@/lib/ipc`; C17 registers those commands in
// the Tauri invoke handler — until then, calling them against a real
// backend rejects, exactly like `hotkey-change` before C6 wires it (see
// General.tsx).
//
// Editing a built-in in place doesn't mutate the shipped default: it saves
// a user *override* row that shadows it (`Preset.overridden`), so saving
// over a built-in trigger first asks for confirmation (the
// `preset-override-*` modal) — "Duplicate instead" and "Reset to default"
// are the two built-in-only escapes offered alongside that warning.
// Downstream e2e specs drive those controls entirely through this
// already-built surface: C8 (S14, the override warning), C9 (S28, import),
// C10 (S29, export).
import { useEffect, useState } from 'react';
import {
  modelsList,
  presetDelete,
  presetDuplicate,
  presetExport,
  presetImport,
  presetList,
  presetResetDefault,
  presetSave,
  type Preset,
  type PresetExample,
  type PresetImportResult,
} from '@/lib/ipc';

interface PresetForm {
  trigger: string;
  direction: string;
  model: string;
  lang: string;
  inject: string;
  examples: PresetExample[];
}

const INJECT_OPTIONS: { value: string; label: string }[] = [
  { value: '', label: 'Inherit (Blind)' },
  { value: 'blind', label: 'Blind' },
  { value: 'review', label: 'Review & confirm' },
];

function blankForm(): PresetForm {
  return { trigger: '', direction: '', model: '', lang: '', inject: '', examples: [] };
}

function formFromPreset(p: Preset): PresetForm {
  return {
    trigger: `/${p.trigger}`,
    direction: p.direction,
    model: p.model ?? '',
    lang: p.lang ?? '',
    inject: p.inject ?? '',
    examples: p.examples.map((e) => ({ ...e })),
  };
}

/** Strips a leading `/` (any number, defensively) and normalizes whitespace/
 * case exactly like `presets.rs`'s `normalize_trigger`, so the trigger the
 * user typed (with or without the `/`) matches what the backend will store
 * it as. */
function normalizeTriggerInput(raw: string): string {
  return raw.trim().replace(/^\/+/, '').trim().toLowerCase();
}

export default function Presets() {
  const [presets, setPresets] = useState<Preset[]>([]);
  const [modelIds, setModelIds] = useState<string[]>([]);
  const [selectedTrigger, setSelectedTrigger] = useState<string | null>(null);
  const [form, setForm] = useState<PresetForm>(blankForm());
  const [error, setError] = useState('');

  const [overrideModalOpen, setOverrideModalOpen] = useState(false);
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);

  const [importOpen, setImportOpen] = useState(false);
  const [importPasteText, setImportPasteText] = useState('');
  const [importResult, setImportResult] = useState<PresetImportResult | null>(null);
  const [importError, setImportError] = useState('');

  const [exportOpen, setExportOpen] = useState(false);
  const [exportText, setExportText] = useState('');

  async function load(): Promise<Preset[]> {
    try {
      const list = await presetList();
      setPresets(list);
      return list;
    } catch {
      setPresets([]);
      return [];
    }
  }

  useEffect(() => {
    load();
    modelsList()
      .then((result) => setModelIds(Array.from(new Set(result.models.map((m) => m.modelId)))))
      .catch(() => setModelIds([]));
  }, []);

  const selected = presets.find((p) => p.trigger === selectedTrigger) ?? null;
  const isBuiltinSelected = !!selected?.builtin;
  const builtinPresets = presets.filter((p) => p.builtin);
  const userPresets = presets.filter((p) => !p.builtin);

  function selectPreset(p: Preset) {
    setSelectedTrigger(p.trigger);
    setForm(formFromPreset(p));
    setError('');
  }

  function newPreset() {
    setSelectedTrigger(null);
    setForm(blankForm());
    setError('');
  }

  function cancelEdit() {
    if (selected) {
      setForm(formFromPreset(selected));
    } else {
      setForm(blankForm());
    }
    setError('');
  }

  function updateField<K extends keyof PresetForm>(key: K, value: PresetForm[K]) {
    setForm((f) => ({ ...f, [key]: value }));
  }

  function addExample() {
    setForm((f) => ({ ...f, examples: [...f.examples, { before: '', after: '' }] }));
  }

  function updateExample(index: number, field: keyof PresetExample, value: string) {
    setForm((f) => ({
      ...f,
      examples: f.examples.map((e, i) => (i === index ? { ...e, [field]: value } : e)),
    }));
  }

  function removeExample(index: number) {
    setForm((f) => ({ ...f, examples: f.examples.filter((_, i) => i !== index) }));
  }

  async function commitSave() {
    const trigger = normalizeTriggerInput(form.trigger);
    try {
      const saved = await presetSave({
        trigger,
        direction: form.direction,
        model: form.model || undefined,
        lang: form.lang || undefined,
        inject: form.inject || undefined,
        examples: form.examples,
      });
      await load();
      setSelectedTrigger(saved.trigger);
      setForm(formFromPreset(saved));
      setError('');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setOverrideModalOpen(false);
    }
  }

  function requestSave() {
    const trigger = normalizeTriggerInput(form.trigger);
    if (!trigger) {
      setError('Name / trigger must not be empty');
      return;
    }
    const overridesBuiltin = presets.some((p) => p.trigger === trigger && p.builtin);
    if (overridesBuiltin) {
      setOverrideModalOpen(true);
      return;
    }
    void commitSave();
  }

  async function duplicatePreset() {
    if (!selected) return;
    const existingTriggers = new Set(presets.map((p) => p.trigger));
    let candidate = `${selected.trigger}-copy`;
    let n = 2;
    while (existingTriggers.has(candidate)) {
      candidate = `${selected.trigger}-copy-${n++}`;
    }
    try {
      const created = await presetDuplicate(selected.trigger, candidate);
      await load();
      setSelectedTrigger(created.trigger);
      setForm(formFromPreset(created));
      setError('');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function resetToDefault() {
    if (!selected) return;
    try {
      await presetResetDefault(selected.trigger);
      const list = await load();
      const restored = list.find((p) => p.trigger === selected.trigger);
      if (restored) {
        setSelectedTrigger(restored.trigger);
        setForm(formFromPreset(restored));
      }
      setError('');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function confirmDelete() {
    if (!selected) return;
    try {
      await presetDelete(selected.trigger);
      await load();
      newPreset();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setDeleteModalOpen(false);
    }
  }

  function openImport() {
    setImportOpen(true);
    setImportPasteText('');
    setImportResult(null);
    setImportError('');
  }

  function closeImport() {
    setImportOpen(false);
  }

  async function handleImportFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    try {
      const text = await file.text();
      const result = await presetImport(text);
      setImportResult(result);
      setImportError('');
      await load();
    } catch (err) {
      setImportError(err instanceof Error ? err.message : String(err));
    }
  }

  async function confirmImport() {
    try {
      const result = await presetImport(importPasteText);
      setImportResult(result);
      setImportError('');
      await load();
    } catch (err) {
      setImportError(err instanceof Error ? err.message : String(err));
    }
  }

  async function openExport() {
    setExportOpen(true);
    try {
      setExportText(await presetExport());
    } catch {
      setExportText('');
    }
  }

  function closeExport() {
    setExportOpen(false);
  }

  async function copyExport() {
    try {
      const text = await presetExport();
      setExportText(text);
      if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
      }
    } catch {
      // Best-effort: nothing more to do if the export/clipboard call fails.
    }
  }

  return (
    <div className="settings" data-testid="presets-screen">
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginBottom: 8 }}>
        <button className="btn btn--sm" data-testid="preset-import" onClick={openImport}>
          Import
        </button>
        <button className="btn btn--sm" data-testid="preset-export" onClick={openExport}>
          Export
        </button>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '280px 1fr', gap: 16 }}>
        {/* PRESET LIST */}
        <div>
          <button
            className="btn btn--primary btn--sm"
            style={{ width: '100%', justifyContent: 'center', marginBottom: 10 }}
            data-testid="preset-new"
            onClick={newPreset}
          >
            + New preset
          </button>
          <div className="preset-list" role="listbox" aria-label="Presets" data-testid="preset-list">
            <div role="group" aria-label="Built-in presets" data-testid="preset-group-builtin">
              <div className="preset-group-label">Built-in</div>
              {builtinPresets.map((p) => (
                <button
                  key={p.trigger}
                  className="preset-item"
                  role="option"
                  aria-selected={selectedTrigger === p.trigger}
                  data-testid={`preset-item-${p.trigger}`}
                  onClick={() => selectPreset(p)}
                >
                  <span className="trig mono">/{p.trigger}</span>
                  {p.overridden && <span className="chip warn tiny">overridden</span>}
                </button>
              ))}
            </div>
            <div role="group" aria-label="My presets" data-testid="preset-group-mine">
              <div className="preset-group-label">My presets</div>
              {userPresets.map((p) => (
                <button
                  key={p.trigger}
                  className="preset-item"
                  role="option"
                  aria-selected={selectedTrigger === p.trigger}
                  data-testid={`preset-item-${p.trigger}`}
                  onClick={() => selectPreset(p)}
                >
                  <span className="trig mono">/{p.trigger}</span>
                </button>
              ))}
              {userPresets.length === 0 && (
                <p className="muted tiny" style={{ margin: '4px 8px' }}>
                  No presets of your own yet.
                </p>
              )}
            </div>
          </div>
        </div>

        {/* PRESET EDITOR */}
        <section className="card" data-testid="preset-editor" style={{ padding: 18 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 16 }}>
            <h2 className="sec__title" style={{ margin: 0 }}>
              {selected ? (
                <>
                  Editing <span className="mono">/{selected.trigger}</span>
                </>
              ) : (
                'New preset'
              )}
            </h2>
            {isBuiltinSelected && (
              <span className="chip" data-testid="preset-builtin-badge">
                Built-in
              </span>
            )}
          </div>

          {isBuiltinSelected && (
            <div className="preset-warning" data-testid="preset-builtin-warning" style={{ marginBottom: 14 }}>
              <div style={{ fontSize: 'var(--fs-small)' }}>
                This is a built-in preset. Saving changes creates an override — you can reset it to the default
                anytime.
              </div>
              <div style={{ display: 'flex', gap: 8, marginTop: 8, flexWrap: 'wrap' }}>
                <button className="btn btn--sm" data-testid="preset-duplicate" onClick={duplicatePreset}>
                  Duplicate instead
                </button>
                <button className="btn btn--sm" data-testid="preset-reset-default" onClick={resetToDefault}>
                  Reset to default
                </button>
              </div>
            </div>
          )}

          {error && (
            <div className="chip err" data-testid="preset-save-error" style={{ marginBottom: 14 }}>
              {error}
            </div>
          )}

          <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 14 }}>
              <div className="field">
                <label htmlFor="preset-name">Name / trigger</label>
                <input
                  className="input mono"
                  id="preset-name"
                  data-testid="preset-name"
                  value={form.trigger}
                  onChange={(e) => updateField('trigger', e.target.value)}
                />
              </div>
              <div className="field">
                <label htmlFor="preset-model">Model override</label>
                <select
                  className="input mono"
                  id="preset-model"
                  data-testid="preset-model"
                  value={form.model}
                  onChange={(e) => updateField('model', e.target.value)}
                >
                  <option value="">Inherit (active model)</option>
                  {/* A preset can pin a model that's since been disabled/removed
                      (the wireframe's "stale pin" state) — include it so the
                      select still shows the right value instead of silently
                      falling back to the first option. */}
                  {form.model && !modelIds.includes(form.model) && (
                    <option value={form.model}>{form.model}</option>
                  )}
                  {modelIds.map((id) => (
                    <option key={id} value={id}>
                      {id}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            <div className="field">
              <label htmlFor="preset-direction">Direction</label>
              <textarea
                className="textarea"
                id="preset-direction"
                data-testid="preset-direction"
                rows={3}
                value={form.direction}
                onChange={(e) => updateField('direction', e.target.value)}
              />
            </div>

            <div className="grp" style={{ padding: 14 }}>
              <div style={{ fontWeight: 600, fontSize: 'var(--fs-small)', marginBottom: 10 }}>Overrides</div>
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 14 }}>
                <div className="field">
                  <label htmlFor="preset-lang">Language</label>
                  <input
                    className="input mono"
                    id="preset-lang"
                    data-testid="preset-lang"
                    value={form.lang}
                    onChange={(e) => updateField('lang', e.target.value)}
                  />
                </div>
                <div className="field">
                  <label htmlFor="preset-inject">Inject behavior</label>
                  <select
                    className="input"
                    id="preset-inject"
                    data-testid="preset-inject"
                    value={form.inject}
                    onChange={(e) => updateField('inject', e.target.value)}
                  >
                    {INJECT_OPTIONS.map((opt) => (
                      <option key={opt.value} value={opt.value}>
                        {opt.label}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
            </div>

            {/* FEW-SHOT EXAMPLES */}
            <div data-testid="preset-examples">
              <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 10 }}>
                <div style={{ fontWeight: 600, fontSize: 'var(--fs-small)' }}>Few-shot examples</div>
                <span className="muted tiny">before → after pairs teach the tone</span>
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                {form.examples.map((example, index) => (
                  <div className="example-pair" data-testid={`preset-example-${index + 1}`} key={index}>
                    <div className="field">
                      <label htmlFor={`preset-example-before-${index + 1}`}>Before</label>
                      <textarea
                        className="textarea"
                        id={`preset-example-before-${index + 1}`}
                        data-testid={`preset-example-before-${index + 1}`}
                        rows={2}
                        value={example.before}
                        onChange={(e) => updateExample(index, 'before', e.target.value)}
                      />
                    </div>
                    <div className="field">
                      <label htmlFor={`preset-example-after-${index + 1}`}>After</label>
                      <textarea
                        className="textarea"
                        id={`preset-example-after-${index + 1}`}
                        data-testid={`preset-example-after-${index + 1}`}
                        rows={2}
                        value={example.after}
                        onChange={(e) => updateExample(index, 'after', e.target.value)}
                      />
                    </div>
                    <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
                      <button
                        type="button"
                        className="btn btn--ghost btn--sm"
                        data-testid={`preset-example-remove-${index + 1}`}
                        onClick={() => removeExample(index)}
                      >
                        Remove
                      </button>
                    </div>
                  </div>
                ))}
              </div>
              <button
                type="button"
                className="btn btn--ghost btn--sm"
                style={{ marginTop: 10 }}
                data-testid="preset-add-example"
                onClick={addExample}
              >
                + Add example
              </button>
            </div>

            <hr className="div" />
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <button
                className="btn btn--danger btn--sm"
                data-testid="preset-delete"
                disabled={!selected}
                onClick={() => setDeleteModalOpen(true)}
              >
                Delete preset
              </button>
              <div style={{ flex: 1 }} />
              <button className="btn" data-testid="preset-cancel" onClick={cancelEdit}>
                Cancel
              </button>
              <button className="btn btn--primary" data-testid="preset-save" onClick={requestSave}>
                Save preset
              </button>
            </div>
          </div>
        </section>
      </div>

      {/* IMPORT MODAL */}
      {importOpen && (
        <div className="modal-back" style={{ display: 'grid' }} role="dialog" aria-modal="true" data-testid="import-modal">
          <div className="modal">
            <h2 className="modal__title">Import presets</h2>
            <p className="muted" style={{ fontSize: 'var(--fs-small)', margin: '0 0 14px' }}>
              Choose a presets file or paste its contents. Matching triggers are merged (and flagged as
              conflicts).
            </p>
            <label className="btn btn--sm" htmlFor="preset-import-file-input">
              Choose file…
            </label>
            <input
              type="file"
              id="preset-import-file-input"
              data-testid="preset-import-file"
              accept=".json,.toml,.txt"
              style={{ display: 'none' }}
              onChange={handleImportFile}
            />
            <div className="field" style={{ marginTop: 14 }}>
              <label htmlFor="import-paste">Or paste JSON</label>
              <textarea
                className="textarea mono"
                id="import-paste"
                data-testid="preset-import-paste"
                rows={4}
                value={importPasteText}
                onChange={(e) => setImportPasteText(e.target.value)}
              />
            </div>
            {importError && (
              <div className="chip err" data-testid="preset-import-error" style={{ marginTop: 8 }}>
                {importError}
              </div>
            )}
            {importResult && (
              <div
                className={importResult.conflicts.length > 0 ? 'chip warn' : 'chip ok'}
                data-testid="preset-import-result"
                style={{ marginTop: 8 }}
              >
                Imported {importResult.imported.length} · {importResult.conflicts.length} conflicts
              </div>
            )}
            <div className="modal__foot">
              <button className="btn" data-testid="preset-import-cancel" onClick={closeImport}>
                Cancel
              </button>
              <button className="btn btn--primary" data-testid="preset-import-confirm" onClick={confirmImport}>
                Import
              </button>
            </div>
          </div>
        </div>
      )}

      {/* EXPORT MODAL */}
      {exportOpen && (
        <div className="modal-back" style={{ display: 'grid' }} role="dialog" aria-modal="true" data-testid="export-modal">
          <div className="modal" style={{ width: 520 }}>
            <h2 className="modal__title">Export presets</h2>
            <p className="muted" style={{ fontSize: 'var(--fs-small)', margin: '0 0 12px' }}>
              Copy your saved presets to share them across machines.
            </p>
            <pre className="codeblock mono" data-testid="preset-export-text">
              {exportText}
            </pre>
            <div className="modal__foot">
              <button className="btn" data-testid="preset-export-close" onClick={closeExport}>
                Close
              </button>
              <button className="btn btn--primary" data-testid="preset-export-copy" onClick={copyExport}>
                Copy to clipboard
              </button>
            </div>
          </div>
        </div>
      )}

      {/* DELETE CONFIRM */}
      {deleteModalOpen && selected && (
        <div className="modal-back" style={{ display: 'grid' }} role="alertdialog" aria-modal="true" data-testid="delete-preset-modal">
          <div className="modal" style={{ width: 400 }}>
            <h2 className="modal__title">Delete preset?</h2>
            <p className="muted" style={{ fontSize: 'var(--fs-small)', margin: 0 }}>
              Delete <span className="mono">/{selected.trigger}</span>? This can&apos;t be undone.
            </p>
            <div className="modal__foot">
              <button className="btn" data-testid="preset-delete-cancel" onClick={() => setDeleteModalOpen(false)}>
                Cancel
              </button>
              <button className="btn btn--primary btn--danger" data-testid="preset-delete-confirm" onClick={confirmDelete}>
                Delete preset
              </button>
            </div>
          </div>
        </div>
      )}

      {/* OVERRIDE BUILT-IN CONFIRM */}
      {overrideModalOpen && (
        <div className="modal-back" style={{ display: 'grid' }} role="alertdialog" aria-modal="true" data-testid="preset-override-modal">
          <div className="modal" style={{ width: 420 }}>
            <h2 className="modal__title">Modify built-in preset?</h2>
            <p className="muted" style={{ fontSize: 'var(--fs-small)', margin: 0 }}>
              Your changes to <span className="mono">/{normalizeTriggerInput(form.trigger)}</span> will override
              the default. You can reset it later.
            </p>
            <div className="modal__foot">
              <button className="btn" data-testid="preset-override-cancel" onClick={() => setOverrideModalOpen(false)}>
                Cancel
              </button>
              <button className="btn btn--primary" data-testid="preset-override-confirm" onClick={commitSave}>
                Override
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
