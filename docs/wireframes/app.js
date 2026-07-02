/* redrafter — shared interactive helpers (vanilla, no framework).
   Functions are global so pages can wire them via onclick. All feature blocks
   are guarded by element presence. */

/* ---------- Toast ---------- */
function toast(msg, kind) {
  var wrap = document.getElementById("toast-wrap");
  if (!wrap) return;
  var t = document.createElement("div");
  t.className = "toast";
  t.setAttribute("data-testid", "toast");
  var icon = kind === "error"
    ? '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" style="color:var(--orig)"><circle cx="12" cy="12" r="9"/><path d="M12 8v5M12 16h.01"/></svg>'
    : '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><path d="M5 12l5 5L20 6"/></svg>';
  t.innerHTML = icon + "<span></span>";
  t.querySelector("span").textContent = msg;
  wrap.appendChild(t);
  setTimeout(function () { t.classList.add("leaving"); setTimeout(function () { t.remove(); }, 240); }, 1900);
}

/* ---------- Modal open / close ---------- */
function openModal(id, focusId, opener) {
  var m = document.getElementById(id);
  if (!m) return;
  m.setAttribute("open", "");
  if (focusId) { var f = document.getElementById(focusId); if (f) f.focus(); }
}
function closeModal(id) { var m = document.getElementById(id); if (m) m.removeAttribute("open"); }

/* ---------- Theme ---------- */
function applyTheme(mode) {
  var prefersLight = window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches;
  var light = mode === "light" || (mode === "system" && prefersLight);
  document.documentElement.classList.toggle("light", light);
  try { localStorage.setItem("rd-theme", mode); } catch (e) {}
}
function setTheme(mode, btn) {
  applyTheme(mode);
  syncThemeSeg(mode);
}
/* keep the Appearance segmented control in sync with the current theme mode */
function syncThemeSeg(mode) {
  var seg = document.getElementById("theme-seg");
  if (!seg) return;
  seg.querySelectorAll("button").forEach(function (b) {
    b.classList.toggle("active", b.getAttribute("data-theme") === mode);
  });
}

/* ---------- Hotkey capture (inside #hotkey-modal) ---------- */
var _hkCapturing = false;
function startHotkeyCapture() {
  openModal("hotkey-modal");
  var field = document.getElementById("hotkey-capture");
  if (!field) return;
  field.textContent = "Press keys…";
  _hkCapturing = true;
  document.addEventListener("keydown", _hkKey, true);
}
function _hkKey(e) {
  if (!_hkCapturing) return;
  if (e.key === "Escape") return; // let global Esc close the modal
  e.preventDefault(); e.stopPropagation();
  if (["Control", "Meta", "Alt", "Shift"].indexOf(e.key) !== -1) return;
  var parts = [];
  if (e.ctrlKey) parts.push("⌃");
  if (e.altKey) parts.push("⌥");
  if (e.shiftKey) parts.push("⇧");
  if (e.metaKey) parts.push("⌘");
  var k = e.key === " " ? "Space" : (e.key.length === 1 ? e.key.toUpperCase() : e.key);
  parts.push(k);
  document.getElementById("hotkey-capture").textContent = parts.join("");
}
function stopHotkeyCapture() { _hkCapturing = false; document.removeEventListener("keydown", _hkKey, true); }
function saveHotkey() {
  var v = document.getElementById("hotkey-capture").textContent.trim();
  stopHotkeyCapture();
  if (v && v !== "Press keys…") {
    var disp = document.getElementById("hotkey-value");
    if (disp) disp.textContent = v;
    toast("Hotkey set to " + v);
  }
  closeModal("hotkey-modal");
}
function cancelHotkey() { stopHotkeyCapture(); closeModal("hotkey-modal"); }

/* ==========================================================================
   SINGLE SOURCE OF TRUTH — the enabled model set. Every model picker
   (tray, models table, preset override, fallback chain, re-refine menus)
   renders from this so they can never drift.
   ========================================================================== */
var ENABLED_MODELS = [
  { key: "opus",   id: "claude-opus-4-6",   provider: "Anthropic", active: true, favorite: true },
  { key: "sonnet", id: "claude-sonnet-4-6", provider: "Anthropic", favorite: true },
  { key: "gpt",    id: "gpt-5.1",           provider: "OpenAI" },
  { key: "gemini", id: "gemini-1.5-flash",  provider: "Google" },
  { key: "qwen",   id: "qwen3:8b",          provider: "Ollama", local: true, running: true, favorite: true },
  { key: "llama",  id: "llama3.1:8b",       provider: "Ollama", local: true, running: false }
];
function enabledProviders() {
  var order = [], seen = {};
  ENABLED_MODELS.forEach(function (m) { if (!seen[m.provider]) { seen[m.provider] = 1; order.push(m.provider); } });
  return order;
}
/* build <optgroup> option HTML for a <select>, grouped by provider.
   opts.inherit = a leading "Inherit (active model)" option (value "inherit"). */
function modelOptionsHTML(opts) {
  opts = opts || {};
  var html = "";
  if (opts.inherit) html += '<option value="inherit"' + (opts.inheritSelected ? " selected" : "") + '>Inherit (active model)</option>';
  enabledProviders().forEach(function (p) {
    html += '<optgroup label="' + p + '">';
    ENABLED_MODELS.filter(function (m) { return m.provider === p; }).forEach(function (m) {
      html += '<option value="' + m.id + '">' + m.id + '</option>';
    });
    html += '</optgroup>';
  });
  return html;
}
/* populate any <select data-model-options> from the enabled set (keeps a
   leading Inherit option if data-model-inherit is present). */
function fillModelSelects() {
  document.querySelectorAll("select[data-model-options]").forEach(function (sel) {
    var inherit = sel.hasAttribute("data-model-inherit");
    sel.innerHTML = modelOptionsHTML({ inherit: inherit, inheritSelected: inherit });
  });
}

/* ---------- Failure fallback chain (behavior.html) ---------- */
function addFallback() {
  var chain = document.getElementById("failure-fallback-chain");
  if (!chain) return;
  var n = chain.children.length + 1;
  var row = document.createElement("div");
  row.className = "opt-row";
  row.style.cssText = "padding:0;align-items:center;gap:8px";
  row.setAttribute("data-testid", "failure-fallback-" + n);
  row.innerHTML =
    '<span class="mono tiny muted" style="width:16px">' + n + '.</span>' +
    '<select class="input mono" aria-label="Fallback model ' + n + '" style="min-width:150px">' + modelOptionsHTML() + '</select>' +
    '<button type="button" class="btn btn--ghost btn--sm" data-testid="failure-fallback-remove" aria-label="Remove fallback" onclick="this.parentElement.remove()">' +
    '<svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 6l12 12M18 6L6 18"/></svg></button>';
  chain.appendChild(row);
}

/* ---------- Tray model switcher ---------- */
/* collapse/expand the "Active model" section (both tray.html and capture.html) */
function toggleTrayModels(row) {
  var list = document.getElementById(row.getAttribute("aria-controls"));
  var expanded = row.getAttribute("aria-expanded") === "true";
  setTrayModelsExpanded(row, list, !expanded);
}
function setTrayModelsExpanded(row, list, expanded) {
  if (!row) return;
  row.setAttribute("aria-expanded", String(expanded));
  if (list) list.hidden = !expanded;
  var caret = row.querySelector(".tray-caret");
  if (caret) caret.textContent = expanded ? "▾" : "▸";
}
function pickTrayModel(btn) {
  var group = btn.closest("[data-testid='tray-model-list']") || btn.parentElement;
  group.querySelectorAll("[role='menuitemradio']").forEach(function (r) {
    r.setAttribute("aria-checked", String(r === btn));
  });
  var label = btn.getAttribute("data-model-label");
  // update the collapsed "Active model" row (and tray.html header) to the picked model
  document.querySelectorAll("[data-active-model-label]").forEach(function (el) { el.textContent = label; });
  // collapse the list again after a pick
  var row = document.querySelector('[data-testid="tray-active-model"][aria-controls]');
  if (row) setTrayModelsExpanded(row, document.getElementById(row.getAttribute("aria-controls")), false);
  toast("Active model: " + label);
}

/* ---------- Active model radio (models.html) ---------- */
function pickActiveModel(btn) {
  var group = btn.closest("[data-testid='active-model']");
  group.querySelectorAll("[role='radio']").forEach(function (r) { r.setAttribute("aria-checked", String(r === btn)); });
}

/* ==========================================================================
   TRAY PAGE (tray.html) — rendered from the ENABLED_MODELS source of truth
   ========================================================================== */
function runBadge(m) {
  if (!m.local) return "";
  return m.running
    ? ' <span class="run-state running" style="font-size:11px">● running</span>'
    : ' <span class="run-state" style="font-size:11px">○ available</span>';
}
/* build the unified switcher: Favorites flat on top, then grouped by provider */
function renderTrayModels(container) {
  if (!container) return;
  var html = '<div class="tray__sec">★ Favorites</div>';
  ENABLED_MODELS.filter(function (m) { return m.favorite; }).forEach(function (m) {
    html += '<button class="menu__item" role="menuitemradio" aria-checked="' + (m.active ? "true" : "false") + '"' +
      ' data-testid="tray-fav-' + m.key + '" data-model-label="' + m.id + '" onclick="pickTrayModel(this)">' +
      '<span class="radio-mark" aria-hidden="true"></span> <span class="mono" style="flex:1">' + m.id + '</span>' +
      runBadge(m) + '<span style="color:var(--warning)">★</span></button>';
  });
  enabledProviders().forEach(function (p) {
    html += '<div class="tray__sec">' + p + '</div>';
    ENABLED_MODELS.filter(function (m) { return m.provider === p; }).forEach(function (m) {
      html += '<button class="menu__item" role="menuitemradio" aria-checked="false"' +
        ' data-testid="tray-model-' + m.key + '" data-model-label="' + m.id + '" onclick="pickTrayModel(this)">' +
        '<span class="radio-mark" aria-hidden="true"></span> <span class="mono" style="flex:1">' + m.id + '</span>' +
        runBadge(m) + '</button>';
    });
  });
  container.innerHTML = html;
}
/* menu-bar icon + header status + menu affordances */
function setTrayIconState(state, btn) {
  var icon = document.getElementById("menubar-icon");
  if (icon) {
    icon.setAttribute("data-state", state);
    var glyph = icon.querySelector(".mb-icon__glyph");
    var spin = icon.querySelector(".mb-icon__spin");
    var badge = icon.querySelector(".mb-icon__badge");
    if (spin) spin.hidden = state !== "refining";
    if (glyph) glyph.hidden = state === "refining";
    icon.classList.toggle("paused", state === "paused");
    if (badge) {
      if (state === "error") { badge.hidden = false; badge.style.background = "var(--error)"; }
      else if (state === "permission-needed") { badge.hidden = false; badge.style.background = "var(--warning)"; }
      else { badge.hidden = true; }
    }
  }
  // header status lines
  ["idle", "refining", "error", "perm", "paused"].forEach(function (s) {
    var el = document.getElementById("tray-status-" + s);
    if (!el) return;
    var match = (s === "perm") ? state === "permission-needed" : state === s;
    el.hidden = !match;
  });
  // affordances that appear only in certain states
  var retry = document.getElementById("tray-retry-last"); if (retry) retry.hidden = state !== "error";
  var permItem = document.getElementById("tray-perm-open-settings"); if (permItem) permItem.hidden = state !== "permission-needed";
  // refine disabled when paused or permission-needed
  var refine = document.getElementById("tray-refine");
  if (refine) {
    var off = (state === "paused" || state === "permission-needed");
    refine.setAttribute("aria-disabled", String(off));
    refine.classList.toggle("is-disabled", off);
  }
  // pause / resume item reflects paused state
  var pause = document.getElementById("tray-pause");
  var resume = document.getElementById("tray-resume");
  if (pause) pause.hidden = state === "paused";
  if (resume) resume.hidden = state !== "paused";
  if (btn) btn.parentElement.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}
function trayPause() { setTrayIconState("paused"); toast("Capturing paused"); syncIconSwitch("paused"); }
function trayResume() { setTrayIconState("idle"); toast("Capturing resumed"); syncIconSwitch("idle"); }
/* keep the scaffolding switcher's active button in sync when pausing/resuming from the menu */
function syncIconSwitch(state) {
  var sw = document.getElementById("tray-icon-state-switch");
  if (!sw) return;
  sw.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b.getAttribute("data-state") === state); });
}
/* Check-for-updates sub-states */
function setTrayUpdates(kind, btn) {
  ["idle", "checking", "uptodate", "available"].forEach(function (s) {
    var el = document.getElementById("tray-updates-" + s);
    if (el) el.hidden = s !== kind;
  });
  if (btn) btn.parentElement.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}
/* Launch-at-login checkable toggle */
function toggleLaunchLogin(btn) {
  var on = btn.getAttribute("aria-checked") !== "true";
  btn.setAttribute("aria-checked", String(on));
  var chk = btn.querySelector(".check");
  if (chk) chk.style.opacity = on ? "1" : "0";
}

/* ---------- Capture state switching ---------- */
function showCaptureState(name, btn) {
  document.querySelectorAll("[data-capture-state]").forEach(function (p) {
    p.hidden = p.getAttribute("data-capture-state") !== name;
  });
  var sw = document.getElementById("capture-state-switch");
  if (sw) sw.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}

/* ---------- Tray status (idle / refining / perm-lost) ---------- */
function setTrayStatus(kind, btn) {
  var idle = document.getElementById("tray-status-idle");
  var refining = document.getElementById("tray-status-refining");
  var permLost = document.getElementById("tray-perm-lost");
  if (idle) idle.hidden = kind !== "idle";
  if (refining) refining.hidden = kind !== "refining";
  if (permLost) permLost.hidden = kind !== "perm-lost";
  if (btn) { var sw = btn.parentElement; sw.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); }); }
}

/* ---------- OS notification mocks (capture.html) ---------- */
function showNotification(kind, btn) {
  var c = document.getElementById("notification-complete");
  var f = document.getElementById("notification-failure");
  if (c) c.hidden = kind !== "complete";
  if (f) f.hidden = kind !== "failure";
  if (btn) { var sw = btn.parentElement; sw.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); }); }
}

/* ---------- Cursor HUD pill (capture.html) ---------- */
function showCursorHud(kind, btn) {
  var hud = document.getElementById("cursor-hud");
  var refining = document.getElementById("cursor-hud-refining");
  var done = document.getElementById("cursor-hud-done");
  if (hud) hud.hidden = kind === "none";
  if (refining) refining.hidden = kind !== "refining";
  if (done) done.hidden = kind !== "done";
  if (btn) btn.parentElement.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}

/* ---------- Favorite (star) toggle ---------- */
function toggleStar(btn) {
  var on = btn.getAttribute("aria-pressed") !== "true";
  btn.setAttribute("aria-pressed", String(on));
  btn.textContent = on ? "★" : "☆";
  btn.style.color = on ? "var(--warning)" : "";
}

/* ---------- generic segmented radio (aria-checked) ---------- */
function pickSeg(btn) {
  btn.parentElement.querySelectorAll("button").forEach(function (b) {
    b.classList.toggle("active", b === btn);
    b.setAttribute("aria-checked", String(b === btn));
  });
}

/* ---------- index: active-model summary state ---------- */
function setActiveModelState(state, btn) {
  var set = document.getElementById("active-model-set");
  var none = document.getElementById("active-model-none");
  var noneConnected = document.getElementById("active-model-none-connected");
  if (set) set.hidden = state !== "set";
  if (none) none.hidden = state !== "none"; // no connection → Connections
  if (noneConnected) noneConnected.hidden = state !== "none-connected"; // connected, none active → Models
  if (btn) btn.parentElement.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}

/* ---------- index: hotkey conflict scaffolding ---------- */
function toggleHotkeyConflict() {
  var c = document.getElementById("hotkey-conflict");
  var override = document.getElementById("hotkey-conflict-override");
  var on = c && c.hidden;
  if (c) c.hidden = !on;
  if (override) override.hidden = !on;
}

/* ---------- connections: state / modal / test ---------- */
function setConnectionsState(state, btn) {
  var pop = document.getElementById("connections-populated");
  var empty = document.getElementById("connections-empty-sec");
  if (pop) pop.hidden = state !== "list";
  if (empty) empty.hidden = state !== "empty";
  if (btn) btn.parentElement.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}
var CONN_BASE = { "Anthropic": "https://api.anthropic.com", "OpenAI": "https://api.openai.com/v1", "Google Gemini": "https://generativelanguage.googleapis.com", "OpenAI-compatible": "", "Ollama": "http://localhost:11434" };
function prefillConnBase() {
  var type = document.getElementById("conn-provider-type").value;
  var base = document.getElementById("conn-base-url");
  if (base && CONN_BASE[type] !== undefined) base.value = CONN_BASE[type];
  // Ollama local: no key needed
  var keyField = document.getElementById("conn-key-field");
  if (keyField) keyField.style.display = type === "Ollama" ? "none" : "";
}
function openConnectionModal(mode, data) {
  var title = document.getElementById("connection-modal-title");
  var saveLbl = document.getElementById("connection-save-label");
  var remove = document.getElementById("connection-remove");
  var type = document.getElementById("conn-provider-type");
  var base = document.getElementById("conn-base-url");
  var key = document.getElementById("conn-api-key");
  // no test result until the user clicks "Test connection"
  var _ok = document.getElementById("conn-test-ok"); if (_ok) _ok.hidden = true;
  var _err = document.getElementById("conn-test-error"); if (_err) _err.hidden = true;
  if (mode === "edit" && data) {
    if (title) title.textContent = "Edit connection";
    if (saveLbl) saveLbl.textContent = "Save changes";
    if (remove) remove.hidden = false;
    if (type) type.value = data.type;
    if (base) base.value = data.base || "";
    if (key) key.value = data.key || "";
    var nm = document.getElementById("conn-remove-name"); if (nm) nm.textContent = data.name || "this connection";
    var kf = document.getElementById("conn-key-field"); if (kf) kf.style.display = data.type === "Ollama" ? "none" : "";
  } else {
    if (title) title.textContent = "Add connection";
    if (saveLbl) saveLbl.textContent = "Add connection";
    if (remove) remove.hidden = true;
    if (type) type.selectedIndex = 0;
    prefillConnBase();
    if (key) key.value = "";
  }
  openModal("connection-modal");
}
function connTest(outcome, btn) {
  var ok = document.getElementById("conn-test-ok");
  var err = document.getElementById("conn-test-error");
  if (ok) ok.hidden = outcome !== "ok";
  if (err) err.hidden = outcome !== "error";
  if (btn) btn.parentElement.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}

/* ---------- models: state banners + ollama pull ---------- */
function setModelsState(state, btn) {
  var normal = document.getElementById("models-normal");
  var empty = document.getElementById("models-empty-sec");
  var banner = document.getElementById("models-no-active-banner");
  var gone = document.getElementById("models-active-unavailable");
  if (normal) normal.hidden = state === "empty";
  if (empty) empty.hidden = state !== "empty";
  if (banner) banner.hidden = state !== "no-active";
  if (gone) gone.hidden = state !== "active-unavailable";
  // keep the table honest about which radio is active
  var radios = document.querySelectorAll("[data-testid^='model-active-radio-']");
  if (state === "no-active" || state === "active-unavailable") {
    // active model gone / none picked → nothing selected
    radios.forEach(function (r) { r.setAttribute("aria-checked", "false"); });
  } else if (state === "normal") {
    radios.forEach(function (r) {
      r.setAttribute("aria-checked", String(r.getAttribute("data-testid") === "model-active-radio-opus"));
    });
  }
  if (btn) btn.parentElement.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}
/* disable / remove an enabled model from the curation table */
function disableModel(btn, model) {
  var row = btn.closest("tr");
  if (row) row.remove();
  toast("Disabled " + model);
}
/* preset stale-pin warning toggle (presets.html scaffolding) */
function togglePresetStale(show, sw) {
  var chip = document.getElementById("preset-model-stale");
  if (chip) chip.hidden = !show;
  if (sw) sw.parentElement.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === sw); });
}
/* fallback stale row toggle (behavior.html scaffolding) */
function toggleFallbackStale(show, sw) {
  var row = document.querySelector('[data-testid="failure-fallback-stale"]');
  if (row) row.hidden = !show;
  if (sw) sw.parentElement.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === sw); });
}
function ollamaPull(state, btn) {
  ["idle", "progress", "done", "error"].forEach(function (s) {
    var el = document.getElementById("ollama-pull-" + s);
    if (el) el.hidden = true;
  });
  var key = state === "pulling" ? "progress" : state;
  var show = document.getElementById("ollama-pull-" + key);
  if (show) show.hidden = false;
  if (btn) btn.parentElement.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}

/* ---------- first-run chooser ---------- */
function firstrunChoose(kind, btn) {
  var cloud = document.getElementById("firstrun-cloud-panel");
  var local = document.getElementById("firstrun-local-panel");
  if (cloud) cloud.hidden = kind !== "cloud";
  if (local) local.hidden = kind !== "local";
  if (btn) btn.parentElement.querySelectorAll("[role='tab']").forEach(function (b) { b.classList.toggle("active", b === btn); b.setAttribute("aria-selected", String(b === btn)); });
}
function firstrunProvider(btn, name, url) {
  btn.parentElement.querySelectorAll("[role='radio']").forEach(function (b) { b.classList.toggle("active", b === btn); b.setAttribute("aria-checked", String(b === btn)); });
  var nm = document.getElementById("firstrun-provider-name"); if (nm) nm.textContent = name;
}
function firstrunConnected() {
  var c = document.getElementById("firstrun-connected");
  if (c) c.hidden = false;
  toast("Connected · default model enabled");
}
function firstrunOllama(state, btn) {
  var det = document.getElementById("firstrun-ollama-detected");
  var miss = document.getElementById("firstrun-ollama-missing");
  if (det) det.hidden = state !== "detected";
  if (miss) miss.hidden = state !== "missing";
  if (btn) btn.parentElement.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}

/* ---------- Menu-bar tray popover (capture.html) ---------- */
function toggleTray(e) {
  if (e) e.stopPropagation();
  var m = document.getElementById("tray-popover");
  if (!m) return;
  m.hidden = !m.hidden;
  var b = document.getElementById("tray-btn");
  if (b) b.setAttribute("aria-expanded", String(!m.hidden));
}

/* ---------- History: re-refine menu + state toggles ---------- */
function toggleRerefine(e, btn) {
  e.stopPropagation();
  var menu = btn.parentElement.querySelector(".popover");
  var open = menu.hidden;
  document.querySelectorAll(".hrow__actions .popover").forEach(function (p) { p.hidden = true; });
  menu.hidden = !open;
  btn.setAttribute("aria-expanded", String(open));
}
function rerefineWith(label) {
  document.querySelectorAll(".hrow__actions .popover").forEach(function (p) { p.hidden = true; });
  toast("Re-refining with " + label + "…");
}
function setHistoryState(name, btn) {
  var populated = name === "populated";
  var list = document.getElementById("history-list");
  var empty = document.getElementById("history-empty");
  var noRes = document.getElementById("history-no-results");
  if (list) list.hidden = name !== "populated";
  if (empty) empty.hidden = name !== "empty";
  if (noRes) noRes.hidden = name !== "no-results";
  var sw = document.getElementById("history-state-switch");
  if (sw) sw.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}
function historySearch(v) {
  // "zzz" demonstrates the no-results branch
  if (v && v.trim().toLowerCase() === "zzz") { setHistoryState("no-results"); }
  else { setHistoryState("populated"); }
}

/* ---------- Presets: select / new / save / examples ---------- */
/* per-preset language + few-shot examples (built-ins are language-neutral English) */
var PRESET_CONTENT = {
  "/formal": { lang: "", examples: [{ before: "thanks, sounds good, i'll check the numbers and get back to you tomorrow", after: "Thank you — that sounds good. I'll review the figures and follow up tomorrow." }] },
  "/concise": { lang: "", examples: [{ before: "I just wanted to quickly reach out and see if maybe you had a chance to look at the doc I sent over the other day", after: "Have you had a chance to review the doc I sent?" }] },
  "/friendly": { lang: "", examples: [{ before: "Attached is the report. Let me know if changes are required.", after: "Here's the report! Happy to tweak anything — just let me know." }] },
  "/bullets": { lang: "", examples: [{ before: "we shipped the api, fixed the login bug, and started on the dashboard which is about half done", after: "• Shipped the API\n• Fixed the login bug\n• Dashboard ~50% done" }] },
  "/reply": { lang: "", examples: [{ before: "> Can you send the Q3 numbers? — my rough: yeah will do by eod", after: "Sure — I'll send the Q3 numbers by end of day." }] },
  "/reply-de": { lang: "de", examples: [
    { before: "thanks, sounds good, i'll check the numbers and get back to you tomorrow", after: "Danke, klingt gut! Ich prüfe die Zahlen und melde mich morgen bei dir." },
    { before: "sorry for late reply, we can do the meeting friday at 3 works for me", after: "Entschuldige die späte Antwort! Freitag um 15 Uhr passt mir gut für das Meeting." }
  ] },
  "/exec-summary": { lang: "", examples: [{ before: "the launch went well overall, we had a few support tickets but resolved them, revenue is up 12% since", after: "• Launch landed smoothly; minor support tickets, all resolved\n• Revenue up 12% since launch" }] },
  "/standup": { lang: "", examples: [{ before: "yesterday finished the auth refactor, today starting on the billing page, no blockers rn", after: "Yesterday: finished the auth refactor.\nToday: starting the billing page.\nBlockers: none." }] }
};
function renderPresetExamples(content) {
  var list = document.getElementById("preset-examples-list");
  if (!list) return;
  list.innerHTML = "";
  var lang = (content && content.lang) ? content.lang : "";
  ((content && content.examples) || []).forEach(function (ex) {
    var pair = document.createElement("div");
    pair.className = "example-pair";
    pair.setAttribute("data-testid", "preset-example");
    var langTag = lang ? ' · <span class="tag tag--lang">' + lang + '</span>' : '';
    pair.innerHTML =
      '<div class="diff-block"><span class="lbl before">Before</span><div class="diff"><span class="diff__orig"></span></div></div>' +
      '<div class="diff-block"><span class="lbl after">After' + langTag + '</span><div class="diff"><span class="diff__refined" style="white-space:pre-line"></span></div></div>' +
      '<div style="display:flex;justify-content:flex-end"><button type="button" class="btn btn--ghost btn--sm" onclick="this.closest(&quot;.example-pair&quot;).remove()">Remove</button></div>';
    pair.querySelector(".diff__orig").textContent = ex.before;
    pair.querySelector(".diff__refined").textContent = ex.after;
    list.appendChild(pair);
  });
}

/* show/hide the built-in badge + modify warning */
function setBuiltinUI(isBuiltin) {
  var editor = document.getElementById("preset-editor");
  if (editor) editor.dataset.presetKind = isBuiltin ? "builtin" : "user";
  var badge = document.getElementById("preset-builtin-badge");
  var warn = document.getElementById("preset-builtin-warning");
  if (badge) badge.hidden = !isBuiltin;
  if (warn) warn.hidden = !isBuiltin;
}
function selectPreset(btn) {
  document.querySelectorAll(".preset-item").forEach(function (b) { b.setAttribute("aria-selected", String(b === btn)); });
  var editor = document.getElementById("preset-editor");
  if (editor) { editor.dataset.mode = "edit"; editor.dataset.originalTrigger = btn.dataset.trigger; }
  set("#preset-editor-title", "Editing " + btn.dataset.trigger, true);
  setVal("#preset-name", btn.dataset.trigger);
  setVal("#preset-direction", btn.dataset.direction || "");
  // load this preset's own language + examples (built-ins are English / language-neutral)
  var content = PRESET_CONTENT[btn.dataset.trigger] || { lang: "", examples: [] };
  setVal("#preset-lang", content.lang || "");
  renderPresetExamples(content);
  var ex = document.getElementById("preset-examples");
  if (ex) ex.hidden = false;
  setBuiltinUI(btn.dataset.kind === "builtin");
  setStatus("", ""); // clean state — "unsaved changes" only appears after an edit
  clearSaveFeedback();
}
function newPreset() {
  document.querySelectorAll(".preset-item").forEach(function (b) { b.setAttribute("aria-selected", "false"); });
  var editor = document.getElementById("preset-editor");
  if (editor) { editor.dataset.mode = "new"; editor.dataset.originalTrigger = ""; }
  set("#preset-editor-title", "Editing new preset", false);
  var n = document.getElementById("preset-name"); if (n) { n.value = ""; n.focus(); }
  setVal("#preset-direction", "");
  var ex = document.getElementById("preset-examples"); if (ex) ex.hidden = true;
  setBuiltinUI(false); // new presets are always user presets
  setStatus("new · not saved", "");
  clearSaveFeedback();
}
function savePreset() {
  var nameEl = document.getElementById("preset-name");
  var name = (nameEl && nameEl.value || "").trim();
  // (a) empty name
  if (!name) {
    setStatus("Name required", "err");
    showSaveFeedback("Name required — give the preset a trigger like /reply", "err");
    if (nameEl) nameEl.focus();
    return;
  }
  // Derive the preset being edited from the selected list item (reliable attribute
  // reads) rather than mutable editor dataset — keeping its OWN trigger is valid.
  var selected = document.querySelector('.preset-item[aria-selected="true"]');
  var selfTrigger = selected ? (selected.dataset.trigger || "") : "";
  // (b) trigger collides with a DIFFERENT preset
  if (name !== selfTrigger) {
    var dup = false;
    document.querySelectorAll(".preset-item").forEach(function (b) {
      if (b !== selected && b.dataset.trigger === name) dup = true;
    });
    if (dup) {
      setStatus("Trigger exists", "err");
      showSaveFeedback("Trigger " + name + " already exists — choose another", "err");
      return;
    }
  }
  // (c) editing a BUILT-IN → confirm override first (do not save yet)
  if (selected && selected.dataset.kind === "builtin") {
    var nm = document.getElementById("preset-override-name"); if (nm) nm.textContent = name;
    openModal("preset-override-modal");
    return;
  }
  // (d) valid user preset → save directly
  commitSave();
}
function commitSave() {
  var nameEl = document.getElementById("preset-name");
  var name = (nameEl && nameEl.value || "").trim();
  setStatus("Saved ✓", "ok");
  showSaveFeedback("Saved " + name, "ok");
  var ex = document.getElementById("preset-examples"); if (ex) ex.hidden = false;
  toast("Preset " + name + " saved");
}
function duplicatePreset() {
  var editor = document.getElementById("preset-editor");
  var nameEl = document.getElementById("preset-name");
  var base = (nameEl && nameEl.value || "/preset").trim();
  var copy = base + "-copy";
  if (nameEl) nameEl.value = copy;
  if (editor) { editor.dataset.mode = "new"; editor.dataset.originalTrigger = ""; }
  document.querySelectorAll(".preset-item").forEach(function (b) { b.setAttribute("aria-selected", "false"); });
  set("#preset-editor-title", "Editing " + copy, true);
  setBuiltinUI(false); // duplicate becomes an editable user preset
  setStatus("new · not saved", "");
  clearSaveFeedback();
  if (nameEl) nameEl.focus();
  toast("Duplicated as " + copy);
}
function resetPresetDefault() {
  var selected = document.querySelector('.preset-item[aria-selected="true"]');
  if (selected) setVal("#preset-direction", selected.dataset.direction || "");
  setStatus("Reset to default", "");
  clearSaveFeedback();
  toast("Reset to default");
}
function addExample() {
  var box = document.getElementById("preset-examples-list");
  if (!box) return;
  var div = document.createElement("div");
  div.className = "example-pair";
  div.setAttribute("data-testid", "preset-example");
  div.innerHTML =
    '<div class="diff-block"><span class="lbl before">Before</span>' +
    '<input class="input" aria-label="Example before" placeholder="paste your rough text…"></div>' +
    '<div class="diff-block"><span class="lbl after">After</span>' +
    '<input class="input" aria-label="Example after" placeholder="the refined result…"></div>' +
    '<div style="display:flex;justify-content:flex-end"><button type="button" class="btn btn--ghost btn--sm" ' +
    'onclick="this.closest(&quot;.example-pair&quot;).remove()">Remove</button></div>';
  box.appendChild(div);
  var inp = div.querySelector("input"); if (inp) inp.focus();
}
/* preset helpers */
function set(sel, text, mono) {
  var el = document.querySelector(sel); if (!el) return;
  if (mono) { el.innerHTML = 'Editing <span class="mono" style="color:var(--primary)"></span>'; el.querySelector("span").textContent = text.replace(/^Editing /, ""); }
  else { el.textContent = text; }
}
function setVal(sel, v) { var el = document.querySelector(sel); if (el) el.value = v; }
function setStatus(text, cls) {
  var el = document.getElementById("preset-status"); if (!el) return;
  if (!text) { el.hidden = true; el.textContent = ""; el.className = "chip"; return; }
  el.hidden = false; el.className = "chip" + (cls ? " " + cls : ""); el.textContent = text;
}
/* mark the editor dirty when the user actually edits a field */
function presetMarkDirty() {
  var el = document.getElementById("preset-status");
  // don't clobber a success/error message that's already showing
  if (el && (el.classList.contains("ok") || el.classList.contains("err"))) return;
  setStatus("unsaved changes", "");
}
function showSaveFeedback(text, cls) {
  var el = document.getElementById("preset-save-feedback"); if (!el) return;
  el.hidden = false; el.className = "chip " + cls;
  el.innerHTML = '<span class="dot ' + (cls === "ok" ? "status-dot green" : "status-dot red") + '"></span>';
  var s = document.createElement("span"); s.textContent = text; el.appendChild(s);
}
function clearSaveFeedback() { var el = document.getElementById("preset-save-feedback"); if (el) { el.hidden = true; el.innerHTML = ""; } }

/* ---------- Onboarding: permission grant toggle ---------- */
function grantPermission() {
  var status = document.getElementById("perm-status");
  var cont = document.getElementById("perm-continue");
  if (status) {
    status.setAttribute("data-granted", "true");
    var dot = status.querySelector(".status-dot");
    var txt = status.querySelector("[data-perm-text]");
    var btn = document.getElementById("perm-open-settings");
    if (dot) { dot.classList.remove("red"); dot.classList.add("green"); }
    if (txt) txt.textContent = "Granted";
    if (btn) { btn.textContent = "Granted ✓"; btn.classList.add("btn--ghost"); btn.setAttribute("aria-disabled", "true"); }
  }
  if (cont) { cont.removeAttribute("aria-disabled"); cont.removeAttribute("tabindex"); cont.classList.remove("btn--ghost"); cont.classList.add("btn--primary"); }
  toast("Accessibility granted");
}

/* ---------- Import result state switch (presets import modal) ---------- */
function showImportResult(kind, btn) {
  var clean = document.getElementById("import-result-clean");
  var conflict = document.getElementById("import-result-conflict");
  if (clean) clean.hidden = kind !== "clean";
  if (conflict) conflict.hidden = kind !== "conflict";
  var sw = document.getElementById("import-result-switch");
  if (sw) sw.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
}

/* copy helper */
function copyCode(btn, msg) { toast(msg || "Copied to clipboard"); }

/* ---------- Global wiring (runs on every page) ---------- */
(function () {
  // populate every model picker from the single enabled-model source of truth
  fillModelSelects();

  // tray.html: render the unified model switcher + initialise the icon state
  var trayList = document.querySelector('[data-testid="tray-model-list"][data-tray-render]');
  if (trayList) { renderTrayModels(trayList); setTrayIconState("idle"); }

  // theme toggle button on the rail
  var tt = document.getElementById("theme-toggle");
  if (tt) tt.addEventListener("click", function () {
    var isLight = document.documentElement.classList.toggle("light");
    var mode = isLight ? "light" : "dark";
    try { localStorage.setItem("rd-theme", mode); } catch (e) {}
    syncThemeSeg(mode); // keep the Appearance control in step with the rail toggle
  });

  // sync theme segmented control (if present) to stored value
  var seg = document.getElementById("theme-seg");
  if (seg) {
    var saved; try { saved = localStorage.getItem("rd-theme"); } catch (e) {}
    var mode = saved === "light" ? "light" : saved === "system" ? "system" : "dark";
    syncThemeSeg(mode);
  }

  // backdrop click closes modals
  document.querySelectorAll(".modal-back").forEach(function (mb) {
    mb.addEventListener("click", function (e) { if (e.target === mb) mb.removeAttribute("open"); });
  });

  // preset editor: mark "unsaved changes" only after a real user edit
  var presetEditor = document.getElementById("preset-editor");
  if (presetEditor) {
    presetEditor.addEventListener("input", presetMarkDirty);
    presetEditor.addEventListener("change", presetMarkDirty);
  }

  // Escape: close modals + popovers
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") {
      document.querySelectorAll(".modal-back[open]").forEach(function (m) { m.removeAttribute("open"); });
      document.querySelectorAll(".popover").forEach(function (p) { if (!p.hasAttribute("data-static")) p.hidden = true; });
      var tp = document.getElementById("tray-popover"); if (tp && !tp.hidden) tp.hidden = true;
      if (_hkCapturing) cancelHotkey();
    }
  });

  // click-outside closes tray + re-refine menus
  document.addEventListener("click", function (e) {
    if (!e.target.closest("#tray-btn") && !e.target.closest("#tray-popover")) {
      var tp = document.getElementById("tray-popover"); if (tp) tp.hidden = true;
    }
    if (!e.target.closest(".hrow__actions")) {
      document.querySelectorAll(".hrow__actions .popover").forEach(function (p) { p.hidden = true; });
    }
  });
})();
