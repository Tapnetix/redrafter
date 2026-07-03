'use client';

// Menu-bar tray dropdown (wireframes/tray.html, controls/tray.json). B9's
// scope is the active-model switcher: the "Active model" row is collapsed
// by default (only the current active model's id shows); expanding it
// reveals favorites (starred models) flat on top, then the full
// per-connection model list grouped by provider beneath — mirroring
// app.js's `renderTrayModels`. Picking any entry (favorite or full-list)
// calls `tray_set_active_model` and collapses the switcher back down.
//
// Also renders the always-present entries the manifest calls out for this
// task per the guidance: Refine selection, Settings…, History…, and Quit
// redrafter — plus "Manage models…" inside the expanded switcher region,
// navigating to the Models screen. Wired via ipc.ts: `modelsList` (B8)
// feeds the switcher, `trayRefine`/`trayQuit` (A9) back the primary action
// and quit, and `traySetActiveModel` (new, this task) applies a pick.
//
// Pause/resume + status, check-for-updates, and launch-at-login (also on
// `wireframes/tray.html`/`controls/tray.json`) are out of this task's
// scope — B17 ("Tray status and pause", which depends on this task) wires
// those affordances into this component. The native OS tray menu itself is
// B23/tray.rs (Rust); this is the frontend tray surface/preview the
// switcher logic lives in.

import { useEffect, useState } from 'react';
import {
  modelsList,
  trayQuit,
  trayRefine,
  traySetActiveModel,
  type CuratedModel,
  type ModelsListResult,
} from '@/lib/ipc';

const EMPTY_RESULT: ModelsListResult = {
  models: [],
  hasActive: false,
  activeUnavailable: false,
  staleActiveModelId: null,
};

export interface TrayProps {
  /** Called when the user follows "Manage models…" to the Models screen. */
  onNavigateToModels?: () => void;
  /** Called when the user follows "Settings…" to the General screen. */
  onNavigateToSettings?: () => void;
  /** Called when the user follows "History…" to the History screen. */
  onNavigateToHistory?: () => void;
}

export default function Tray({ onNavigateToModels, onNavigateToSettings, onNavigateToHistory }: TrayProps) {
  const [result, setResult] = useState<ModelsListResult>(EMPTY_RESULT);
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    modelsList()
      .then(setResult)
      .catch(() => setResult(EMPTY_RESULT));
  }, []);

  async function pick(model: CuratedModel) {
    const updated = await traySetActiveModel({ connectionId: model.connectionId, modelId: model.modelId });
    setResult(updated);
    // Mirrors app.js's pickTrayModel: collapse the switcher after a pick.
    setExpanded(false);
  }

  function handleRefine() {
    void trayRefine();
  }

  function handleQuit() {
    void trayQuit();
  }

  const active = result.models.find((m) => m.active);
  const activeLabel = active ? active.modelId : 'No model selected';
  const favorites = result.models.filter((m) => m.favorite);

  // Providers in first-seen order, so the grouped list below favorites is
  // stable rather than resorting on every models_list refresh.
  const providers: string[] = [];
  result.models.forEach((m) => {
    if (!providers.includes(m.providerKind)) providers.push(m.providerKind);
  });

  return (
    <div className="tray" role="menu" aria-label="redrafter menu" data-testid="tray">
      <div className="tray__head">
        <span className="cap__badge" style={{ width: 24, height: 24, fontSize: 13 }} aria-hidden="true">
          R
        </span>
        <strong style={{ fontSize: 14 }}>redrafter</strong>
      </div>

      <button className="menu__item" role="menuitem" data-testid="tray-refine" onClick={handleRefine}>
        Refine selection
      </button>

      <div className="menu__sep" />

      <button
        className="menu__item"
        role="menuitem"
        data-testid="tray-active-model"
        aria-expanded={expanded}
        aria-controls="tray-model-region"
        onClick={() => setExpanded((v) => !v)}
      >
        Active model
        <span
          className="mono tiny"
          data-testid="tray-active-model-label"
          style={{ marginLeft: 'auto', color: 'var(--primary)' }}
        >
          {activeLabel}
        </span>
        <span className="tray-caret" aria-hidden="true">
          {expanded ? '▾' : '▸'}
        </span>
      </button>

      {expanded && (
        <div id="tray-model-region" data-testid="tray-model-region">
          {favorites.length > 0 && (
            <div role="group" aria-label="Favorites" data-testid="tray-favorites">
              <div className="tray__sec">★ Favorites</div>
              {favorites.map((model) => (
                <button
                  key={`fav-${model.connectionId}:${model.modelId}`}
                  className="menu__item"
                  role="menuitemradio"
                  aria-checked={model.active}
                  data-testid={`tray-fav-${model.modelId}`}
                  onClick={() => pick(model)}
                >
                  <span className="radio-mark" aria-hidden="true" />
                  <span className="mono" style={{ flex: 1 }}>
                    {model.modelId}
                  </span>
                  <span style={{ color: 'var(--warning)' }}>★</span>
                </button>
              ))}
            </div>
          )}

          {providers.map((provider) => (
            <div key={provider} role="group" aria-label={provider} data-testid={`tray-provider-${provider}`}>
              <div className="tray__sec">{provider}</div>
              {result.models
                .filter((m) => m.providerKind === provider)
                .map((model) => (
                  <button
                    key={`model-${model.connectionId}:${model.modelId}`}
                    className="menu__item"
                    role="menuitemradio"
                    aria-checked={model.active}
                    data-testid={`tray-model-${model.modelId}`}
                    onClick={() => pick(model)}
                  >
                    <span className="radio-mark" aria-hidden="true" />
                    <span className="mono" style={{ flex: 1 }}>
                      {model.modelId}
                    </span>
                  </button>
                ))}
            </div>
          ))}

          <button
            className="menu__item"
            role="menuitem"
            data-testid="tray-manage-models"
            onClick={() => onNavigateToModels?.()}
          >
            Manage models…
          </button>
        </div>
      )}

      <div className="menu__sep" />

      <button
        className="menu__item"
        role="menuitem"
        data-testid="tray-settings"
        onClick={() => onNavigateToSettings?.()}
      >
        Settings…
      </button>
      <button
        className="menu__item"
        role="menuitem"
        data-testid="tray-history"
        onClick={() => onNavigateToHistory?.()}
      >
        History…
      </button>

      <div className="menu__sep" />

      <button className="menu__item" role="menuitem" data-testid="tray-quit" onClick={handleQuit}>
        Quit redrafter
      </button>
    </div>
  );
}
