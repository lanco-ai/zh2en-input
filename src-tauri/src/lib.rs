//! zh2en-input desktop app: a tray app with a hidden popup window.
//!
//! Hotkey → popup opens (remembering the window you were in) → type Chinese
//! with your usual IME → a live English preview appears → Enter → popup hides,
//! focus returns, English is pasted.

mod commit;
mod focus;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_opener::OpenerExt;
use zh2en_core::{Config, Engine, Translation};

use commit::{CommitJob, Committer};
use focus::PrevWindow;

const POPUP: &str = "popup";

struct AppState {
    config_path: PathBuf,
    config: RwLock<Config>,
    /// The engine, or the reason it could not be built (e.g. no API key).
    engine: RwLock<Result<Arc<Engine>, String>>,
    hotkey_error: RwLock<Option<String>>,
    /// Hotkey currently registered, so a reload only re-registers on change.
    registered_hotkey: Mutex<Option<String>>,
    /// Config file mtime at last load; the popup reloads when it changes.
    config_mtime: Mutex<Option<SystemTime>>,
    prev_window: Mutex<Option<PrevWindow>>,
    shown_at: Mutex<Option<Instant>>,
    committer: Committer,
}

impl AppState {
    fn engine(&self) -> Result<Arc<Engine>, String> {
        self.engine.read().unwrap().clone()
    }
}

/// What the popup needs to know about the current setup.
#[derive(Serialize, Clone)]
struct UiInfo {
    hotkey: String,
    provider: String,
    preview_debounce_ms: u64,
    auto_paste: bool,
    config_path: String,
    /// A setup problem to show instead of the normal hint line.
    problem: Option<String>,
}

fn ui_info(state: &AppState) -> UiInfo {
    let cfg = state.config.read().unwrap();
    let engine = state.engine();
    let problem = match (&engine, state.hotkey_error.read().unwrap().as_ref()) {
        (Err(e), _) => Some(e.clone()),
        (_, Some(h)) => Some(h.clone()),
        _ => None,
    };
    UiInfo {
        hotkey: cfg.hotkey.clone(),
        provider: engine.map(|e| e.provider_name().to_string()).unwrap_or_else(|_| "—".into()),
        preview_debounce_ms: cfg.preview_debounce_ms,
        auto_paste: cfg.auto_paste,
        config_path: state.config_path.display().to_string(),
        problem,
    }
}

// ---------------------------------------------------------------- commands

#[tauri::command]
async fn translate(state: State<'_, AppState>, text: String) -> Result<Translation, String> {
    let engine = state.engine()?;
    engine.translate(&text).await.map_err(|e| e.to_string())
}

/// Hide the popup and deliver `text`. `copy_only` skips pasting.
#[tauri::command]
async fn commit(app: AppHandle, state: State<'_, AppState>, text: String, copy_only: bool) -> Result<(), String> {
    let (deliver, method, paste_delay_ms, restore_clipboard) = {
        let cfg = state.config.read().unwrap();
        (cfg.auto_paste && !copy_only, cfg.commit_method, cfg.paste_delay_ms, cfg.restore_clipboard)
    };
    let prev = state.prev_window.lock().unwrap().take();
    hide_popup_window(&app, true);
    state
        .committer
        .commit(|reply| CommitJob { text, deliver, method, prev, paste_delay_ms, restore_clipboard, reply })
        .await
}

#[tauri::command]
fn hide_popup(app: AppHandle) {
    hide_popup_window(&app, true);
}

#[tauri::command]
fn get_ui_info(state: State<'_, AppState>) -> UiInfo {
    ui_info(&state)
}

#[tauri::command]
fn open_config(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    open_config_file(&app, &state)
}

#[tauri::command]
fn reload_config(app: AppHandle) -> UiInfo {
    apply_config(&app);
    ui_info(&app.state::<AppState>())
}

// ------------------------------------------------------------ popup window

fn show_popup(app: &AppHandle) {
    let Some(w) = app.get_webview_window(POPUP) else { return };
    let state = app.state::<AppState>();
    // Pick up edits to the config file without a restart.
    if config_mtime(&state.config_path) != *state.config_mtime.lock().unwrap() {
        apply_config(app);
    }
    if !w.is_visible().unwrap_or(false) {
        // Remember the target window before we steal focus from it.
        *state.prev_window.lock().unwrap() = focus::capture();
    }
    *state.shown_at.lock().unwrap() = Some(Instant::now());
    let _ = w.center();
    let _ = w.show();
    let _ = w.unminimize();
    let _ = w.set_focus();
    let _ = w.emit("popup:shown", ui_info(&state));
}

fn hide_popup_window(app: &AppHandle, return_focus: bool) {
    if let Some(w) = app.get_webview_window(POPUP) {
        let _ = w.hide();
    }
    // On macOS, hiding the whole (accessory) app gives focus back to the
    // app the user was in.
    #[cfg(target_os = "macos")]
    if return_focus {
        let _ = app.hide();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = return_focus;
}

fn toggle_popup(app: &AppHandle) {
    let visible = app
        .get_webview_window(POPUP)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide_popup_window(app, true);
    } else {
        show_popup(app);
    }
}

// ------------------------------------------------------------------ config

fn open_config_file(app: &AppHandle, state: &AppState) -> Result<(), String> {
    app.opener()
        .open_path(state.config_path.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

fn config_mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// (Re)load the config file, rebuild the engine and re-register the hotkey
/// if it changed.
fn apply_config(app: &AppHandle) {
    let state = app.state::<AppState>();
    let cfg = match Config::load_or_create(&state.config_path) {
        Ok(c) => c,
        Err(e) => {
            *state.engine.write().unwrap() = Err(e.to_string());
            return;
        }
    };
    *state.engine.write().unwrap() = Engine::from_config(&cfg).map(Arc::new).map_err(|e| e.to_string());
    *state.config_mtime.lock().unwrap() = config_mtime(&state.config_path);

    let mut registered = state.registered_hotkey.lock().unwrap();
    if registered.as_deref() != Some(cfg.hotkey.as_str()) {
        let gs = app.global_shortcut();
        if let Some(old) = registered.take() {
            let _ = gs.unregister(old.as_str());
        }
        match gs.register(cfg.hotkey.as_str()) {
            Ok(()) => {
                *registered = Some(cfg.hotkey.clone());
                *state.hotkey_error.write().unwrap() = None;
            }
            Err(e) => {
                *state.hotkey_error.write().unwrap() =
                    Some(format!("快捷键 {} 注册失败（可能被其他程序占用）：{e}", cfg.hotkey));
            }
        }
    }
    drop(registered);

    *state.config.write().unwrap() = cfg;
}

// --------------------------------------------------------------------- app

pub fn run() {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_popup(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_popup(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![translate, commit, hide_popup, get_ui_info, open_config, reload_config])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let config_path = app.path().app_config_dir()?.join("config.json");
            tracing::info!("config: {}", config_path.display());
            app.manage(AppState {
                config_path,
                config: RwLock::new(Config::default()),
                engine: RwLock::new(Err("loading".into())),
                hotkey_error: RwLock::new(None),
                registered_hotkey: Mutex::new(None),
                config_mtime: Mutex::new(None),
                prev_window: Mutex::new(None),
                shown_at: Mutex::new(None),
                committer: Committer::spawn(),
            });
            apply_config(app.handle());

            build_tray(app.handle())?;

            // Show once on first launch so the user sees it's running and
            // any setup problem (missing API key) right away.
            show_popup(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != POPUP {
                return;
            }
            match event {
                // Clicking elsewhere dismisses the popup, like Spotlight.
                // Ignore blur right after showing: some window managers
                // briefly bounce focus while mapping the window.
                WindowEvent::Focused(false) => {
                    let app = window.app_handle();
                    let state = app.state::<AppState>();
                    let recent = state
                        .shown_at
                        .lock()
                        .unwrap()
                        .is_some_and(|t| t.elapsed() < Duration::from_millis(300));
                    if !recent {
                        hide_popup_window(app, false);
                    }
                }
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    hide_popup_window(window.app_handle(), true);
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running zh2en-input");
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let hotkey = app.state::<AppState>().config.read().unwrap().hotkey.clone();
    let show = MenuItem::with_id(app, "show", format!("打开输入框\t{hotkey}"), true, None::<&str>)?;
    let edit = MenuItem::with_id(app, "edit", "编辑配置文件…", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "重新加载配置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &sep, &edit, &reload, &sep, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("zh2en-input · 中文输入，英文上屏")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_popup(app),
            "edit" => {
                let _ = open_config_file(app, &app.state::<AppState>());
            }
            "reload" => {
                apply_config(app);
                show_popup(app);
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
