'use client';

import { useState } from 'react';
import { connectionAdd, openExternal } from '@/lib/ipc';

type ProviderType = 'cloud' | 'local';
type CloudProviderId = 'anthropic' | 'openai' | 'gemini' | 'openai-compat';

interface CloudProviderInfo {
  id: CloudProviderId;
  label: string;
  baseUrl: string;
  keyUrl: string;
}

// Real per-vendor base URLs for the three named APIs; the OpenAI-compatible
// option targets a generic local/self-hosted endpoint. A dedicated
// custom-base-URL field for that option is Phase B's Connections screen
// (B7) — this first-run chooser only has the single API-key field the
// wireframe/controls manifest defines.
const CLOUD_PROVIDERS: CloudProviderInfo[] = [
  {
    id: 'anthropic',
    label: 'Anthropic',
    baseUrl: 'https://api.anthropic.com',
    keyUrl: 'https://platform.claude.com/settings/keys',
  },
  {
    id: 'openai',
    label: 'OpenAI',
    baseUrl: 'https://api.openai.com',
    keyUrl: 'https://platform.openai.com/api-keys',
  },
  {
    id: 'gemini',
    label: 'Google Gemini',
    baseUrl: 'https://generativelanguage.googleapis.com',
    keyUrl: 'https://aistudio.google.com/apikey',
  },
  {
    id: 'openai-compat',
    label: 'OpenAI-compatible',
    baseUrl: 'http://localhost:8080',
    keyUrl: 'https://platform.openai.com/api-keys',
  },
];

const OLLAMA_BASE_URL = 'http://localhost:11434';

type ConnectStatus = 'idle' | 'connecting' | 'connected' | 'error';

export interface FirstRunProps {
  /** Called when the user clicks Continue, finishing first-run setup. */
  onContinue?: () => void;
}

export default function FirstRun({ onContinue }: FirstRunProps) {
  const [providerType, setProviderType] = useState<ProviderType>('cloud');
  const [cloudProviderId, setCloudProviderId] = useState<CloudProviderId>('anthropic');
  const [apiKey, setApiKey] = useState('');
  const [cloudStatus, setCloudStatus] = useState<ConnectStatus>('idle');
  const [cloudError, setCloudError] = useState<string | null>(null);
  const [ollamaStatus, setOllamaStatus] = useState<ConnectStatus>('idle');
  const [ollamaError, setOllamaError] = useState<string | null>(null);
  // Reported separately from `cloudError` so a failure to hand the key URL to
  // the OS browser doesn't masquerade as a failed provider connection.
  const [linkError, setLinkError] = useState<string | null>(null);

  const cloudProvider = CLOUD_PROVIDERS.find((p) => p.id === cloudProviderId)!;

  function selectCloudProvider(id: CloudProviderId) {
    setCloudProviderId(id);
    setCloudStatus('idle');
    setCloudError(null);
  }

  async function connectCloud() {
    setCloudStatus('connecting');
    setCloudError(null);
    try {
      await connectionAdd({
        providerKind: cloudProvider.id,
        baseUrl: cloudProvider.baseUrl,
        apiKey,
      });
      setCloudStatus('connected');
    } catch (err) {
      setCloudStatus('error');
      setCloudError(err instanceof Error ? err.message : String(err));
    }
  }

  async function connectOllama() {
    setOllamaStatus('connecting');
    setOllamaError(null);
    try {
      await connectionAdd({
        providerKind: 'ollama',
        baseUrl: OLLAMA_BASE_URL,
        apiKey: '',
      });
      setOllamaStatus('connected');
    } catch (err) {
      setOllamaStatus('error');
      setOllamaError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <div className="min-h-screen grid place-items-center p-6 bg-[var(--bg)] text-[var(--text)]">
      <div className="w-[600px] max-w-[94vw]">
        <img
          src="/logo.png"
          alt=""
          width={46}
          height={46}
          draggable={false}
          className="w-[46px] h-[46px] mb-4"
        />
        <p className="font-mono text-xs tracking-widest uppercase text-[var(--primary)] mb-2">First run</p>
        <h1 className="text-[30px] font-bold tracking-tight mb-2.5">Connect your first AI provider</h1>
        <p className="text-[var(--text-secondary)] text-base max-w-[56ch] mb-2">
          redrafter needs one model to refine your text. Use a cloud provider with an API key, or run models
          locally with Ollama — your text never leaves your machine.
        </p>

        <h2 className="sr-only">Choose provider type</h2>
        <div
          role="tablist"
          aria-label="Provider type"
          data-testid="firstrun-provider-type-tabs"
          className="grid grid-cols-2 gap-3 my-3.5"
        >
          <button
            type="button"
            role="tab"
            id="firstrun-cloud-tab"
            aria-selected={providerType === 'cloud'}
            aria-controls="firstrun-cloud-panel"
            data-testid="firstrun-cloud"
            onClick={() => setProviderType('cloud')}
            className={`text-left p-4 rounded-[var(--radius)] border bg-[var(--surface)] flex flex-col gap-1.5 ${
              providerType === 'cloud'
                ? 'border-[var(--primary)] shadow-[0_0_0_1px_var(--primary)]'
                : 'border-[var(--border)]'
            }`}
          >
            <strong className="text-[var(--fs-body)]">☁ Cloud provider</strong>
            <span className="text-[var(--text-secondary)] text-xs">
              Anthropic, OpenAI, Gemini, or any OpenAI-compatible endpoint. Paste an API key.
            </span>
          </button>
          <button
            type="button"
            role="tab"
            id="firstrun-local-tab"
            aria-selected={providerType === 'local'}
            aria-controls="firstrun-local-panel"
            data-testid="firstrun-local"
            onClick={() => setProviderType('local')}
            className={`text-left p-4 rounded-[var(--radius)] border bg-[var(--surface)] flex flex-col gap-1.5 ${
              providerType === 'local'
                ? 'border-[var(--primary)] shadow-[0_0_0_1px_var(--primary)]'
                : 'border-[var(--border)]'
            }`}
          >
            <strong className="text-[var(--fs-body)]">🖥 Local (Ollama)</strong>
            <span className="text-[var(--text-secondary)] text-xs">
              Run open models on your own machine. Private, offline, no key.
            </span>
          </button>
        </div>

        <section
          id="firstrun-cloud-panel"
          role="tabpanel"
          aria-labelledby="firstrun-cloud-tab"
          data-testid="firstrun-cloud-panel"
          hidden={providerType !== 'cloud'}
        >
          <h2 className="text-[var(--fs-h2)] font-semibold mb-2">Pick a cloud provider</h2>
          <div
            role="radiogroup"
            aria-label="Cloud provider"
            data-testid="firstrun-cloud-providers"
            className="grid grid-cols-2 gap-2.5"
          >
            {CLOUD_PROVIDERS.map((p) => (
              <button
                key={p.id}
                type="button"
                role="radio"
                aria-checked={cloudProviderId === p.id}
                data-testid={`firstrun-provider-${p.id}`}
                onClick={() => selectCloudProvider(p.id)}
                className={`flex items-center gap-2.5 p-3 rounded-[var(--radius)] border bg-[var(--surface)] ${
                  cloudProviderId === p.id
                    ? 'border-[var(--primary)] shadow-[0_0_0_1px_var(--primary)]'
                    : 'border-[var(--border)]'
                }`}
              >
                {p.label}
              </button>
            ))}
          </div>

          <div className="border border-[var(--border)] rounded-[var(--radius-md)] bg-[var(--surface)] p-3.5 mt-3">
            <div className="flex flex-col gap-1.5">
              <label htmlFor="firstrun-key" className="text-[var(--fs-small)] font-medium text-[var(--text-secondary)]">
                API key for <span className="font-mono">{cloudProvider.label}</span>
              </label>
              <input
                id="firstrun-key"
                data-testid="firstrun-key"
                type="password"
                placeholder="sk-…"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                className="font-mono bg-[var(--surface)] border border-[var(--border)] rounded-[var(--radius)] text-[var(--text)] text-[var(--fs-small)] px-2.5 py-2 w-full"
              />
              <a
                href={cloudProvider.keyUrl}
                target="_blank"
                rel="noopener noreferrer"
                data-testid="firstrun-get-key"
                // A `target="_blank"` href is inert inside a Tauri webview, so
                // hand the URL to the OS through `open_external` instead.
                onClick={(e) => {
                  e.preventDefault();
                  setLinkError(null);
                  void openExternal(cloudProvider.keyUrl).catch((err) =>
                    setLinkError(err instanceof Error ? err.message : String(err)),
                  );
                }}
                className="text-[var(--primary)] text-xs"
              >
                Get a key ↗
              </a>
              {linkError && (
                <span role="alert" data-testid="firstrun-get-key-error" className="text-[var(--error)] text-xs">
                  Could not open the browser: {linkError}
                </span>
              )}
            </div>
            <div className="flex items-center gap-2.5 mt-2.5">
              <button
                type="button"
                data-testid="firstrun-connect"
                onClick={connectCloud}
                disabled={cloudStatus === 'connecting' || cloudStatus === 'connected' || apiKey === ''}
                className="inline-flex items-center gap-1.5 px-3.5 py-1.5 text-sm font-semibold rounded-[var(--radius)] bg-[var(--primary)] text-white disabled:opacity-45"
              >
                {cloudStatus === 'connecting' ? 'Connecting…' : cloudStatus === 'connected' ? 'Connected' : 'Connect'}
              </button>
              {cloudStatus === 'error' && cloudError && (
                <span role="alert" className="text-[var(--error)] text-xs">
                  {cloudError}
                </span>
              )}
            </div>
          </div>
        </section>

        <section
          id="firstrun-local-panel"
          role="tabpanel"
          aria-labelledby="firstrun-local-tab"
          data-testid="firstrun-local-panel"
          hidden={providerType !== 'local'}
        >
          <div
            id="firstrun-ollama-detected"
            data-testid="firstrun-ollama-detected"
            className="border border-[var(--border)] rounded-[var(--radius-md)] bg-[var(--surface)] p-3.5"
          >
            <div className="flex items-center gap-3">
              <span className="w-2.5 h-2.5 rounded-full bg-[var(--refined)] flex-none" />
              <div className="flex-1">
                <strong className="text-[var(--fs-small)]">
                  Ollama detected at <span className="font-mono">localhost:11434</span>
                </strong>
                <div className="text-[var(--text-secondary)] text-xs">3 models available locally.</div>
              </div>
              <button
                type="button"
                data-testid="firstrun-ollama-connect"
                onClick={connectOllama}
                disabled={ollamaStatus === 'connecting' || ollamaStatus === 'connected'}
                className="inline-flex items-center gap-1.5 px-3.5 py-1.5 text-sm font-semibold rounded-[var(--radius)] bg-[var(--primary)] text-white disabled:opacity-45"
              >
                {ollamaStatus === 'connecting'
                  ? 'Connecting…'
                  : ollamaStatus === 'connected'
                    ? 'Connected'
                    : 'Connect'}
              </button>
            </div>
            {ollamaStatus === 'error' && ollamaError && (
              <p role="alert" className="text-[var(--error)] text-xs mt-2">
                {ollamaError}
              </p>
            )}
          </div>
        </section>

        <div className="flex items-center gap-2.5 mt-6">
          <span className="text-[var(--text-secondary)] text-xs">
            You can add more providers later in Connections.
          </span>
          <div className="flex-1" />
          <button
            type="button"
            data-testid="firstrun-continue"
            onClick={() => onContinue?.()}
            className="inline-flex items-center gap-1.5 px-3.5 py-1.5 text-sm font-semibold rounded-[var(--radius)] bg-[var(--primary)] text-white"
          >
            Continue →
          </button>
        </div>
      </div>
    </div>
  );
}
