import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Translation {
  source: string;
  text: string;
  cached: boolean;
  elapsed_ms: number;
  provider: string;
}

interface UiInfo {
  hotkey: string;
  provider: string;
  preview_debounce_ms: number;
  auto_paste: boolean;
  config_path: string;
  problem: string | null;
}

const input = document.querySelector<HTMLTextAreaElement>("#input")!;
const preview = document.querySelector<HTMLDivElement>("#preview")!;
const hint = document.querySelector<HTMLSpanElement>("#hint")!;
const meta = document.querySelector<HTMLSpanElement>("#meta")!;

const isMac = navigator.userAgent.includes("Mac");
const DEFAULT_HINT = `Enter 上屏 · ${isMac ? "⌘" : "Ctrl"}+Enter 仅复制 · Shift+Enter 换行 · Esc 取消`;

let debounceMs = 350;
let composing = false;
let committing = false;
let debounceTimer: number | undefined;
let hasProblem = false;

// One in-flight request per source text, so pressing Enter while a preview
// is still loading reuses that request instead of sending a second one.
const inflight = new Map<string, Promise<Translation>>();
let latest: Translation | null = null;
let seq = 0;

function translate(text: string): Promise<Translation> {
  let p = inflight.get(text);
  if (!p) {
    p = invoke<Translation>("translate", { text }).finally(() => inflight.delete(text));
    inflight.set(text, p);
  }
  return p;
}

function applyInfo(info: UiInfo) {
  debounceMs = info.preview_debounce_ms;
  hasProblem = !!info.problem;
  hint.textContent = info.problem ?? DEFAULT_HINT;
  hint.classList.toggle("problem", hasProblem);
  hint.title = hasProblem ? `点击打开配置文件：${info.config_path}` : "";
  meta.textContent = info.provider;
}

function setPreview(text: string, state: "ok" | "pending" | "error" = "ok") {
  preview.textContent = text;
  preview.classList.toggle("pending", state === "pending");
  preview.classList.toggle("error", state === "error");
}

function reset() {
  input.value = "";
  latest = null;
  seq++;
  clearTimeout(debounceTimer);
  setPreview("");
  autosize();
}

function autosize() {
  input.style.height = "auto";
  input.style.height = `${Math.min(input.scrollHeight, 84)}px`;
}

async function requestPreview() {
  const text = input.value.trim();
  const id = ++seq;
  if (!text) {
    latest = null;
    setPreview("");
    return;
  }
  if (latest && latest.source !== text) preview.classList.add("pending");
  try {
    const t = await translate(text);
    if (id !== seq) return; // a newer keystroke superseded this request
    latest = t;
    setPreview(t.text);
    meta.textContent = `${t.provider} · ${t.cached ? "缓存" : `${t.elapsed_ms} ms`}`;
  } catch (e) {
    if (id !== seq) return;
    setPreview(String(e), "error");
  }
}

async function commit(copyOnly: boolean) {
  if (committing) return;
  const text = input.value.trim();
  if (!text) {
    await invoke("hide_popup");
    return;
  }
  committing = true;
  try {
    clearTimeout(debounceTimer);
    let english = latest?.source === text ? latest.text : null;
    if (english === null) {
      setPreview(preview.textContent || "翻译中…", "pending");
      const t = await translate(text);
      english = t.text;
    }
    await invoke("commit", { text: english, copyOnly });
    reset();
  } catch (e) {
    // Keep the popup and the text so the user can retry or copy manually.
    setPreview(String(e), "error");
  } finally {
    committing = false;
  }
}

// IME-aware key handling: while the Chinese IME is composing, Enter picks a
// candidate and must not commit the translation.
input.addEventListener("compositionstart", () => (composing = true));
input.addEventListener("compositionend", () => {
  composing = false;
  schedulePreview();
});

input.addEventListener("input", () => {
  autosize();
  if (!composing) schedulePreview();
});

function schedulePreview() {
  clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(requestPreview, debounceMs);
}

input.addEventListener("keydown", (e) => {
  if (e.isComposing || composing || e.keyCode === 229) return;
  if (e.key === "Escape") {
    e.preventDefault();
    reset();
    invoke("hide_popup");
  } else if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    commit(isMac ? e.metaKey : e.ctrlKey);
  }
});

hint.addEventListener("click", async () => {
  if (!hasProblem) return;
  await invoke("open_config");
});

// Rust emits this every time the popup is shown.
listen<UiInfo>("popup:shown", (e) => {
  applyInfo(e.payload);
  input.focus();
  input.select();
});

// Re-check config when the window regains focus (e.g. after editing it).
window.addEventListener("focus", async () => {
  input.focus();
  if (hasProblem) applyInfo(await invoke<UiInfo>("reload_config"));
});

invoke<UiInfo>("get_ui_info").then(applyInfo);
input.focus();
