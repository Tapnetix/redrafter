# redrafter

A cross-platform (macOS-first) menu-bar app that refines the text you're writing.

Select text in any app, press a global hotkey, and redrafter refines it via a
configured AI model — local (Qwen via Ollama) or cloud (Anthropic Claude, Google
Gemini, or any OpenAI-compatible endpoint) — and pastes the polished result back
in place.

Inline commands inside the selection let you direct the model:

- `/rd <direction>` — an instruction to the model (e.g. "make it formal and concise")
- `/m <message>` — marks the start of your own message
- `/q <quote>` — quoted context (heuristically detected for replies; this is a manual override)
- `/lang <code>` — output in a target language (e.g. English notes → a polished German reply)

With no commands, redrafter applies a configurable light grammar/clarity polish
that preserves your voice.

## Status

Early development. Built with Tauri 2 (Rust) + React/Next.js.

Design docs and wireframes live under `docs/` (wireframes are interactive HTML/CSS
prototypes of every screen).

## License

MIT — see [LICENSE](LICENSE).
