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
// This task (B6b) extends the screen with inject-mode, quote-behavior, and
// the on-failure fallback chain — the settings B5's orchestrator
// (`InjectMode`/`FallbackTarget` in app/src-tauri/src/orchestrator.rs) and
// B23 (which wires settings into the orchestrator call) will read. History
// retention and progress-feedback controls are a later phase (C5) that
// further extends this file; do not add them here.
import { useEffect, useState } from 'react';
import { settingsGet, settingsSet } from '@/lib/ipc';

const DEFAULT_DIRECTION_SETTINGS_KEY = 'refine.default_direction';
const INJECT_MODE_SETTINGS_KEY = 'behavior.inject_mode';
const QUOTE_MODE_SETTINGS_KEY = 'behavior.quote_mode';
const ON_FAILURE_SETTINGS_KEY = 'behavior.on_failure';
const FALLBACK_CHAIN_SETTINGS_KEY = 'behavior.fallback_chain';
const RETRY_COUNT_SETTINGS_KEY = 'behavior.retry_count';

type InjectMode = 'blind' | 'review';
type QuoteMode = 'answer' | 'answer_quote' | 'rd';
type OnFailureMode = 'notify' | 'fallback';

const DEFAULT_INJECT_MODE: InjectMode = 'blind';
const DEFAULT_QUOTE_MODE: QuoteMode = 'answer_quote';
const DEFAULT_ON_FAILURE: OnFailureMode = 'notify';
const DEFAULT_FALLBACK_CHAIN = ['gpt-5.1', 'qwen3:8b'];
const DEFAULT_RETRY_COUNT = '2';

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
    </div>
  );
}
