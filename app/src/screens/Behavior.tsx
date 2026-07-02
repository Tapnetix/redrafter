'use client';

// Behavior settings screen (wireframes/behavior.html, S5: "Default refine
// direction"). Phase A (this task, A8) builds ONLY the default-direction
// control: the editable baseline instruction applied to every refine that
// has no inline `/rd` command override. It's a plain settings value
// (`refine.default_direction`), read via settings_get on mount and
// persisted via settings_set on edit — the same read/write contract the
// backend prompt_builder falls back to (see
// app/src-tauri/src/prompt_builder.rs's DEFAULT_DIRECTION).
//
// The rest of this screen — inject-mode, quote-behavior, on-failure
// fallback chain, history retention, and progress-feedback controls — are
// later phases (B6b, C5) that EXTEND this file; do not add them here.
import { useEffect, useState } from 'react';
import { settingsGet, settingsSet } from '@/lib/ipc';

const DEFAULT_DIRECTION_SETTINGS_KEY = 'refine.default_direction';

/**
 * The default direction shown/edited on the Behavior screen — the wireframe's
 * short, user-facing baseline instruction. Distinct from the backend's
 * internal `prompt_builder::DEFAULT_DIRECTION` (the full LLM system prompt);
 * this is what the settings UI prefills and lets the user edit.
 */
export const DEFAULT_DIRECTION =
  'Fix grammar, spelling and clarity — keep my voice, tone and length.';

export default function Behavior() {
  const [direction, setDirection] = useState<string>(DEFAULT_DIRECTION);

  useEffect(() => {
    settingsGet(DEFAULT_DIRECTION_SETTINGS_KEY)
      .then((value) => setDirection(value ?? DEFAULT_DIRECTION))
      .catch(() => setDirection(DEFAULT_DIRECTION));
  }, []);

  const saveDirection = () => {
    settingsSet(DEFAULT_DIRECTION_SETTINGS_KEY, direction).catch(() => {});
  };

  return (
    <div className="settings">
      {/* DEFAULT DIRECTION */}
      <section className="sec" data-testid="behavior-default-direction">
        <h2 className="sec__title">Default direction</h2>
        <p className="muted tiny" style={{ margin: '0 0 8px' }}>
          Applied to every selection with no inline <span className="tag tag--rd">/rd</span> command.
          Your baseline voice.
        </p>
        <div className="grp" style={{ padding: 14 }}>
          <label className="vh" htmlFor="default-direction">
            Default direction
          </label>
          <textarea
            className="textarea"
            id="default-direction"
            data-testid="default-direction"
            rows={3}
            value={direction}
            onChange={(event) => setDirection(event.target.value)}
            onBlur={saveDirection}
          />
          <p className="muted tiny" data-testid="default-language-note" style={{ margin: '8px 0 0' }}>
            Output language: same as your text — use <span className="tag tag--lang">/lang</span> or a
            preset to translate.
          </p>
        </div>
      </section>
    </div>
  );
}
