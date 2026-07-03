'use client';

// Behavior settings screen (wireframes/behavior.html, S5: "Default refine
// direction"). Phase A (A8) built ONLY the default-direction control: the
// editable baseline instruction applied to every refine that has no inline
// `/rd` command override. It's a plain settings value
// (`refine.default_direction`), read via settings_get on mount and
// persisted via settings_set on edit — the same read/write contract the
// backend prompt_builder falls back to (see
// app/src-tauri/src/prompt_builder.rs's DEFAULT_DIRECTION).
//
// B6b extended the screen with inject-mode, quote-behavior, and the
// on-failure fallback chain — the settings B5's orchestrator
// (`InjectMode`/`FallbackTarget` in app/src-tauri/src/orchestrator.rs) and
// B23 (which wires settings into the orchestrator call) will read.
//
// This task (C5) adds the history-retention controls and the
// progress-feedback toggles. The feedback toggles persist to the exact
// settings keys (`feedback.spinner`/`feedback.hud`/`feedback.sound`) that
// `app/src-tauri/src/feedback.rs`'s `on_refine_start`/`on_refine_done` (C1)
// read via `SettingsStore`'s typed `feedback_*_enabled` accessors to decide
// which in-flight/completion cues fire on the next refine — wiring those
// hooks into the real refine call is C17's job. The retention controls
// (`history.retention_count`/`history.retention_days`) are plain
// settings values with no backend reader yet (a later task's job), same as
// B6b's fallback chain/retry count above.
import { useEffect, useState } from 'react';
import { settingsGet, settingsSet } from '@/lib/ipc';

const DEFAULT_DIRECTION_SETTINGS_KEY = 'refine.default_direction';
const INJECT_MODE_SETTINGS_KEY = 'behavior.inject_mode';
const QUOTE_MODE_SETTINGS_KEY = 'behavior.quote_mode';
const ON_FAILURE_SETTINGS_KEY = 'behavior.on_failure';
const FALLBACK_CHAIN_SETTINGS_KEY = 'behavior.fallback_chain';
const RETRY_COUNT_SETTINGS_KEY = 'behavior.retry_count';
const RETENTION_COUNT_SETTINGS_KEY = 'history.retention_count';
const RETENTION_DAYS_SETTINGS_KEY = 'history.retention_days';
const FEEDBACK_SPINNER_SETTINGS_KEY = 'feedback.spinner';
const FEEDBACK_HUD_SETTINGS_KEY = 'feedback.hud';
const FEEDBACK_SOUND_SETTINGS_KEY = 'feedback.sound';

type InjectMode = 'blind' | 'review';
type QuoteMode = 'answer' | 'answer_quote' | 'rd';
type OnFailureMode = 'notify' | 'fallback';

const DEFAULT_INJECT_MODE: InjectMode = 'blind';
const DEFAULT_QUOTE_MODE: QuoteMode = 'answer_quote';
const DEFAULT_ON_FAILURE: OnFailureMode = 'notify';
const DEFAULT_FALLBACK_CHAIN = ['gpt-5.1', 'qwen3:8b'];
const DEFAULT_RETRY_COUNT = '2';
const DEFAULT_RETENTION_COUNT = '50';
const DEFAULT_RETENTION_DAYS = '7';
/** Matches the wireframe's default (checked) selection for each cue. */
const DEFAULT_FEEDBACK_SPINNER = true;
const DEFAULT_FEEDBACK_HUD = false;
const DEFAULT_FEEDBACK_SOUND = true;

/** How many history entries to keep, matching the wireframe's dropdown. */
const RETENTION_COUNT_OPTIONS = ['25', '50', '200', 'Unlimited'];
/** Days before auto-purge; 0 means session-only (cleared on quit). */
const RETENTION_DAYS_OPTIONS = ['0', '7', '30', '90'];

/** Parses a `settings_get` boolean result (`"true"`/`"false"`), tolerating
 * missing/malformed values by falling back to `fallback` — mirrors
 * `SettingsStore::get_bool`'s (settings.rs) tolerant read. */
function parseBool(value: string | null, fallback: boolean): boolean {
  if (value === 'true') return true;
  if (value === 'false') return false;
  return fallback;
}

/** Grouped by provider, matching the wireframe's fallback-model dropdown. */
const FALLBACK_MODEL_GROUPS: Array<{ label: string; models: string[] }> = [
  { label: 'Anthropic', models: ['claude-opus-4-6', 'claude-sonnet-4-6'] },
  { label: 'OpenAI', models: ['gpt-5.1'] },
  { label: 'Google', models: ['gemini-1.5-flash'] },
  { label: 'Ollama', models: ['qwen3:8b', 'llama3.1:8b'] },
];

/** The model a newly-added fallback row starts with. */
const NEW_FALLBACK_MODEL = 'claude-opus-4-6';

/** Parses a settings_get result for the fallback chain, tolerating missing/malformed JSON. */
function parseFallbackChain(value: string | null): string[] | null {
  if (!value) return null;
  try {
    const parsed = JSON.parse(value);
    if (Array.isArray(parsed) && parsed.every((item) => typeof item === 'string')) {
      return parsed;
    }
  } catch {
    // fall through to null below
  }
  return null;
}

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
  const [injectMode, setInjectMode] = useState<InjectMode>(DEFAULT_INJECT_MODE);
  const [quoteMode, setQuoteMode] = useState<QuoteMode>(DEFAULT_QUOTE_MODE);
  const [onFailure, setOnFailure] = useState<OnFailureMode>(DEFAULT_ON_FAILURE);
  const [fallbackChain, setFallbackChain] = useState<string[]>(DEFAULT_FALLBACK_CHAIN);
  const [retryCount, setRetryCount] = useState<string>(DEFAULT_RETRY_COUNT);
  const [retentionCount, setRetentionCount] = useState<string>(DEFAULT_RETENTION_COUNT);
  const [retentionDays, setRetentionDays] = useState<string>(DEFAULT_RETENTION_DAYS);
  const [feedbackSpinner, setFeedbackSpinner] = useState<boolean>(DEFAULT_FEEDBACK_SPINNER);
  const [feedbackHud, setFeedbackHud] = useState<boolean>(DEFAULT_FEEDBACK_HUD);
  const [feedbackSound, setFeedbackSound] = useState<boolean>(DEFAULT_FEEDBACK_SOUND);

  useEffect(() => {
    settingsGet(DEFAULT_DIRECTION_SETTINGS_KEY)
      .then((value) => setDirection(value ?? DEFAULT_DIRECTION))
      .catch(() => setDirection(DEFAULT_DIRECTION));

    settingsGet(INJECT_MODE_SETTINGS_KEY)
      .then((value) => setInjectMode(value === 'review' ? 'review' : DEFAULT_INJECT_MODE))
      .catch(() => setInjectMode(DEFAULT_INJECT_MODE));

    settingsGet(QUOTE_MODE_SETTINGS_KEY)
      .then((value) =>
        setQuoteMode(value === 'answer' || value === 'answer_quote' || value === 'rd' ? value : DEFAULT_QUOTE_MODE),
      )
      .catch(() => setQuoteMode(DEFAULT_QUOTE_MODE));

    settingsGet(ON_FAILURE_SETTINGS_KEY)
      .then((value) => setOnFailure(value === 'fallback' ? 'fallback' : DEFAULT_ON_FAILURE))
      .catch(() => setOnFailure(DEFAULT_ON_FAILURE));

    settingsGet(FALLBACK_CHAIN_SETTINGS_KEY)
      .then((value) => setFallbackChain(parseFallbackChain(value) ?? DEFAULT_FALLBACK_CHAIN))
      .catch(() => setFallbackChain(DEFAULT_FALLBACK_CHAIN));

    settingsGet(RETRY_COUNT_SETTINGS_KEY)
      .then((value) => setRetryCount(value ?? DEFAULT_RETRY_COUNT))
      .catch(() => setRetryCount(DEFAULT_RETRY_COUNT));

    settingsGet(RETENTION_COUNT_SETTINGS_KEY)
      .then((value) => setRetentionCount(value ?? DEFAULT_RETENTION_COUNT))
      .catch(() => setRetentionCount(DEFAULT_RETENTION_COUNT));

    settingsGet(RETENTION_DAYS_SETTINGS_KEY)
      .then((value) => setRetentionDays(value ?? DEFAULT_RETENTION_DAYS))
      .catch(() => setRetentionDays(DEFAULT_RETENTION_DAYS));

    settingsGet(FEEDBACK_SPINNER_SETTINGS_KEY)
      .then((value) => setFeedbackSpinner(parseBool(value, DEFAULT_FEEDBACK_SPINNER)))
      .catch(() => setFeedbackSpinner(DEFAULT_FEEDBACK_SPINNER));

    settingsGet(FEEDBACK_HUD_SETTINGS_KEY)
      .then((value) => setFeedbackHud(parseBool(value, DEFAULT_FEEDBACK_HUD)))
      .catch(() => setFeedbackHud(DEFAULT_FEEDBACK_HUD));

    settingsGet(FEEDBACK_SOUND_SETTINGS_KEY)
      .then((value) => setFeedbackSound(parseBool(value, DEFAULT_FEEDBACK_SOUND)))
      .catch(() => setFeedbackSound(DEFAULT_FEEDBACK_SOUND));
  }, []);

  const saveDirection = () => {
    settingsSet(DEFAULT_DIRECTION_SETTINGS_KEY, direction).catch(() => {});
  };

  const chooseInjectMode = (mode: InjectMode) => {
    setInjectMode(mode);
    settingsSet(INJECT_MODE_SETTINGS_KEY, mode).catch(() => {});
  };

  const chooseQuoteMode = (mode: QuoteMode) => {
    setQuoteMode(mode);
    settingsSet(QUOTE_MODE_SETTINGS_KEY, mode).catch(() => {});
  };

  const chooseOnFailure = (mode: OnFailureMode) => {
    setOnFailure(mode);
    settingsSet(ON_FAILURE_SETTINGS_KEY, mode).catch(() => {});
  };

  const saveFallbackChain = (next: string[]) => {
    setFallbackChain(next);
    settingsSet(FALLBACK_CHAIN_SETTINGS_KEY, JSON.stringify(next)).catch(() => {});
  };

  const changeFallbackModel = (index: number, model: string) => {
    saveFallbackChain(fallbackChain.map((existing, i) => (i === index ? model : existing)));
  };

  const removeFallbackModel = (index: number) => {
    saveFallbackChain(fallbackChain.filter((_, i) => i !== index));
  };

  const addFallbackModel = () => {
    saveFallbackChain([...fallbackChain, NEW_FALLBACK_MODEL]);
  };

  const changeRetryCount = (value: string) => {
    setRetryCount(value);
    settingsSet(RETRY_COUNT_SETTINGS_KEY, value).catch(() => {});
  };

  const changeRetentionCount = (value: string) => {
    setRetentionCount(value);
    settingsSet(RETENTION_COUNT_SETTINGS_KEY, value).catch(() => {});
  };

  const changeRetentionDays = (value: string) => {
    setRetentionDays(value);
    settingsSet(RETENTION_DAYS_SETTINGS_KEY, value).catch(() => {});
  };

  const toggleFeedbackSpinner = () => {
    const next = !feedbackSpinner;
    setFeedbackSpinner(next);
    settingsSet(FEEDBACK_SPINNER_SETTINGS_KEY, next ? 'true' : 'false').catch(() => {});
  };

  const toggleFeedbackHud = () => {
    const next = !feedbackHud;
    setFeedbackHud(next);
    settingsSet(FEEDBACK_HUD_SETTINGS_KEY, next ? 'true' : 'false').catch(() => {});
  };

  const toggleFeedbackSound = () => {
    const next = !feedbackSound;
    setFeedbackSound(next);
    settingsSet(FEEDBACK_SOUND_SETTINGS_KEY, next ? 'true' : 'false').catch(() => {});
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

      {/* INJECT MODE */}
      <section className="sec" data-testid="behavior-inject-mode">
        <h2 className="sec__title">Inject mode</h2>
        <div className="grp">
          <div className="opt">
            <div className="opt__main">
              <div className="opt__name">How the refined text lands</div>
              <div className="opt__desc">
                <strong>Blind</strong> replaces instantly. <strong>Review &amp; confirm</strong> shows the diff
                first so you can Accept, Edit or Discard.
              </div>
            </div>
            <div className="opt__ctrl">
              <div className="segmented" role="radiogroup" aria-label="Inject mode" data-testid="inject-mode">
                <button
                  className={injectMode === 'blind' ? 'active' : ''}
                  role="radio"
                  aria-checked={injectMode === 'blind'}
                  data-testid="inject-mode-blind"
                  onClick={() => chooseInjectMode('blind')}
                >
                  Blind (instant)
                </button>
                <button
                  className={injectMode === 'review' ? 'active' : ''}
                  role="radio"
                  aria-checked={injectMode === 'review'}
                  data-testid="inject-mode-review"
                  onClick={() => chooseInjectMode('review')}
                >
                  Review &amp; confirm
                </button>
              </div>
            </div>
          </div>
        </div>
      </section>

      {/* QUOTE BEHAVIOR */}
      <section className="sec" data-testid="behavior-quote-mode">
        <h2 className="sec__title">When the selection has a quote</h2>
        <p className="muted tiny" style={{ margin: '0 0 8px' }}>
          Quoted context (lines starting <span className="mono">&gt;</span>, &ldquo;On &hellip; wrote:&rdquo;,
          signatures) is detected automatically or marked with <span className="tag tag--q">/q</span>.
        </p>
        <div className="grp" style={{ padding: 14 }}>
          <div className="seg" data-testid="quote-behavior">
            <input
              type="radio"
              name="quote"
              id="q-answer"
              data-testid="quote-answer"
              checked={quoteMode === 'answer'}
              onChange={() => chooseQuoteMode('answer')}
            />
            <label htmlFor="q-answer">Answer only</label>
            <input
              type="radio"
              name="quote"
              id="q-answerquote"
              data-testid="quote-answer-quote"
              checked={quoteMode === 'answer_quote'}
              onChange={() => chooseQuoteMode('answer_quote')}
            />
            <label htmlFor="q-answerquote">Answer + quote</label>
            <input
              type="radio"
              name="quote"
              id="q-rd"
              data-testid="quote-rd"
              checked={quoteMode === 'rd'}
              onChange={() => chooseQuoteMode('rd')}
            />
            <label htmlFor="q-rd">Let /rd decide</label>
          </div>
        </div>
      </section>

      {/* ON FAILURE */}
      <section className="sec" data-testid="behavior-on-failure">
        <h2 className="sec__title">On failure</h2>
        <p className="muted tiny" style={{ margin: '0 0 8px' }}>
          If the model is unreachable, times out, or returns empty.
        </p>
        <div className="grp" style={{ padding: 14 }} data-testid="failure-mode">
          <div className="opt-row">
            <input
              type="radio"
              name="fail"
              id="fail-notify"
              data-testid="failure-notify"
              checked={onFailure === 'notify'}
              onChange={() => chooseOnFailure('notify')}
            />
            <label htmlFor="fail-notify">
              <strong>Notify, keep text.</strong> Show the error and leave your selection untouched.
            </label>
          </div>
          <div className="opt-row">
            <input
              type="radio"
              name="fail"
              id="fail-fallback"
              data-testid="failure-fallback"
              checked={onFailure === 'fallback'}
              onChange={() => chooseOnFailure('fallback')}
            />
            <label htmlFor="fail-fallback">
              <strong>Fall back through a chain,</strong> then notify if all fail.
            </label>
          </div>

          {/* ordered fallback chain */}
          <div style={{ margin: '8px 0 0 26px', display: 'flex', flexDirection: 'column', gap: 8 }}>
            {fallbackChain.map((model, index) => (
              <div
                key={index}
                className="opt-row"
                style={{ padding: 0, alignItems: 'center', gap: 8 }}
              >
                <span className="mono tiny muted" style={{ width: 16 }}>
                  {index + 1}.
                </span>
                <select
                  className="input mono"
                  aria-label={`Fallback model ${index + 1}`}
                  data-testid={`failure-fallback-model-${index + 1}`}
                  style={{ minWidth: 160 }}
                  value={model}
                  onChange={(event) => changeFallbackModel(index, event.target.value)}
                >
                  {FALLBACK_MODEL_GROUPS.map((group) => (
                    <optgroup key={group.label} label={group.label}>
                      {group.models.map((option) => (
                        <option key={option} value={option}>
                          {option}
                        </option>
                      ))}
                    </optgroup>
                  ))}
                </select>
                <button
                  type="button"
                  className="btn btn--ghost btn--sm"
                  data-testid={`failure-fallback-remove-${index + 1}`}
                  aria-label={`Remove fallback ${index + 1}`}
                  onClick={() => removeFallbackModel(index)}
                >
                  <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                    <path d="M6 6l12 12M18 6L6 18" />
                  </svg>
                </button>
              </div>
            ))}
          </div>

          <div
            style={{
              margin: '8px 0 0 26px',
              display: 'flex',
              alignItems: 'center',
              gap: 14,
              flexWrap: 'wrap',
            }}
          >
            <button type="button" className="btn btn--ghost btn--sm" data-testid="failure-fallback-add" onClick={addFallbackModel}>
              <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4}>
                <path d="M12 5v14M5 12h14" />
              </svg>{' '}
              Add fallback
            </button>
            <label style={{ display: 'inline-flex', alignItems: 'center', gap: 6, fontSize: 'var(--fs-small)', flex: '0 0 auto' }}>
              <span style={{ whiteSpace: 'nowrap' }}>retries per model</span>
              <select
                className="input"
                aria-label="Retry count"
                data-testid="failure-retry-count"
                style={{ minWidth: 64 }}
                value={retryCount}
                onChange={(event) => changeRetryCount(event.target.value)}
              >
                <option value="1">1</option>
                <option value="2">2</option>
                <option value="3">3</option>
              </select>
            </label>
          </div>
        </div>
      </section>

      {/* HISTORY RETENTION */}
      <section className="sec" data-testid="behavior-history-retention">
        <h2 className="sec__title">History retention</h2>
        <div className="grp" style={{ padding: 14 }} data-testid="history-retention">
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap', fontSize: 'var(--fs-small)' }}>
            <span>Keep</span>
            <label className="vh" htmlFor="ret-count">
              Entries to keep
            </label>
            <select
              className="input"
              id="ret-count"
              data-testid="retention-count"
              style={{ minWidth: 90 }}
              value={retentionCount}
              onChange={(event) => changeRetentionCount(event.target.value)}
            >
              {RETENTION_COUNT_OPTIONS.map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </select>
            <span>entries · auto-purge after</span>
            <label className="vh" htmlFor="ret-days">
              Days before purge
            </label>
            <select
              className="input"
              id="ret-days"
              data-testid="retention-days"
              style={{ minWidth: 70 }}
              value={retentionDays}
              onChange={(event) => changeRetentionDays(event.target.value)}
            >
              {RETENTION_DAYS_OPTIONS.map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </select>
            <span>days</span>
          </div>
          <p className="muted tiny" style={{ margin: '10px 0 0' }}>
            <span className="mono">0</span> days = session only (history cleared on quit).
          </p>
        </div>
      </section>

      {/* PROGRESS FEEDBACK */}
      <section className="sec" data-testid="behavior-feedback">
        <h2 className="sec__title">Progress feedback</h2>
        <div className="grp" style={{ padding: 14 }} data-testid="feedback-opts">
          <div className="opt-row">
            <input
              type="checkbox"
              id="fb-spinner"
              data-testid="feedback-spinner"
              checked={feedbackSpinner}
              onChange={toggleFeedbackSpinner}
            />
            <label htmlFor="fb-spinner">
              <strong>Menu-bar spinner</strong> — baseline indicator while refining.
            </label>
          </div>
          <div className="opt-row">
            <input
              type="checkbox"
              id="fb-hud"
              data-testid="feedback-hud"
              checked={feedbackHud}
              onChange={toggleFeedbackHud}
            />
            <label htmlFor="fb-hud">
              <strong>Cursor HUD</strong> — floating pill near your cursor.
            </label>
          </div>
          <div className="opt-row">
            <input
              type="checkbox"
              id="fb-sound"
              data-testid="feedback-sound"
              checked={feedbackSound}
              onChange={toggleFeedbackSound}
            />
            <label htmlFor="fb-sound">
              <strong>Completion sound</strong> — chime when the redraft lands.
            </label>
          </div>
        </div>
      </section>
    </div>
  );
}
