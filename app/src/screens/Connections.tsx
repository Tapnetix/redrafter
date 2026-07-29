'use client';

// Connections settings screen (wireframes/connections.html, controls/connections.json).
// The sole Phase-B owner of this file (B7): the connection list, the add/edit
// sheet (per-provider fields, test, save, remove), and the discovered-model
// curation checklist with its manual model-id fallback. B7b
// (app/src-tauri/src/connections.rs) built the backend this calls through
// `@/lib/ipc`; B23 registers those commands (`connection_edit`/`remove`/
// `test`/`refresh_models`, `model_add_manual`) in the Tauri invoke handler —
// until then, calling them against a real backend rejects, exactly like
// `hotkey-change` before C6 wires it (see General.tsx).
//
// Two controls render here for later tasks to drive, per B7's plan:
//   - `key-storage`/`key-storage-encrypted`/`key-storage-keychain`: calls
//     `secretsSet`; the real `secrets.rs` backend is B10's job (S21).
//   - `conn-model-star-*`: a local, visual-only favorite toggle (no backend
//     call) — the real cross-provider favorite/active-model semantics are
//     Models.tsx's job (B8, S25); this mirrors General.tsx's inert
//     `hotkey-change` pattern until then.
import { useEffect, useState } from 'react';
import {
  connectionAdd,
  connectionEdit,
  connectionList,
  connectionRefreshModels,
  connectionRemove,
  connectionTest,
  modelAddManual,
  openExternal,
  secretsSet,
  type Connection,
  type KeyStorageLocation,
} from '@/lib/ipc';

/** Renders a thrown/rejected IPC value as something showable. */
function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

interface ProviderTypeInfo {
  id: string;
  label: string;
  baseUrl: string;
  keyUrl: string;
  needsKey: boolean;
}

// Order matches the wireframe's `conn-provider-type` <select>. Base URLs
// mirror FirstRun.tsx's CLOUD_PROVIDERS + OLLAMA_BASE_URL so the same
// provider kind always maps to the same default endpoint across screens.
const PROVIDER_TYPES: ProviderTypeInfo[] = [
  {
    id: 'anthropic',
    label: 'Anthropic',
    baseUrl: 'https://api.anthropic.com',
    keyUrl: 'https://console.anthropic.com/settings/keys',
    needsKey: true,
  },
  {
    id: 'openai',
    label: 'OpenAI',
    baseUrl: 'https://api.openai.com',
    keyUrl: 'https://platform.openai.com/api-keys',
    needsKey: true,
  },
  {
    id: 'gemini',
    label: 'Google Gemini',
    baseUrl: 'https://generativelanguage.googleapis.com',
    keyUrl: 'https://aistudio.google.com/apikey',
    needsKey: true,
  },
  {
    id: 'openai-compat',
    label: 'OpenAI-compatible',
    baseUrl: 'http://localhost:8080',
    keyUrl: 'https://platform.openai.com/api-keys',
    needsKey: true,
  },
  {
    id: 'ollama',
    label: 'Ollama',
    baseUrl: 'http://localhost:11434',
    keyUrl: '',
    needsKey: false,
  },
];

function providerInfo(id: string): ProviderTypeInfo {
  return PROVIDER_TYPES.find((p) => p.id === id) ?? PROVIDER_TYPES[0];
}

type ModalMode = 'add' | 'edit';
type TestStatus = 'idle' | 'testing' | 'ok' | 'error';

export interface ConnectionsProps {
  /** Called when the user follows the "Models" link to go curate/enable
   * discovered models there instead (B8's Models screen). */
  onNavigateToModels?: () => void;
}

export default function Connections({ onNavigateToModels }: ConnectionsProps) {
  const [connections, setConnections] = useState<Connection[]>([]);
  const [rowStatus, setRowStatus] = useState<Record<string, { status: TestStatus; message?: string }>>({});
  const [lastEnabledByConn, setLastEnabledByConn] = useState<Record<string, string[]>>({});

  const [rowRefreshed, setRowRefreshed] = useState<Record<string, string>>({});

  const [modalOpen, setModalOpen] = useState(false);
  const [modalMode, setModalMode] = useState<ModalMode>('add');
  // Feedback for the sheet's own actions. Every mutating call used to run
  // bare: a rejection vanished into an unhandled promise and a *success*
  // rendered nothing at all, so "Save changes" and the model checkboxes both
  // looked like dead controls.
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved'>('idle');
  const [sheetError, setSheetError] = useState('');
  const [providerType, setProviderType] = useState<string>(PROVIDER_TYPES[0].id);
  const [baseUrl, setBaseUrl] = useState<string>(PROVIDER_TYPES[0].baseUrl);
  const [apiKey, setApiKey] = useState('');
  const [testStatus, setTestStatus] = useState<TestStatus>('idle');
  const [testError, setTestError] = useState('');
  const [savedConnection, setSavedConnection] = useState<Connection | null>(null);
  const [manualRequired, setManualRequired] = useState(false);
  const [manualReason, setManualReason] = useState('');
  const [manualModelId, setManualModelId] = useState('');
  const [favorites, setFavorites] = useState<Record<string, boolean>>({});
  const [removeTarget, setRemoveTarget] = useState<Connection | null>(null);
  const [keyStorage, setKeyStorage] = useState<KeyStorageLocation>('encrypted_file');

  const loadConnections = () => {
    connectionList()
      .then(setConnections)
      .catch(() => setConnections([]));
  };

  useEffect(() => {
    loadConnections();
  }, []);

  function updateConnectionInList(updated: Connection) {
    setConnections((prev) => {
      const existing = prev.some((c) => c.id === updated.id);
      return existing ? prev.map((c) => (c.id === updated.id ? updated : c)) : [...prev, updated];
    });
  }

  function resetModalState() {
    setProviderType(PROVIDER_TYPES[0].id);
    setBaseUrl(PROVIDER_TYPES[0].baseUrl);
    setApiKey('');
    setTestStatus('idle');
    setTestError('');
    setSavedConnection(null);
    setManualRequired(false);
    setManualReason('');
    setManualModelId('');
    setSaveState('idle');
    setSheetError('');
  }

  function openAdd() {
    resetModalState();
    setModalMode('add');
    setModalOpen(true);
  }

  function openEdit(connection: Connection) {
    setModalMode('edit');
    setProviderType(connection.providerKind);
    setBaseUrl(connection.baseUrl);
    setApiKey('');
    setTestStatus('idle');
    setTestError('');
    setSavedConnection(connection);
    setManualRequired(false);
    setManualReason('');
    setManualModelId('');
    setSaveState('idle');
    setSheetError('');
    setModalOpen(true);
  }

  function closeModal() {
    setModalOpen(false);
  }

  function onProviderTypeChange(id: string) {
    setProviderType(id);
    setBaseUrl(providerInfo(id).baseUrl);
    setTestStatus('idle');
  }

  async function runTest() {
    setTestStatus('testing');
    setTestError('');
    try {
      await connectionTest({ providerKind: providerType, baseUrl, apiKey });
      setTestStatus('ok');
    } catch (err) {
      setTestStatus('error');
      setTestError(err instanceof Error ? err.message : String(err));
    }
  }

  async function discoverModels(id: string) {
    const result = await connectionRefreshModels(id);
    if (result.status === 'discovered') {
      setManualRequired(false);
      setManualReason('');
      setSavedConnection(result.connection);
      updateConnectionInList(result.connection);
    } else {
      setManualRequired(true);
      setManualReason(result.reason);
    }
  }

  async function runSave() {
    setSheetError('');
    setSaveState('saving');
    try {
      if (savedConnection) {
        // Already persisted (either opened via Edit, or just added in this
        // same modal session): a plain edit of the base URL/key.
        const updated = await connectionEdit({
          id: savedConnection.id,
          baseUrl,
          ...(apiKey ? { apiKey } : {}),
        });
        setSavedConnection(updated);
        updateConnectionInList(updated);
        setSaveState('saved');
        // An *edit* is finished when it's saved, so close the sheet — leaving
        // it open with no acknowledgement is what made "Save changes" look
        // like it did nothing. An *add* deliberately stays open: the freshly
        // discovered model checklist below is the next step of that flow.
        if (modalMode === 'edit') {
          setModalOpen(false);
        }
        return;
      }

      const added = await connectionAdd({ providerKind: providerType, baseUrl, apiKey });
      setSavedConnection(added);
      updateConnectionInList(added);
      await discoverModels(added.id);
      setSaveState('saved');
    } catch (err) {
      setSaveState('idle');
      setSheetError(errorText(err));
    }
  }

  async function toggleModelEnabled(modelId: string, checked: boolean) {
    if (!savedConnection) return;
    const current = savedConnection.enabledModels;
    const next = checked ? [...current, modelId] : current.filter((m) => m !== modelId);
    setSheetError('');
    try {
      const updated = await connectionEdit({ id: savedConnection.id, enabledModels: next });
      setSavedConnection(updated);
      updateConnectionInList(updated);
    } catch (err) {
      // Without this the checkbox silently snapped back with no explanation.
      setSheetError(`Could not update enabled models: ${errorText(err)}`);
    }
  }

  function openKeyUrl(url: string) {
    // `openExternal` rather than the anchor's own navigation: inside a Tauri
    // webview a `target="_blank"` href is inert (no tab to open, and the
    // webview won't leave its origin), so this link did nothing at all.
    void openExternal(url).catch((err) => setSheetError(`Could not open ${url}: ${errorText(err)}`));
  }

  function toggleFavorite(modelId: string) {
    setFavorites((prev) => ({ ...prev, [modelId]: !prev[modelId] }));
  }

  async function addManualModel() {
    if (!savedConnection || !manualModelId.trim()) return;
    setSheetError('');
    try {
      const updated = await modelAddManual(savedConnection.id, manualModelId);
      setSavedConnection(updated);
      updateConnectionInList(updated);
      setManualModelId('');
      setManualRequired(false);
      setManualReason('');
    } catch (err) {
      // Leave the typed id in place so the user can correct and retry, but
      // say why it didn't take.
      setSheetError(`Could not add "${manualModelId}": ${errorText(err)}`);
    }
  }

  function confirmRemove(connection: Connection) {
    setModalOpen(false);
    setRemoveTarget(connection);
  }

  async function removeConnection() {
    if (!removeTarget) return;
    await connectionRemove(removeTarget.id);
    setConnections((prev) => prev.filter((c) => c.id !== removeTarget.id));
    setRemoveTarget(null);
  }

  async function toggleConnectionEnabled(connection: Connection) {
    const isEnabled = connection.enabledModels.length > 0;
    try {
      if (isEnabled) {
        setLastEnabledByConn((prev) => ({ ...prev, [connection.id]: connection.enabledModels }));
        const updated = await connectionEdit({ id: connection.id, enabledModels: [] });
        updateConnectionInList(updated);
      } else {
        const restore = lastEnabledByConn[connection.id] ?? connection.availableModels.slice(0, 1);
        const updated = await connectionEdit({ id: connection.id, enabledModels: restore });
        updateConnectionInList(updated);
      }
    } catch (err) {
      setRowStatus((prev) => ({
        ...prev,
        [connection.id]: { status: 'error', message: errorText(err) },
      }));
    }
  }

  async function testRow(connection: Connection) {
    setRowStatus((prev) => ({ ...prev, [connection.id]: { status: 'testing' } }));
    try {
      // The stored API key never crosses back to the frontend (see
      // `Connection.keyRef`'s doc), so a row-level re-test can only re-check
      // reachability with whatever key was in the form — for a keyless
      // connection (e.g. Ollama) this fully verifies auth; for a keyed one
      // it can only confirm the endpoint responds. Re-entering a key via
      // Edit + Test is the reliable path for the latter.
      await connectionTest({ providerKind: connection.providerKind, baseUrl: connection.baseUrl, apiKey: '' });
      setRowStatus((prev) => ({ ...prev, [connection.id]: { status: 'ok' } }));
    } catch (err) {
      setRowStatus((prev) => ({
        ...prev,
        [connection.id]: { status: 'error', message: err instanceof Error ? err.message : String(err) },
      }));
    }
  }

  async function refreshRow(connection: Connection) {
    setRowRefreshed((prev) => {
      const { [connection.id]: _dropped, ...rest } = prev;
      return rest;
    });
    const result = await connectionRefreshModels(connection.id);
    if (result.status === 'discovered') {
      updateConnectionInList(result.connection);
      // Say what discovery found. Previously the only trace was the row's
      // "N models" counter quietly changing, so a refresh that found the same
      // models looked like the button did nothing.
      setRowRefreshed((prev) => ({
        ...prev,
        [connection.id]: `${result.connection.availableModels.length} models found`,
      }));
    } else {
      // No list endpoint (or discovery failed): open Edit so the user can
      // fall back to the manual model-id control.
      openEdit(connection);
      setManualRequired(true);
      setManualReason(result.reason);
    }
  }

  async function chooseKeyStorage(location: KeyStorageLocation) {
    setKeyStorage(location);
    try {
      await secretsSet(location);
    } catch {
      // secrets_set's backend lands in B10; nothing more to do here yet.
    }
  }

  const info = providerInfo(providerType);
  const isEditingSaved = savedConnection != null;

  return (
    <div className="settings" data-testid="connections-screen">
      <section className="sec">
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 8 }}>
          <button className="btn btn--primary btn--sm" data-testid="add-connection" onClick={openAdd}>
            + Add connection
          </button>
        </div>

        {connections.length === 0 ? (
          <div className="grp">
            <div className="empty" data-testid="connections-empty">
              <div className="empty__icon" aria-hidden="true">
                ⎋
              </div>
              <p style={{ fontSize: 16, color: 'var(--text)', margin: '0 0 6px' }}>
                No providers connected yet
              </p>
              <p className="muted" style={{ margin: '0 0 16px' }}>
                Connect a cloud provider or your local Ollama to start refining text.
              </p>
              <button className="btn btn--primary" data-testid="connections-empty-cta" onClick={openAdd}>
                Add your first AI provider
              </button>
            </div>
          </div>
        ) : (
          <>
            <p className="muted tiny" style={{ margin: '0 0 8px' }}>
              A connection is a provider + base URL + key. redrafter auto-discovers models from each one;
              enable the ones you want in{' '}
              <button
                type="button"
                data-testid="connections-models-link"
                onClick={() => onNavigateToModels?.()}
                style={{
                  color: 'var(--primary)',
                  background: 'none',
                  border: 'none',
                  padding: 0,
                  font: 'inherit',
                  cursor: 'pointer',
                }}
              >
                Models
              </button>
              .
            </p>
            <div className="grp list" data-testid="connections-list">
              {connections.map((connection) => {
                const status = rowStatus[connection.id];
                const enabled = connection.enabledModels.length > 0;
                return (
                  <div className="prov" key={connection.id} data-testid={`connection-row-${connection.providerKind}`}>
                    <span
                      className={`status-dot ${status?.status === 'error' ? 'red' : enabled ? 'green' : 'amber'}`}
                      aria-hidden="true"
                    />
                    <div className="prov__main">
                      <div className="prov__name">
                        {providerInfo(connection.providerKind).label}
                        {status?.status === 'error' && <span className="chip warn">auth error</span>}
                        {/* Success and in-flight used to render nothing at all,
                            so a passing Test left the row byte-identical — the
                            button looked broken. */}
                        {status?.status === 'testing' && (
                          <span className="chip" data-testid={`connection-test-testing-${connection.providerKind}`}>
                            testing…
                          </span>
                        )}
                        {status?.status === 'ok' && (
                          <span className="chip ok" data-testid={`connection-test-ok-${connection.providerKind}`}>
                            <span className="status-dot green" aria-hidden="true" /> reachable
                          </span>
                        )}
                        {rowRefreshed[connection.id] && (
                          <span className="chip" data-testid={`connection-refreshed-${connection.providerKind}`}>
                            {rowRefreshed[connection.id]}
                          </span>
                        )}
                      </div>
                      <div className="prov__desc mono">
                        {connection.baseUrl} · {connection.availableModels.length} models
                        {connection.keyRef ? ' · key set' : ' · no key needed'}
                      </div>
                      {status?.status === 'error' && status.message && (
                        <div
                          className="tiny"
                          role="status"
                          data-testid={`connection-error-${connection.providerKind}`}
                          style={{ marginTop: 4, color: 'var(--danger, #d0433b)' }}
                        >
                          {status.message}
                        </div>
                      )}
                    </div>
                    <button
                      className="btn btn--sm"
                      data-testid={`connection-test-${connection.providerKind}`}
                      onClick={() => testRow(connection)}
                      disabled={status?.status === 'testing'}
                    >
                      {status?.status === 'testing' ? 'Testing…' : 'Test'}
                    </button>
                    <button
                      className="btn btn--sm"
                      data-testid={`connection-refresh-${connection.providerKind}`}
                      onClick={() => refreshRow(connection)}
                    >
                      Refresh models
                    </button>
                    <button
                      className="btn btn--sm"
                      data-testid={`connection-edit-${connection.providerKind}`}
                      onClick={() => openEdit(connection)}
                    >
                      Edit
                    </button>
                    <label className="switch" title="Enable connection">
                      <input
                        type="checkbox"
                        checked={enabled}
                        aria-label={`${providerInfo(connection.providerKind).label} enabled`}
                        data-testid={`connection-enable-${connection.providerKind}`}
                        onChange={() => toggleConnectionEnabled(connection)}
                      />
                      <span className="track" />
                    </label>
                  </div>
                );
              })}
            </div>
          </>
        )}
      </section>

      {/* KEY STORAGE */}
      <section className="sec">
        <h2 className="sec__title">Key storage</h2>
        <p className="muted tiny" style={{ margin: '0 0 8px' }}>
          Where API keys are kept at rest.
        </p>
        <div className="segmented" role="radiogroup" aria-label="Key storage" data-testid="key-storage">
          <button
            className={keyStorage === 'encrypted_file' ? 'active' : ''}
            role="radio"
            aria-checked={keyStorage === 'encrypted_file'}
            data-testid="key-storage-encrypted"
            onClick={() => chooseKeyStorage('encrypted_file')}
          >
            Encrypted config file
          </button>
          <button
            className={keyStorage === 'keychain' ? 'active' : ''}
            role="radio"
            aria-checked={keyStorage === 'keychain'}
            data-testid="key-storage-keychain"
            onClick={() => chooseKeyStorage('keychain')}
          >
            OS Keychain
          </button>
        </div>
        <p className="muted tiny mono" style={{ marginTop: 10 }}>
          store · ~/.config/redrafter/config.toml
        </p>
      </section>

      {/* ADD / EDIT CONNECTION SHEET */}
      {modalOpen && (
        <div className="modal-back" style={{ display: 'grid' }} role="dialog" aria-modal="true" data-testid="connection-modal">
          <div className="modal" style={{ width: 520 }}>
            <h2 className="modal__title">
              {modalMode === 'add' && !isEditingSaved ? 'Add connection' : 'Edit connection'}
            </h2>

            <div className="field">
              <label htmlFor="conn-provider-type">Provider type</label>
              <select
                className="input"
                id="conn-provider-type"
                data-testid="conn-provider-type"
                value={providerType}
                disabled={isEditingSaved}
                onChange={(e) => onProviderTypeChange(e.target.value)}
              >
                {PROVIDER_TYPES.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </select>
            </div>
            <div className="field">
              <label htmlFor="conn-base-url">Base URL</label>
              <input
                className="input mono"
                id="conn-base-url"
                data-testid="conn-base-url"
                value={baseUrl}
                onChange={(e) => setBaseUrl(e.target.value)}
              />
            </div>
            {info.needsKey && (
              <div className="field">
                <label htmlFor="conn-api-key">API key</label>
                <input
                  className="input mono"
                  id="conn-api-key"
                  type="password"
                  data-testid="conn-api-key"
                  placeholder={isEditingSaved ? 'Leave blank to keep the current key' : 'sk-ant-…'}
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                />
                {info.keyUrl && (
                  <a
                    href={info.keyUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    data-testid="conn-get-key-link"
                    onClick={(e) => {
                      e.preventDefault();
                      openKeyUrl(info.keyUrl);
                    }}
                    style={{ color: 'var(--primary)', fontSize: 12 }}
                  >
                    Get an API key ↗
                  </a>
                )}
              </div>
            )}

            <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 14, flexWrap: 'wrap' }}>
              <button className="btn btn--sm" data-testid="conn-test" onClick={runTest} disabled={testStatus === 'testing'}>
                Test connection
              </button>
              {testStatus === 'ok' && (
                <span className="chip ok" data-testid="conn-test-ok">
                  <span className="status-dot green" /> reachable · auth ok
                </span>
              )}
              {testStatus === 'error' && (
                <span className="chip err" data-testid="conn-test-error">
                  <span className="status-dot red" /> connection failed · {testError || 'check key'}
                </span>
              )}
            </div>

            {isEditingSaved && (
              <div style={{ marginTop: 16 }} data-testid="conn-discovered-list">
                <div style={{ fontWeight: 600, fontSize: 'var(--fs-small)', marginBottom: 6 }}>
                  Discovered models{' '}
                  <span className="muted tiny" style={{ fontWeight: 400 }}>
                    — enable the ones you want; ⭐ favorites show at the top of the tray
                  </span>
                </div>
                <div className="grp" style={{ padding: 8 }}>
                  {savedConnection!.availableModels.map((modelId) => {
                    const checked = savedConnection!.enabledModels.includes(modelId);
                    const starred = !!favorites[modelId];
                    return (
                      <div className="opt-row" style={{ padding: '6px 4px', alignItems: 'center', gap: 10 }} key={modelId}>
                        <input
                          type="checkbox"
                          id={`conn-model-${modelId}`}
                          data-testid={`conn-model-check-${modelId}`}
                          checked={checked}
                          onChange={(e) => toggleModelEnabled(modelId, e.target.checked)}
                        />
                        <label htmlFor={`conn-model-${modelId}`} className="mono" style={{ flex: 1 }}>
                          {modelId}
                        </label>
                        <button
                          className="btn btn--ghost btn--sm"
                          data-testid={`conn-model-star-${modelId}`}
                          aria-pressed={starred}
                          aria-label={`Favorite ${modelId}`}
                          onClick={() => toggleFavorite(modelId)}
                          style={starred ? { color: 'var(--warning)' } : undefined}
                        >
                          {starred ? '★' : '☆'}
                        </button>
                      </div>
                    );
                  })}
                  {savedConnection!.availableModels.length === 0 && (
                    <p className="muted tiny" style={{ margin: 0 }}>
                      {manualRequired
                        ? `No models could be listed automatically${manualReason ? ` (${manualReason})` : ''} — add one by name below.`
                        : 'No models discovered yet.'}
                    </p>
                  )}
                </div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 8 }}>
                  <input
                    className="input mono"
                    aria-label="Add model manually"
                    placeholder="+ Add model manually — e.g. claude-opus-4-6"
                    data-testid="conn-add-model-manual"
                    style={{ flex: 1 }}
                    value={manualModelId}
                    onChange={(e) => setManualModelId(e.target.value)}
                  />
                  <button className="btn btn--sm" data-testid="conn-add-model-manual-add" onClick={addManualModel}>
                    Add
                  </button>
                </div>
                <p className="muted tiny" style={{ margin: '8px 0 0' }}>
                  Discovery uses <span className="mono">GET /v1/models</span>. If a provider has no list
                  endpoint, add models by name above — redrafter never blocks on discovery.
                </p>
              </div>
            )}

            {sheetError && (
              <p
                className="tiny"
                role="alert"
                data-testid="conn-sheet-error"
                style={{ margin: '12px 0 0', color: 'var(--danger, #d0433b)' }}
              >
                {sheetError}
              </p>
            )}

            <div className="modal__foot" style={{ justifyContent: 'space-between' }}>
              {isEditingSaved ? (
                <button
                  className="btn btn--danger btn--sm"
                  data-testid="connection-remove"
                  onClick={() => confirmRemove(savedConnection!)}
                >
                  Remove connection
                </button>
              ) : (
                <span />
              )}
              <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' }}>
                {saveState === 'saved' && (
                  <span className="chip ok" data-testid="connection-save-ok" role="status">
                    <span className="status-dot green" aria-hidden="true" /> saved
                  </span>
                )}
                <button className="btn" data-testid="connection-cancel" onClick={closeModal}>
                  {isEditingSaved ? 'Close' : 'Cancel'}
                </button>
                <button
                  className="btn btn--primary"
                  data-testid="connection-save"
                  onClick={runSave}
                  disabled={saveState === 'saving'}
                >
                  {saveState === 'saving'
                    ? 'Saving…'
                    : isEditingSaved
                      ? 'Save changes'
                      : 'Add connection'}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* REMOVE CONNECTION CONFIRM */}
      {removeTarget && (
        <div className="modal-back" style={{ display: 'grid' }} role="alertdialog" aria-modal="true" data-testid="connection-remove-modal">
          <div className="modal" style={{ width: 400 }}>
            <h2 className="modal__title">Remove connection?</h2>
            <p className="muted" style={{ fontSize: 'var(--fs-small)', margin: 0 }}>
              Remove <span className="mono">{providerInfo(removeTarget.providerKind).label}</span> and its
              stored key? Its models will be disabled. This can&apos;t be undone.
            </p>
            <div className="modal__foot">
              <button className="btn" data-testid="connection-remove-cancel" onClick={() => setRemoveTarget(null)}>
                Cancel
              </button>
              <button
                className="btn btn--primary btn--danger"
                data-testid="connection-remove-confirm"
                onClick={removeConnection}
              >
                Remove connection
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
