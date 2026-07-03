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
// Pause/resume + status, check-for-updates, and launch-at-login
// (`wireframes/tray.html`/`controls/tray.json`) are this task's own scope
// (B17, "Tray status and pause"): a global pause toggle suspends the tray's
// Refine action (the real hotkey suspension is B23/native), the status line
// reflects idle/refining/error/paused, "Check for updates…" triggers the
// updater check, and "Launch at login" toggles autostart. The native OS
// tray menu itself is B23/tray.rs (Rust); this is the frontend tray
// surface/preview these controls live in.

import { useEffect, useState } from 'react';
import {
  checkUpdates,
  modelsList,
  setLaunchAtLogin,
  settingsGet,
  trayPause,
  trayQuit,
  trayRefine,
  trayResume,
  traySetActiveModel,
  type CuratedModel,
  type ModelsListResult,
} from '@/lib/ipc';

/** Settings keys `tray_pause`/`tray_resume`/`tray_set_launch_login` persist
 * on the backend (B17); read on mount so the tray reflects state that
 * outlives a single dropdown open (e.g. paused across a restart). */
const PAUSED_SETTINGS_KEY = 'paused';
const LAUNCH_LOGIN_SETTINGS_KEY = 'launch_at_login';

type TrayStatus = 'idle' | 'refining' | 'error' | 'paused';
type UpdatesStatus = 'idle' | 'checking' | 'uptodate' | 'available';

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
  const [paused, setPaused] = useState(false);
  const [refining, setRefining] = useState(false);
  const [refineError, setRefineError] = useState<string | null>(null);
  const [launchAtLogin, setLaunchAtLoginValue] = useState(true);
  const [updatesStatus, setUpdatesStatus] = useState<UpdatesStatus>('idle');
  const [updateVersion, setUpdateVersion] = useState<string | null>(null);

  useEffect(() => {
    modelsList()
      .then(setResult)
      .catch(() => setResult(EMPTY_RESULT));
    settingsGet(PAUSED_SETTINGS_KEY)
      .then((value) => setPaused(value === 'true'))
      .catch(() => setPaused(false));
    settingsGet(LAUNCH_LOGIN_SETTINGS_KEY)
      .then((value) => setLaunchAtLoginValue(value !== 'false'))
      .catch(() => setLaunchAtLoginValue(true));
  }, []);

  async function pick(model: CuratedModel) {
    const updated = await traySetActiveModel({ connectionId: model.connectionId, modelId: model.modelId });
    setResult(updated);
    // Mirrors app.js's pickTrayModel: collapse the switcher after a pick.
    setExpanded(false);
  }

  async function handleRefine() {
    // While paused, the tray's own Refine entry (like the global hotkey,
    // suspended natively by B23) is a no-op.
    if (paused) return;
    setRefining(true);
    setRefineError(null);
    try {
      await trayRefine();
    } catch (err) {
      setRefineError(err instanceof Error ? err.message : String(err));
    } finally {
      setRefining(false);
    }
  }

  function handleQuit() {
    void trayQuit();
  }

  async function handlePause() {
    await trayPause();
    setPaused(true);
  }

  async function handleResume() {
    await trayResume();
    setPaused(false);
  }

  async function handleCheckUpdates() {
    setUpdatesStatus('checking');
    try {
      const res = await checkUpdates();
      if (res.updateAvailable) {
        setUpdateVersion(res.version ?? null);
        setUpdatesStatus('available');
      } else {
        setUpdatesStatus('uptodate');
      }
    } catch {
      setUpdatesStatus('idle');
    }
  }

  async function handleToggleLaunchLogin() {
    const next = !launchAtLogin;
    setLaunchAtLoginValue(next);
    try {
      await setLaunchAtLogin(next);
    } catch {
      // Revert the optimistic update if the backend rejected the change.
      setLaunchAtLoginValue(!next);
    }
  }

  const status: TrayStatus = paused ? 'paused' : refining ? 'refining' : refineError ? 'error' : 'idle';

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
        <div style={{ flex: 1, minWidth: 0 }}>
          <strong style={{ fontSize: 14, display: 'block' }}>redrafter</strong>
          <span className="tiny muted" aria-live="polite">
            {status === 'idle' && <span data-testid="tray-state-idle">Ready</span>}
            {status === 'refining' && (
              <span data-testid="tray-state-refining">
                Refining… <span className="mono">{activeLabel}</span>
              </span>
            )}
            {status === 'error' && <span data-testid="tray-state-error">Last refine failed — {refineError}</span>}
            {status === 'paused' && <span data-testid="tray-state-paused">Paused — capturing off</span>}
          </span>
        </div>
      </div>

      <button
        className="menu__item"
        role="menuitem"
        data-testid="tray-refine"
        onClick={handleRefine}
        disabled={paused}
      >
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

      {!paused && (
        <button className="menu__item" role="menuitem" data-testid="tray-pause" onClick={handlePause}>
          Pause capturing
        </button>
      )}
      {paused && (
        <button className="menu__item" role="menuitem" data-testid="tray-resume" onClick={handleResume}>
          Resume capturing
        </button>
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
      <button className="menu__item" role="menuitem" data-testid="tray-updates" onClick={handleCheckUpdates}>
        {updatesStatus === 'idle' && 'Check for updates…'}
        {updatesStatus === 'checking' && (
          <span data-testid="tray-updates-checking" style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
            <span className="spinner" style={{ width: 12, height: 12 }} aria-hidden="true" /> Checking…
          </span>
        )}
        {updatesStatus === 'uptodate' && <span data-testid="tray-updates-uptodate">You&apos;re up to date</span>}
        {updatesStatus === 'available' && (
          <span data-testid="tray-updates-available" style={{ color: 'var(--primary)' }}>
            Update to {updateVersion ?? 'latest'} →
          </span>
        )}
      </button>
      <button
        className="menu__item"
        role="menuitemcheckbox"
        aria-checked={launchAtLogin}
        data-testid="tray-launch-login"
        onClick={handleToggleLaunchLogin}
      >
        Launch at login
        <span className="check" aria-hidden="true">
          {launchAtLogin ? '✓' : ''}
        </span>
      </button>

      <div className="menu__sep" />

      <button className="menu__item" role="menuitem" data-testid="tray-quit" onClick={handleQuit}>
        Quit redrafter
      </button>
    </div>
  );
}
