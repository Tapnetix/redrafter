'use client';

// Models settings screen (wireframes/models.html, controls/models.json).
// The sole owner of this file (B8): the cross-connection curated-model
// table (every enabled model, from every connection, in one flat list),
// picking the single global active model, per-model disable, favorite
// star, and the Ollama "get more models" pull affordance. B8 also built the
// backend this calls through `@/lib/ipc` (`app/src-tauri/src/models.rs`);
// B23 registers those commands in the Tauri invoke handler — until then,
// calling them against a real backend rejects, exactly like `hotkey-change`
// before C6 wires it (see General.tsx) or `secrets_set` before B10
// (Connections.tsx).
//
// Enabling a *newly discovered* model happens on the Connections screen
// (B7) — this screen only curates what's already enabled: setting one
// active, disabling it, or starring it. `models-connections-link` sends the
// user there to enable more.
//
// Downstream tasks drive scenarios entirely through this already-built
// surface without touching this file: B9/B20 (favorite -> tray quick-switch),
// B19 (remove connection detaches its models), B21 (disable + the
// active-unavailable banner below), B22 (the Ollama pull section below).
import { useEffect, useState } from 'react';
import {
  modelDisable,
  modelSetActive,
  modelToggleFavorite,
  modelsList,
  ollamaPull,
  type CuratedModel,
  type ModelsListResult,
} from '@/lib/ipc';

const EMPTY_RESULT: ModelsListResult = {
  models: [],
  hasActive: false,
  activeUnavailable: false,
  staleActiveModelId: null,
};

type PullState = 'idle' | 'pulling' | 'done' | 'error';

export interface ModelsProps {
  /** Called when the user follows a "Connections" link to enable more
   * discovered models there (B7's Connections screen). */
  onNavigateToConnections?: () => void;
}

export default function Models({ onNavigateToConnections }: ModelsProps) {
  const [result, setResult] = useState<ModelsListResult>(EMPTY_RESULT);
  const [search, setSearch] = useState('');
  const [pullModelId, setPullModelId] = useState('');
  const [pullState, setPullState] = useState<PullState>('idle');
  const [pullMessage, setPullMessage] = useState('');
  // `setActive`/`disable`/`toggleFavorite` used to run bare, so a rejected
  // command left the row untouched with nothing to explain why.
  const [actionError, setActionError] = useState('');

  const load = () => {
    modelsList()
      .then(setResult)
      .catch(() => setResult(EMPTY_RESULT));
  };

  useEffect(() => {
    load();
  }, []);

  /** Runs one curation command, adopting its returned list or reporting why
   * it failed — never leaving the click with no observable outcome. */
  async function runAction(what: string, call: () => Promise<ModelsListResult>) {
    setActionError('');
    try {
      setResult(await call());
    } catch (err) {
      setActionError(`Could not ${what}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  async function setActive(model: CuratedModel) {
    await runAction(`set ${model.modelId} as the active model`, () =>
      modelSetActive({ connectionId: model.connectionId, modelId: model.modelId }),
    );
  }

  async function disable(model: CuratedModel) {
    await runAction(`disable ${model.modelId}`, () =>
      modelDisable({ connectionId: model.connectionId, modelId: model.modelId }),
    );
  }

  async function toggleFavorite(model: CuratedModel) {
    await runAction(`update the favorite for ${model.modelId}`, () =>
      modelToggleFavorite({ connectionId: model.connectionId, modelId: model.modelId }),
    );
  }

  async function runPull() {
    const modelId = pullModelId.trim();
    if (!modelId) return;
    setPullState('pulling');
    setPullMessage('');
    try {
      await ollamaPull(modelId);
      setPullState('done');
      setPullMessage(`${modelId} pulled · now available`);
      load();
    } catch (err) {
      setPullState('error');
      setPullMessage(err instanceof Error ? err.message : String(err));
    }
  }

  const filtered = result.models.filter((m) => {
    const term = search.trim().toLowerCase();
    if (!term) return true;
    return `${m.modelId} ${m.providerKind}`.toLowerCase().includes(term);
  });

  return (
    <div className="settings" data-testid="models-screen">
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 8 }}>
        <label className="search" style={{ flex: 1 }}>
          <input
            type="search"
            data-testid="models-search"
            aria-label="Search models"
            placeholder="Search models…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </label>
      </div>

      {result.models.length > 0 && (
        <>
          {result.activeUnavailable && (
            <div className="grp" data-testid="model-active-unavailable" style={{ borderColor: 'var(--warning)', padding: '12px 16px', display: 'flex', alignItems: 'center', gap: 12 }}>
              <span className="status-dot amber" aria-hidden="true" />
              <div style={{ flex: 1 }}>
                <strong style={{ fontSize: 'var(--fs-small)' }}>
                  Your active model (<span className="mono">{result.staleActiveModelId}</span>) is no longer available
                </strong>
                <div className="muted tiny">Its connection or model was disabled. Pick a new active model below.</div>
              </div>
            </div>
          )}
          {!result.hasActive && !result.activeUnavailable && (
            <div className="grp" data-testid="models-no-active-banner" style={{ borderColor: 'var(--warning)', padding: '12px 16px', display: 'flex', alignItems: 'center', gap: 12 }}>
              <span className="status-dot amber" aria-hidden="true" />
              <div style={{ flex: 1 }}>
                <strong style={{ fontSize: 'var(--fs-small)' }}>No active model selected</strong>
                <div className="muted tiny">Pick one below — redrafter can&apos;t refine without an active model.</div>
              </div>
            </div>
          )}

          <section className="sec">
            <h2 className="sec__title">Enabled models</h2>
            <p className="muted tiny" style={{ margin: '0 0 8px' }}>
              Curated from your{' '}
              <button
                type="button"
                data-testid="models-connections-link"
                onClick={() => onNavigateToConnections?.()}
                style={{ color: 'var(--primary)', background: 'none', border: 'none', padding: 0, font: 'inherit', cursor: 'pointer' }}
              >
                connections
              </button>
              . Set one as the global <strong>active</strong> model; ⭐ favorites float to the top of the menu-bar tray.
            </p>
            {actionError && (
              <p
                className="tiny"
                role="alert"
                data-testid="models-action-error"
                style={{ margin: '0 0 8px', color: 'var(--danger, #d0433b)' }}
              >
                {actionError}
              </p>
            )}
            <div className="grp" data-testid="models-table">
              <table className="mtable" role="radiogroup" aria-label="Active model" data-testid="active-model">
                <thead>
                  <tr>
                    <th style={{ width: 44 }}>Active</th>
                    <th>Model</th>
                    <th>Provider</th>
                    <th style={{ width: 110 }}>State</th>
                    <th style={{ width: 44 }}>★</th>
                    <th style={{ width: 40 }}>
                      <span className="vh">Disable</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((model) => (
                    <tr key={`${model.connectionId}:${model.modelId}`}>
                      <td>
                        <button
                          className="radio-mark"
                          role="radio"
                          aria-checked={model.active}
                          aria-label={`Set ${model.modelId} active`}
                          data-testid={`model-active-radio-${model.modelId}`}
                          onClick={() => setActive(model)}
                        />
                      </td>
                      <td className="mono">{model.modelId}</td>
                      <td>{model.providerKind}</td>
                      <td>
                        {/* An explicit word, not just the radio dot: which row
                            is active has to be readable at a glance. */}
                        {model.active && (
                          <span className="chip ok" data-testid={`model-active-chip-${model.modelId}`}>
                            active
                          </span>
                        )}
                        <span className="run-state">{model.providerKind === 'ollama' ? 'local' : 'cloud'}</span>
                      </td>
                      <td>
                        <button
                          className="star-btn"
                          data-testid={`model-favorite-${model.modelId}`}
                          aria-pressed={model.favorite}
                          aria-label={`Favorite ${model.modelId}`}
                          onClick={() => toggleFavorite(model)}
                          style={model.favorite ? { color: 'var(--warning)' } : undefined}
                        >
                          {model.favorite ? '★' : '☆'}
                        </button>
                      </td>
                      <td>
                        <button
                          className="btn btn--ghost btn--sm"
                          data-testid={`model-disable-${model.modelId}`}
                          aria-label={`Disable ${model.modelId}`}
                          onClick={() => disable(model)}
                        >
                          ✕
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </>
      )}

      {result.models.length === 0 && (
        <section className="sec">
          <div className="grp">
            <div className="empty" data-testid="models-empty">
              <p style={{ fontSize: 16, color: 'var(--text)', margin: '0 0 6px' }}>No models enabled</p>
              <p className="muted" style={{ margin: '0 0 16px' }}>
                Connect a provider and enable models to get started.
              </p>
              <button
                type="button"
                className="btn btn--primary"
                data-testid="models-connections-link"
                onClick={() => onNavigateToConnections?.()}
              >
                Add one in Connections
              </button>
            </div>
          </div>
        </section>
      )}

      <section className="sec">
        <h2 className="sec__title">Get more Ollama models</h2>
        <p className="muted tiny" style={{ margin: '0 0 8px' }}>
          Pull a model to run locally.
        </p>
        <div className="grp" style={{ padding: 14 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <input
              className="input mono"
              data-testid="ollama-pull-field"
              placeholder="llama3.2  ·  mistral  ·  phi4"
              aria-label="Ollama model to pull"
              style={{ flex: 1, minWidth: 180 }}
              value={pullModelId}
              onChange={(e) => setPullModelId(e.target.value)}
            />
            <button className="btn btn--primary btn--sm" data-testid="ollama-pull-start" onClick={runPull} disabled={pullState === 'pulling'}>
              Pull
            </button>
          </div>
          <div style={{ marginTop: 12 }}>
            {pullState === 'idle' && (
              <div className="muted tiny" data-testid="ollama-pull-idle">
                No download in progress.
              </div>
            )}
            {pullState === 'pulling' && (
              <div data-testid="ollama-pull-progress">
                <span className="mono">pulling {pullModelId}…</span>
              </div>
            )}
            {pullState === 'done' && (
              <div className="chip ok" data-testid="ollama-pull-done">
                <span className="status-dot green" aria-hidden="true" /> {pullMessage}
              </div>
            )}
            {pullState === 'error' && (
              <div className="chip err" data-testid="ollama-pull-error">
                <span className="status-dot red" aria-hidden="true" /> pull failed · {pullMessage}
              </div>
            )}
          </div>
        </div>
      </section>
    </div>
  );
}
