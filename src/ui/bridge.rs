use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use tokio::{runtime::Handle as TokioHandle, sync::RwLock};
use tracing::{error, info};

// ─── 臨時 stderr 抑制（Slint/parley→ICU4X 啟動雜訊）──────────────────────
//
// ICU4X v2.2 的 icu_segmenter 缺少 ja 語系的 segmentation 模型，
// 但 Slint 在 AppWindow::new() 時會觸發 text layout → parley → icu_segmenter，
// 噴出 60+ 行 "ICU4X data error: No segmentation model for language: ja"。
// 這些錯誤走 eprintln!，繞過 tracing/log 過濾器，純屬非功能性雜訊。
// 解法：在 AppWindow::new() 期間暫時 redirect stderr → NUL。

#[cfg(windows)]
/// Guard 物件：建構時將 stderr 重新導向至 NUL，drop 時恢復。
struct IcuStderrGuard {
    old_handle: *mut winapi::ctypes::c_void,
    null_handle: *mut winapi::ctypes::c_void,
}

#[cfg(windows)]
impl IcuStderrGuard {
    fn new() -> Option<Self> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
        use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
        use winapi::um::processenv::{GetStdHandle, SetStdHandle};
        use winapi::um::winbase::STD_ERROR_HANDLE;
        use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_WRITE};

        // SAFETY: 僅在初始化主執行緒上呼叫，無併發競爭。
        unsafe {
            let null_path: Vec<u16> = OsStr::new("NUL").encode_wide().chain(Some(0)).collect();
            let raw_null = CreateFileW(
                null_path.as_ptr(),
                GENERIC_WRITE,
                FILE_SHARE_WRITE | FILE_SHARE_READ,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );

            if raw_null == INVALID_HANDLE_VALUE {
                return None;
            }

            let raw_old = GetStdHandle(STD_ERROR_HANDLE);
            if raw_old.is_null() || raw_old == INVALID_HANDLE_VALUE {
                CloseHandle(raw_null);
                return None;
            }

            SetStdHandle(STD_ERROR_HANDLE, raw_null);

            Some(Self { old_handle: raw_old, null_handle: raw_null })
        }
    }
}

#[cfg(windows)]
impl Drop for IcuStderrGuard {
    fn drop(&mut self) {
        use winapi::um::handleapi::CloseHandle;
        use winapi::um::processenv::SetStdHandle;
        use winapi::um::winbase::STD_ERROR_HANDLE;

        // SAFETY: 恢復原始 stderr 後關閉 NUL handle。
        unsafe {
            SetStdHandle(STD_ERROR_HANDLE, self.old_handle);
            CloseHandle(self.null_handle);
        }
    }
}

#[cfg(not(windows))]
struct IcuStderrGuard;
#[cfg(not(windows))]
impl IcuStderrGuard {
    fn new() -> Option<Self> {
        None
    }
}

// ─── PTT WH_KEYBOARD_LL 靜態狀態（hook callback 無法捕獲環境）──────────────

static PTT_VKEY: AtomicU32 = AtomicU32::new(0);
static PTT_MODS: AtomicU32 = AtomicU32::new(0); // bit0=Alt bit1=Ctrl bit2=Shift
static PTT_HOOK_ENABLED: AtomicBool = AtomicBool::new(true);
// 防止 key repeat 重複觸發；KEYUP 時不需重新檢查 modifier（已可能釋放）
static PTT_ARMED: AtomicBool = AtomicBool::new(false);
// 在 hook proc 內部追蹤 modifier 狀態，避免 GetAsyncKeyState 非同步延遲問題
static PTT_CTRL_HELD: AtomicBool = AtomicBool::new(false);
static PTT_ALT_HELD: AtomicBool = AtomicBool::new(false);
static PTT_SHIFT_HELD: AtomicBool = AtomicBool::new(false);
// SyncSender 從 hook callback 送出 is_down 事件
static PTT_HOOK_TX: std::sync::OnceLock<Mutex<Option<std::sync::mpsc::SyncSender<bool>>>> =
    std::sync::OnceLock::new();
// Hook 執行緒 ID，用於 PostThreadMessageA 喚醒 GetMessageA
static PTT_HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
// 當 PTT 觸發後，hook 執行緒應注入哪些 modifier key-up（bit0=Alt bit1=Ctrl bit2=Shift）
static PTT_PENDING_RELEASE: AtomicU32 = AtomicU32::new(0);

// SendInput 注入事件的標記值，避免 hook proc 把自己注入的事件重新處理
#[cfg(windows)]
const PTT_INJECTED_EXTRA_INFO: usize = 0xCAFE_0001;

#[cfg(windows)]
fn parse_windows_vkey(raw_key: &str, fallback: u32) -> u32 {
    use winapi::um::winuser::*;

    let key = raw_key.trim().to_ascii_uppercase().replace(' ', "");
    if key.is_empty() {
        return fallback;
    }

    if key.len() == 1 {
        let ch = key.as_bytes()[0] as char;
        if ch.is_ascii_alphanumeric() {
            return ch as u32;
        }
        return match ch {
            ' ' => VK_SPACE as u32,
            '-' => VK_OEM_MINUS as u32,
            '=' | '+' => VK_OEM_PLUS as u32,
            ',' => VK_OEM_COMMA as u32,
            '.' => VK_OEM_PERIOD as u32,
            '/' => VK_OEM_2 as u32,
            ';' => VK_OEM_1 as u32,
            '\'' => VK_OEM_7 as u32,
            '[' => VK_OEM_4 as u32,
            ']' => VK_OEM_6 as u32,
            '\\' => VK_OEM_5 as u32,
            '`' => VK_OEM_3 as u32,
            _ => fallback,
        };
    }

    if let Some(num) = key.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&num) {
            return VK_F1 as u32 + num - 1;
        }
    }

    match key.as_str() {
        "SPACE" | "SPACEBAR" => VK_SPACE as u32,
        "ENTER" | "RETURN" => VK_RETURN as u32,
        "ESC" | "ESCAPE" => VK_ESCAPE as u32,
        "TAB" => VK_TAB as u32,
        "BACKSPACE" | "BKSP" => VK_BACK as u32,
        "DELETE" | "DEL" => VK_DELETE as u32,
        "INSERT" | "INS" => VK_INSERT as u32,
        "HOME" => VK_HOME as u32,
        "END" => VK_END as u32,
        "PAGEUP" | "PGUP" => VK_PRIOR as u32,
        "PAGEDOWN" | "PGDN" => VK_NEXT as u32,
        "UP" | "ARROWUP" => VK_UP as u32,
        "DOWN" | "ARROWDOWN" => VK_DOWN as u32,
        "LEFT" | "ARROWLEFT" => VK_LEFT as u32,
        "RIGHT" | "ARROWRIGHT" => VK_RIGHT as u32,
        _ => fallback,
    }
}

#[cfg(windows)]
fn parse_global_hotkey(s: &str) -> (u32, u32) {
    use winapi::um::winuser::*;

    let parts: Vec<&str> = s
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let key_part = parts.last().copied().unwrap_or("W");
    let mut mods: u32 = 0;
    for part in &parts[..parts.len().saturating_sub(1)] {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL as u32,
            "shift" => mods |= MOD_SHIFT as u32,
            "alt" => mods |= MOD_ALT as u32,
            _ => {}
        }
    }
    (mods, parse_windows_vkey(key_part, 'W' as u32))
}

#[cfg(windows)]
fn parse_ptt_hotkey(s: &str) -> (u32, u32) {
    let parts: Vec<&str> = s
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let key_part = parts.last().copied().unwrap_or("L");
    let mut mods: u32 = 0;
    for part in &parts[..parts.len().saturating_sub(1)] {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= 2,
            "alt" => mods |= 1,
            "shift" => mods |= 4,
            _ => {}
        }
    }
    (mods, parse_windows_vkey(key_part, 'L' as u32))
}

#[cfg(windows)]
unsafe extern "system" fn ptt_hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    use winapi::um::winuser::*;
    if code >= 0 {
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);

        // 跳過我們自己透過 SendInput 注入的合成事件，避免無限迴圈
        if kb.dwExtraInfo == PTT_INJECTED_EXTRA_INFO {
            return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
        }

        let is_down = wparam == WM_KEYDOWN as usize || wparam == WM_SYSKEYDOWN as usize;

        // 在 hook proc 內部即時追蹤 modifier 鍵狀態（比 GetAsyncKeyState 更可靠）
        // WH_KEYBOARD_LL 的事件以原始順序到達，modifier 必定先於主鍵被更新
        // winapi VK_* 常數為 c_int (i32)，需轉型為 u32 才能與 DWORD (u32) 比較
        let vk = kb.vkCode;
        if vk == VK_LCONTROL as u32 || vk == VK_RCONTROL as u32 || vk == VK_CONTROL as u32 {
            PTT_CTRL_HELD.store(is_down, Ordering::Relaxed);
        } else if vk == VK_LMENU as u32 || vk == VK_RMENU as u32 || vk == VK_MENU as u32 {
            PTT_ALT_HELD.store(is_down, Ordering::Relaxed);
        } else if vk == VK_LSHIFT as u32 || vk == VK_RSHIFT as u32 || vk == VK_SHIFT as u32 {
            PTT_SHIFT_HELD.store(is_down, Ordering::Relaxed);
        }

        if PTT_HOOK_ENABLED.load(Ordering::Relaxed) {
            let target_vkey = PTT_VKEY.load(Ordering::Relaxed);
            if target_vkey != 0 && vk == target_vkey {
                if is_down {
                    // KEYDOWN：用追蹤的 modifier 狀態檢查組合鍵，且只在未啟動時觸發
                    let mods = PTT_MODS.load(Ordering::Relaxed);
                    let ctrl = PTT_CTRL_HELD.load(Ordering::Relaxed);
                    let alt = PTT_ALT_HELD.load(Ordering::Relaxed);
                    let shift = PTT_SHIFT_HELD.load(Ordering::Relaxed);
                    let ok = ((mods & 2 != 0) == ctrl)
                        && ((mods & 1 != 0) == alt)
                        && ((mods & 4 != 0) == shift);
                    if ok {
                        let armed = PTT_ARMED.load(Ordering::Relaxed);
                        if !armed {
                            tracing::info!(
                                "PTT KEYDOWN vk={:#04x} mods_req={:#03b} ctrl={} alt={} shift={} ok=true",
                                vk,
                                mods,
                                ctrl,
                                alt,
                                shift
                            );
                            PTT_ARMED.store(true, Ordering::Relaxed);
                            // 通知 hook 執行緒事後注入 modifier key-up
                            let rel = (alt as u32) | (ctrl as u32 * 2) | (shift as u32 * 4);
                            PTT_PENDING_RELEASE.store(rel, Ordering::Relaxed);
                            if let Some(m) = PTT_HOOK_TX.get() {
                                if let Ok(g) = m.try_lock() {
                                    if let Some(tx) = g.as_ref() {
                                        let _ = tx.try_send(true);
                                    }
                                }
                            }
                        } else {
                            tracing::debug!("PTT repeat vk={:#04x}", vk);
                        }
                        // 吞掉此按鍵（不傳給焦點視窗），無論是初次還是 repeat
                        return 1;
                    }
                } else {
                    // KEYUP：已啟動就送出停止事件（不重新檢查 modifier，因已可能被釋放）
                    tracing::info!(
                        "PTT KEYUP vk={:#04x} armed={}",
                        vk,
                        PTT_ARMED.load(Ordering::Relaxed)
                    );
                    if PTT_ARMED.swap(false, Ordering::Relaxed) {
                        if let Some(m) = PTT_HOOK_TX.get() {
                            if let Ok(g) = m.try_lock() {
                                if let Some(tx) = g.as_ref() {
                                    let _ = tx.try_send(false);
                                }
                            }
                        }
                        // 吞掉 PTT 主鍵的 key-up
                        return 1;
                    }
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

use crate::app::AppState;
use crate::audio::{AudioCapture, Resampler, VadEvent, VoiceActivityDetector};
use crate::file_transcription::{decode_audio_file, format_transcript, ExportFormat};
use crate::models::registry::ModelInfo;
use crate::models::{ModelDownloader, ModelManager};
use crate::settings::config::AppConfig;
use crate::settings::ui_settings::UiSettings;
use crate::transcription::opencc::{ConversionMode, OpenCCProcessor};
use crate::whisper::{WhisperEngine, WhisperParams};

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::context_templates::templates::PostProcessSettings;
use crate::context_templates::{ContextTemplate, TemplateId, TemplateRegistry};
use crate::{AppWindow, ModelEntry, TemplateEditEntry, TranscriptSegmentData};

// ─── 情境模板管理 ─────────────────────────────────────────────────────────────

const BUILTIN_TEMPLATE_NAMES: &[&str] =
    &["會議記錄", "口語對話", "技術討論", "醫療紀錄", "法律文書"];

fn templates_json_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("templates.json")
}

async fn load_templates_from_disk() -> Vec<ContextTemplate> {
    let path = templates_json_path();
    if path.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            if let Ok(list) = serde_json::from_str::<Vec<ContextTemplate>>(&content) {
                return list;
            }
        }
    }
    let registry = TemplateRegistry::new();
    registry.all().into_iter().cloned().collect()
}

async fn save_templates_to_disk(templates: &[ContextTemplate]) {
    let path = templates_json_path();
    if let Ok(json) = serde_json::to_string_pretty(templates) {
        let _ = tokio::fs::write(&path, json).await;
    }
}

fn templates_to_ui_entries(templates: &[ContextTemplate]) -> Vec<TemplateEditEntry> {
    templates
        .iter()
        .map(|t| {
            let name = t.id.display_name().to_string();
            let is_builtin = BUILTIN_TEMPLATE_NAMES.contains(&name.as_str());
            TemplateEditEntry {
                name: name.into(),
                prompt: t.initial_prompt.clone().into(),
                is_builtin,
            }
        })
        .collect()
}

fn make_custom_template(name: String, prompt: String) -> ContextTemplate {
    ContextTemplate {
        id: TemplateId::Custom(name),
        initial_prompt: prompt,
        language_override: Some("zh".into()),
        temperature_override: None,
        postprocess: PostProcessSettings::default(),
        keywords: vec![],
    }
}

// ─── 錄音控制句柄 ─────────────────────────────────────────────────────────────

struct RecordingHandle {
    _stream: cpal::Stream, // 保持 CPAL 串流存活
    stop_flag: Arc<AtomicBool>,
}

// cpal::Stream 在所有主流平台實作 Send
unsafe impl Send for RecordingHandle {}

struct LoadedWhisperModel {
    model_id: String,
    model_path: PathBuf,
    engine: WhisperEngine,
}

#[derive(Clone)]
struct ModelRuntime {
    cache: Arc<Mutex<Option<LoadedWhisperModel>>>,
    generation: Arc<AtomicU32>,
    download_in_progress: Arc<AtomicBool>,
}

impl ModelRuntime {
    fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(None)),
            generation: Arc::new(AtomicU32::new(0)),
            download_in_progress: Arc::new(AtomicBool::new(false)),
        }
    }
}

// ─── Entry ───────────────────────────────────────────────────────────────────

pub async fn launch(state: Arc<RwLock<AppState>>) -> anyhow::Result<()> {
    // 抑制 Slint/parley→ICU4X 初始化的 stderr 雜訊
    let _icu_guard = IcuStderrGuard::new();
    let ui = AppWindow::new()?;

    // Restore UI settings from settings.json (if it exists) before any callbacks are wired.
    if let Some(saved) = UiSettings::load() {
        ui.set_vad_threshold(saved.vad_threshold);
        ui.set_silence_timeout(saved.silence_timeout_ms);
        ui.set_n_threads(saved.n_threads);
        ui.set_use_gpu(saved.use_gpu);
        ui.set_temperature(saved.temperature);
        ui.set_conversion_mode(saved.conversion_mode.into());
        ui.set_append_newline(saved.append_newline);
        ui.set_hotkey(saved.hotkey.into());
        ui.set_ptt_hotkey(saved.ptt_hotkey.into());
        ui.set_ptt_mode(saved.ptt_mode);
        ui.set_mod_vad(saved.mod_vad);
        ui.set_mod_context(saved.mod_context);
        ui.set_mod_opencc(saved.mod_opencc);
        ui.set_mod_inject(saved.mod_inject);
        ui.set_mod_waveform(saved.mod_waveform);
        ui.set_mod_history(saved.mod_history);
        ui.set_clipboard_enabled(saved.clipboard_enabled);
        ui.set_inject_enabled(saved.inject_enabled);
        if !saved.selected_model_id.is_empty() {
            ui.set_selected_model_id(saved.selected_model_id.into());
        }
    }

    let config = {
        let s = state.read().await;
        let c = s.config.read().await.clone();
        c
    };

    let selected_model_id: Arc<Mutex<String>> =
        Arc::new(Mutex::new(ui.get_selected_model_id().to_string()));
    let recording_handle: Arc<Mutex<Option<RecordingHandle>>> = Arc::new(Mutex::new(None));
    let transcript_list: Arc<Mutex<Vec<TranscriptSegmentData>>> = Arc::new(Mutex::new(Vec::new()));
    let model_runtime = ModelRuntime::new();

    setup_model_entries(&ui, &config).await?;
    setup_model_callbacks(
        &ui,
        &config,
        Arc::clone(&selected_model_id),
        model_runtime.clone(),
    )?;
    if !preload_selected_model(
        &ui,
        &config,
        &model_runtime,
        ui.get_selected_model_id().to_string(),
    ) {
        spawn_model_select_or_download(
            TokioHandle::current(),
            ui.as_weak(),
            AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from(".")),
            ui.get_selected_model_id().to_string(),
            Arc::clone(&selected_model_id),
            model_runtime.clone(),
            config.clone(),
            whisper_params_from_ui(&ui, &config),
            true,
        );
    }
    setup_template_callbacks(&ui).await;
    setup_recording_callbacks(
        &ui,
        config.clone(),
        Arc::clone(&selected_model_id),
        Arc::clone(&recording_handle),
        Arc::clone(&transcript_list),
        model_runtime.clone(),
    );
    setup_file_mode_callbacks(
        &ui,
        &config,
        Arc::clone(&selected_model_id),
        model_runtime.clone(),
    )?;
    let hotkey_tx = setup_global_hotkey(&ui, ui.get_hotkey().to_string());
    PTT_HOOK_ENABLED.store(ui.get_ptt_mode(), Ordering::Relaxed);
    let ptt_hotkey_tx = setup_ptt_hotkey(&ui, ui.get_ptt_hotkey().to_string());
    // Set initial (idle) window icon after the window is ready.
    crate::ui::window_icon::update(false);

    // Wire settings-changed → update toggle hotkey + PTT hotkey + PTT enabled flag
    {
        let ui_ref = ui.as_weak();
        let tx = hotkey_tx.clone();
        let ptt_tx = ptt_hotkey_tx.clone();
        ui.on_settings_changed(move || {
            if let Some(ui) = ui_ref.upgrade() {
                let _ = tx.send(ui.get_hotkey().to_string());
                let _ = ptt_tx.send(ui.get_ptt_hotkey().to_string());
                PTT_HOOK_ENABLED.store(ui.get_ptt_mode(), Ordering::Relaxed);
            }
        });
    }

    // Wire save-settings → serialize UI state → write settings.json
    {
        let ui_ref = ui.as_weak();
        ui.on_save_settings(move || {
            let Some(ui) = ui_ref.upgrade() else { return };

            let settings = UiSettings {
                vad_threshold: ui.get_vad_threshold(),
                silence_timeout_ms: ui.get_silence_timeout(),
                n_threads: ui.get_n_threads(),
                use_gpu: ui.get_use_gpu(),
                temperature: ui.get_temperature(),
                conversion_mode: ui.get_conversion_mode().to_string(),
                append_newline: ui.get_append_newline(),
                hotkey: ui.get_hotkey().to_string(),
                ptt_hotkey: ui.get_ptt_hotkey().to_string(),
                ptt_mode: ui.get_ptt_mode(),
                mod_vad: ui.get_mod_vad(),
                mod_context: ui.get_mod_context(),
                mod_opencc: ui.get_mod_opencc(),
                mod_inject: ui.get_mod_inject(),
                mod_waveform: ui.get_mod_waveform(),
                mod_history: ui.get_mod_history(),
                clipboard_enabled: ui.get_clipboard_enabled(),
                inject_enabled: ui.get_inject_enabled(),
                selected_model_id: ui.get_selected_model_id().to_string(),
            };

            match settings.save() {
                Ok(()) => {
                    ui.set_save_status("✓ 已儲存".into());
                    // Clear feedback after 2 s
                    let u = ui_ref.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = u.upgrade() {
                                if ui.get_save_status() == "✓ 已儲存" {
                                    ui.set_save_status("".into());
                                }
                            }
                        });
                    });
                }
                Err(e) => {
                    error!("設定儲存失敗: {e}");
                    ui.set_save_status("✗ 儲存失敗".into());
                }
            }
        });
    }

    // ── v2.2 新增：浮動視窗 ↔ 完整視窗切換 ─────────────────────
    setup_floating_mode_callbacks(&ui);

    ui.run()?;
    Ok(())
}

// ─── v2.2 浮動模式切換 ────────────────────────────────────────────────────────

/// 視窗尺寸（單位 px，logical）
const FULL_W: f32 = 560.0;
const FULL_H: f32 = 680.0;
const FLOAT_W: f32 = 380.0;
const FLOAT_BAR_H: f32 = 52.0;
const FLOAT_EXP_H: f32 = 540.0;

fn apply_window_geometry(ui: &AppWindow) {
    let floating = ui.get_floating_mode();
    let expanded = ui.get_floating_expanded();
    let (w, h) = if !floating {
        (FULL_W, FULL_H)
    } else if expanded {
        (FLOAT_W, FLOAT_EXP_H)
    } else {
        (FLOAT_W, FLOAT_BAR_H)
    };
    ui.window().set_size(slint::LogicalSize::new(w, h));

    #[cfg(windows)]
    set_topmost_windows(ui, floating);
}

#[cfg(windows)]
fn set_topmost_windows(ui: &AppWindow, topmost: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winapi::shared::windef::HWND;
    use winapi::um::winuser::{SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE};

    let slint_wh = ui.window().window_handle();
    let Ok(raw) = slint_wh.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = raw.as_raw() else {
        return;
    };
    let hwnd = win32.hwnd.get() as HWND;
    let target = if topmost {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    // SAFETY: hwnd 取自 Slint 提供之 RawWindowHandle，仍存活；SetWindowPos 為標準 Win32 API。
    unsafe {
        SetWindowPos(hwnd, target, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
    }
}

#[cfg(not(windows))]
fn set_topmost_windows(_ui: &AppWindow, _topmost: bool) {}

fn setup_floating_mode_callbacks(ui: &AppWindow) {
    // 啟動時不主動 set_size：交給 Slint preferred-* 決定，避免在 run() 前
    // 對尚未建立的 OS window 操作造成的副作用。

    {
        let ui_ref = ui.as_weak();
        ui.on_toggle_floating_mode(move || {
            if let Some(ui) = ui_ref.upgrade() {
                apply_window_geometry(&ui);
                info!(floating = ui.get_floating_mode(), "視窗模式切換");
            }
        });
    }

    {
        let ui_ref = ui.as_weak();
        ui.on_toggle_floating_expanded(move || {
            if let Some(ui) = ui_ref.upgrade() {
                apply_window_geometry(&ui);
            }
        });
    }

    {
        let ui_ref = ui.as_weak();
        ui.on_request_close(move || {
            if let Some(ui) = ui_ref.upgrade() {
                let _ = ui.window().hide();
            }
        });
    }

    // start-drag：Win32 透過 ReleaseCapture + WM_NCLBUTTONDOWN(HTCAPTION) 觸發系統移動
    #[cfg(windows)]
    {
        let ui_ref = ui.as_weak();
        ui.on_start_drag(move || {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            use winapi::shared::windef::HWND;
            use winapi::um::winuser::{ReleaseCapture, SendMessageW, HTCAPTION, WM_NCLBUTTONDOWN};

            let Some(ui) = ui_ref.upgrade() else { return };
            let slint_wh = ui.window().window_handle();
            let Ok(raw) = slint_wh.window_handle() else {
                return;
            };
            let RawWindowHandle::Win32(win32) = raw.as_raw() else {
                return;
            };
            let hwnd = win32.hwnd.get() as HWND;
            // SAFETY: hwnd 來自 Slint live window；兩個 API 皆為標準 Win32 訊息呼叫。
            unsafe {
                ReleaseCapture();
                SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
            }
        });
    }

    {
        let ui_ref = ui.as_weak();
        ui.on_clear_error_log(move || {
            if let Some(ui) = ui_ref.upgrade() {
                let empty: Vec<slint::SharedString> = Vec::new();
                ui.set_error_log(slint::ModelRc::new(slint::VecModel::from(empty)));
                ui.set_show_error_log(false);
            }
        });
    }
}

// ─── 模型列表 ─────────────────────────────────────────────────────────────────

fn model_entries_from(models: &[ModelInfo], _selected_id: &str) -> Vec<ModelEntry> {
    models
        .iter()
        .map(|m| ModelEntry {
            id: m.id.as_str().into(),
            display_name: m.display_name.as_str().into(),
            is_downloaded: m.is_downloaded,
            size_mb: m.size.disk_mb as i32,
            quality: m.quality_stars as i32,
            download_url: m.download_url.as_str().into(),
        })
        .collect()
}

async fn setup_model_entries(ui: &AppWindow, _config: &AppConfig) -> anyhow::Result<()> {
    let models_dir = AppConfig::models_dir()?;
    let manager = ModelManager::new(models_dir);
    let models = manager.list_available().await;

    let selected = ui.get_selected_model_id().to_string();
    let entries = model_entries_from(&models, &selected);
    ui.set_model_entries(ModelRc::new(VecModel::from(entries)));

    if let Some(info) = models.iter().find(|m| m.id == selected) {
        ui.set_model_name(info.display_name.as_str().into());
    }
    Ok(())
}

fn whisper_params_from_ui(ui: &AppWindow, config: &AppConfig) -> WhisperParams {
    let mut params = config.whisper.clone();
    params.use_gpu = ui.get_use_gpu();
    params.n_threads = ui.get_n_threads();
    params.temperature = ui.get_temperature();
    params
}

fn preload_selected_model(
    ui: &AppWindow,
    config: &AppConfig,
    runtime: &ModelRuntime,
    model_id: String,
) -> bool {
    let models_dir = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    let model_path = models_dir.join(format!("ggml-{model_id}.bin"));
    if !model_path.exists() {
        info!("略過模型預載，檔案不存在: {}", model_path.display());
        return false;
    }
    spawn_model_preload(
        runtime.clone(),
        model_id,
        model_path,
        whisper_params_from_ui(ui, config),
        ui.as_weak(),
    );
    true
}

fn spawn_model_preload(
    runtime: ModelRuntime,
    model_id: String,
    model_path: PathBuf,
    whisper_params: WhisperParams,
    ui_weak: slint::Weak<AppWindow>,
) {
    let generation = runtime.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let label = model_id.clone();

    {
        let u = ui_weak.clone();
        let status = format!("預載模型 {label}...");
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                if !ui.get_is_recording() {
                    ui.set_status_text(status.into());
                }
            }
        });
    }

    std::thread::spawn(move || {
        info!("背景預載模型: {} ({})", model_id, model_path.display());
        match WhisperEngine::load(&model_path, &whisper_params) {
            Ok(engine) => {
                if runtime.generation.load(Ordering::SeqCst) != generation {
                    info!("丟棄過期模型預載結果: {}", model_id);
                    return;
                }

                let device = engine.device_label.clone();
                {
                    let mut cache = runtime.cache.lock().unwrap();
                    *cache = Some(LoadedWhisperModel {
                        model_id: model_id.clone(),
                        model_path: model_path.clone(),
                        engine,
                    });
                }

                let u = ui_weak.clone();
                let status = format!("模型已預載: {model_id}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_device_label(device.into());
                        if !ui.get_is_recording() {
                            ui.set_status_text(status.into());
                        }
                    }
                });
            }
            Err(e) => {
                error!("模型預載失敗: {e}");
                let u = ui_weak.clone();
                let msg = format!("模型預載失敗: {e}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        if !ui.get_is_recording() {
                            ui.set_status_text(msg.into());
                        }
                    }
                });
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn spawn_model_select_or_download(
    tokio_handle: TokioHandle,
    ui_weak: slint::Weak<AppWindow>,
    models_dir: PathBuf,
    model_id: String,
    selected_model_id: Arc<Mutex<String>>,
    runtime: ModelRuntime,
    _config: AppConfig,
    whisper_params: WhisperParams,
    confirm_if_missing: bool,
) {
    tokio_handle.spawn(async move {
        let manager = ModelManager::new(models_dir.clone());
        let models = manager.list_available().await;

        let Some(info) = models.iter().find(|m| m.id == model_id).cloned() else {
            let msg = format!("未知模型: {model_id}");
            let u = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_status_text(msg.into());
                }
            });
            return;
        };

        if info.is_downloaded {
            select_downloaded_model(
                ui_weak,
                models_dir,
                model_id,
                selected_model_id,
                runtime,
                whisper_params,
                models,
                "模型已切換",
            );
            return;
        }

        if confirm_if_missing {
            let display_name = info.display_name.clone();
            let size_mb = info.size.disk_mb;
            let result = rfd::AsyncMessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("模型尚未下載")
                .set_description(format!(
                    "語音辨識模型 {display_name} 尚未下載。\n\n大小約 {size_mb} MB，是否現在自動下載？\n下載完成後會自動選用並預載。"
                ))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                .await;

            if result != rfd::MessageDialogResult::Yes {
                let msg = format!("已取消下載模型 {display_name}");
                let u = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_status_text(msg.into());
                    }
                });
                return;
            }
        }

        download_model_and_select(
            ui_weak,
            models_dir,
            info,
            selected_model_id,
            runtime,
            whisper_params,
        )
        .await;
    });
}

#[allow(clippy::too_many_arguments)]
fn select_downloaded_model(
    ui_weak: slint::Weak<AppWindow>,
    models_dir: PathBuf,
    model_id: String,
    selected_model_id: Arc<Mutex<String>>,
    runtime: ModelRuntime,
    whisper_params: WhisperParams,
    models: Vec<ModelInfo>,
    status: &'static str,
) {
    *selected_model_id.lock().unwrap() = model_id.clone();

    let display = models
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| m.display_name.clone())
        .unwrap_or_else(|| model_id.clone());
    let entries = model_entries_from(&models, &model_id);
    let model_path = models_dir.join(format!("ggml-{model_id}.bin"));
    let ui_for_preload = ui_weak.clone();
    let id_for_ui = model_id.clone();

    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_downloading_id("".into());
            ui.set_download_progress(0.0);
            ui.set_selected_model_id(id_for_ui.into());
            ui.set_model_name(display.into());
            ui.set_status_text(status.into());
            ui.set_model_entries(ModelRc::new(VecModel::from(entries)));
        }
    });

    spawn_model_preload(
        runtime,
        model_id,
        model_path,
        whisper_params,
        ui_for_preload,
    );
}

#[allow(clippy::too_many_arguments)]
async fn download_model_and_select(
    ui_weak: slint::Weak<AppWindow>,
    models_dir: PathBuf,
    info: ModelInfo,
    selected_model_id: Arc<Mutex<String>>,
    runtime: ModelRuntime,
    whisper_params: WhisperParams,
) {
    let model_id = info.id.clone();
    let size_mb = info.size.disk_mb;

    if runtime
        .download_in_progress
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        let u = ui_weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                ui.set_status_text("已有模型正在下載，請稍候...".into());
            }
        });
        return;
    }

    {
        let u = ui_weak.clone();
        let id_show = model_id.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                ui.set_downloading_id(id_show.into());
                ui.set_download_progress(0.0);
                ui.set_status_text(format!("下載模型 ({size_mb} MB)...").into());
            }
        });
    }

    let downloader = ModelDownloader::new(models_dir.clone());
    let (tx, mut rx) =
        tokio::sync::mpsc::channel::<crate::models::downloader::DownloadProgress>(64);

    let ui_prog = ui_weak.clone();
    tokio::spawn(async move {
        while let Some(prog) = rx.recv().await {
            let ratio = prog
                .total_bytes
                .map(|t| prog.downloaded_bytes as f32 / t as f32)
                .unwrap_or(0.5_f32);
            let u = ui_prog.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_download_progress(ratio);
                }
            });
        }
    });

    match downloader.download(&info, tx).await {
        Ok(_) => {
            info!("模型 {model_id} 下載完成");
            let manager = ModelManager::new(models_dir.clone());
            let updated = manager.list_available().await;
            select_downloaded_model(
                ui_weak,
                models_dir,
                model_id,
                selected_model_id,
                runtime.clone(),
                whisper_params,
                updated,
                "模型下載完成，已選用",
            );
        }
        Err(e) => {
            error!("模型下載失敗: {e}");
            let msg = format!("下載失敗: {e}");
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_downloading_id("".into());
                    ui.set_download_progress(0.0);
                    ui.set_status_text(msg.into());
                }
            });
        }
    }
    runtime.download_in_progress.store(false, Ordering::SeqCst);
}

// ─── 模型選擇 / 下載 ──────────────────────────────────────────────────────────

fn setup_model_callbacks(
    ui: &AppWindow,
    config: &AppConfig,
    selected_model_id: Arc<Mutex<String>>,
    model_runtime: ModelRuntime,
) -> anyhow::Result<()> {
    let models_dir = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    let ui_weak = ui.as_weak();
    let model_id_arc = Arc::clone(&selected_model_id);
    let config = config.clone();
    let tokio_handle = TokioHandle::current();

    ui.on_model_select_or_download(move |model_id_shared| {
        let params = ui_weak
            .upgrade()
            .map(|ui| whisper_params_from_ui(&ui, &config))
            .unwrap_or_else(|| config.whisper.clone());
        spawn_model_select_or_download(
            tokio_handle.clone(),
            ui_weak.clone(),
            models_dir.clone(),
            model_id_shared.to_string(),
            Arc::clone(&model_id_arc),
            model_runtime.clone(),
            config.clone(),
            params,
            false,
        );
    });

    // 開啟模型資料夾
    let mdir2 = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    ui.on_open_model_manager(move || {
        let dir = mdir2.clone();
        std::thread::spawn(move || {
            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
        });
    });

    Ok(())
}

// ─── 情境模板 Callbacks ───────────────────────────────────────────────────────

async fn setup_template_callbacks(ui: &AppWindow) {
    // Load templates (from disk or built-ins) and populate UI list
    let templates = load_templates_from_disk().await;
    let entries = templates_to_ui_entries(&templates);
    ui.set_template_entries(ModelRc::new(VecModel::from(entries)));

    // template-save(original_name, new_name, prompt)
    {
        let ui_weak = ui.as_weak();
        ui.on_template_save(move |orig_shared, name_shared, prompt_shared| {
            let orig = orig_shared.to_string();
            let name = name_shared.to_string();
            let prompt = prompt_shared.to_string();
            let uw = ui_weak.clone();
            tokio::spawn(async move {
                let mut templates = load_templates_from_disk().await;
                if let Some(t) = templates.iter_mut().find(|t| t.id.display_name() == orig) {
                    if orig != name && !BUILTIN_TEMPLATE_NAMES.contains(&orig.as_str()) {
                        t.id = TemplateId::Custom(name.clone());
                    }
                    t.initial_prompt = prompt;
                } else {
                    templates.push(make_custom_template(name, prompt));
                }
                save_templates_to_disk(&templates).await;
                let entries = templates_to_ui_entries(&templates);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_template_entries(ModelRc::new(VecModel::from(entries)));
                        ui.set_status_text("模板已儲存".into());
                    }
                });
            });
        });
    }

    // template-delete(name)
    {
        let ui_weak = ui.as_weak();
        ui.on_template_delete(move |name_shared| {
            let name = name_shared.to_string();
            let uw = ui_weak.clone();
            tokio::spawn(async move {
                let mut templates = load_templates_from_disk().await;
                templates.retain(|t| t.id.display_name() != name);
                save_templates_to_disk(&templates).await;
                let entries = templates_to_ui_entries(&templates);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_template_entries(ModelRc::new(VecModel::from(entries)));
                    }
                });
            });
        });
    }
}

// ─── 錄音控制 ─────────────────────────────────────────────────────────────────

fn setup_recording_callbacks(
    ui: &AppWindow,
    config: AppConfig,
    selected_model_id: Arc<Mutex<String>>,
    recording_handle: Arc<Mutex<Option<RecordingHandle>>>,
    transcript_list: Arc<Mutex<Vec<TranscriptSegmentData>>>,
    model_runtime: ModelRuntime,
) {
    let ui_weak = ui.as_weak();
    let tokio_handle = TokioHandle::current();

    // ── toggle-recording ─────────────────────────────────────────────────────
    ui.on_toggle_recording({
        let handle_arc = Arc::clone(&recording_handle);
        let model_id_arc = Arc::clone(&selected_model_id);
        let transcript_arc = Arc::clone(&transcript_list);
        let ui_ref = ui_weak.clone();
        let cfg = config.clone();
        let runtime = model_runtime.clone();
        let tokio_handle = tokio_handle.clone();

        move || {
            let mut handle = handle_arc.lock().unwrap();

            if handle.is_some() {
                // ══ 停止錄音 ══════════════════════════════════════
                let h = handle.take().unwrap();
                h.stop_flag.store(true, Ordering::SeqCst);
                // drop h → _stream dropped → CPAL 停止音訊

                crate::ui::window_icon::update(false);
                let u = ui_ref.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_is_recording(false);
                        ui.set_status_text("就緒".into());
                        ui.set_audio_level(0.0);
                        ui.set_is_processing(false);
                        ui.set_current_partial("".into());
                        ui.set_waveform_bars(ModelRc::new(VecModel::from(vec![0.0f32; 20])));
                    }
                });
            } else {
                // ══ 開始錄音 ══════════════════════════════════════
                let model_id = model_id_arc.lock().unwrap().clone();
                let models_dir = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
                let model_path = models_dir.join(format!("ggml-{}.bin", model_id));

                info!("嘗試啟動錄音，模型: {}", model_id);
                if !model_path.exists() {
                    let msg = format!("模型 {model_id} 尚未下載，等待使用者確認下載");
                    error!("{msg}");
                    let u = ui_ref.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = u.upgrade() {
                            ui.set_status_text("模型尚未下載，請確認是否自動下載".into());
                        }
                    });
                    spawn_model_select_or_download(
                        tokio_handle.clone(),
                        ui_ref.clone(),
                        models_dir,
                        model_id,
                        Arc::clone(&model_id_arc),
                        runtime.clone(),
                        cfg.clone(),
                        ui_ref
                            .upgrade()
                            .map(|ui| whisper_params_from_ui(&ui, &cfg))
                            .unwrap_or_else(|| cfg.whisper.clone()),
                        true,
                    );
                    return;
                }

                // 建立音訊捕捉
                let capture = match AudioCapture::new(cfg.audio.device_name.as_deref()) {
                    Ok(c) => c,
                    Err(e) => {
                        let u = ui_ref.clone();
                        let msg = format!("音訊裝置錯誤: {e}");
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = u.upgrade() {
                                ui.set_status_text(msg.into());
                            }
                        });
                        return;
                    }
                };
                let sample_rate = capture.sample_rate();

                // tokio 通道（供 cpal 回呼使用 try_send）
                let (audio_tok_tx, audio_tok_rx) = tokio::sync::mpsc::channel::<Vec<f32>>(128);
                let (level_tok_tx, level_tok_rx) = tokio::sync::mpsc::channel::<f32>(128);

                // std 通道（供處理執行緒使用 try_recv / blocking）
                let (audio_std_tx, audio_std_rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(64);
                let (level_std_tx, level_std_rx) = std::sync::mpsc::sync_channel::<f32>(64);

                // 橋接：tokio → std
                // buffer 滿時丟棄 chunk（模型載入期間）而非退出，否則管線會提早結束
                tokio::spawn(async move {
                    let mut rx = audio_tok_rx;
                    while let Some(data) = rx.recv().await {
                        let _ = audio_std_tx.try_send(data);
                    }
                });
                tokio::spawn(async move {
                    let mut rx = level_tok_rx;
                    while let Some(lvl) = rx.recv().await {
                        let _ = level_std_tx.try_send(lvl);
                    }
                });

                // 啟動 CPAL 串流
                let stream = match capture.start(audio_tok_tx, level_tok_tx) {
                    Ok(s) => s,
                    Err(e) => {
                        let u = ui_ref.clone();
                        let msg = format!("無法啟動錄音串流: {e}");
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = u.upgrade() {
                                ui.set_status_text(msg.into());
                            }
                        });
                        return;
                    }
                };

                // 讀取輸出設定（從 UI 讀取當前狀態）
                let (
                    clipboard_on,
                    inject_on,
                    ui_use_gpu,
                    ui_n_threads,
                    ui_temperature,
                    opencc_enabled,
                    conversion_mode,
                ) = ui_ref
                    .upgrade()
                    .map(|ui| {
                        (
                            ui.get_clipboard_enabled(),
                            ui.get_inject_enabled(),
                            ui.get_use_gpu(),
                            ui.get_n_threads(),
                            ui.get_temperature(),
                            ui.get_mod_opencc(),
                            ui.get_conversion_mode().to_string(),
                        )
                    })
                    .unwrap_or((true, true, true, 4, 0.0, true, "zh-TW".to_string()));

                let stop_flag = Arc::new(AtomicBool::new(false));
                let stop_flag_thread = Arc::clone(&stop_flag);
                let mut whisper_params = cfg.whisper.clone();
                whisper_params.use_gpu = ui_use_gpu;
                whisper_params.n_threads = ui_n_threads;
                whisper_params.temperature = ui_temperature;
                let vad_threshold = cfg.audio.vad_threshold;
                let vad_timeout = cfg.audio.vad_silence_timeout_ms;
                let ui_thread = ui_ref.clone();
                let transcript_thread = Arc::clone(&transcript_arc);
                let runtime_thread = runtime.clone();

                std::thread::spawn(move || {
                    run_recording_pipeline(
                        model_id,
                        model_path,
                        whisper_params,
                        sample_rate,
                        vad_threshold,
                        vad_timeout,
                        clipboard_on,
                        inject_on,
                        opencc_enabled,
                        conversion_mode,
                        audio_std_rx,
                        level_std_rx,
                        stop_flag_thread,
                        ui_thread,
                        transcript_thread,
                        runtime_thread,
                    );
                });

                *handle = Some(RecordingHandle {
                    _stream: stream,
                    stop_flag,
                });

                crate::ui::window_icon::update(true);
                let u = ui_ref.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_is_recording(true);
                        ui.set_status_text("● 錄音中...".into());
                    }
                });
            }
        }
    });

    // ── clear-transcript ──────────────────────────────────────────────────────
    let ui_weak2 = ui_weak.clone();
    ui.on_clear_transcript(move || {
        let mut list = transcript_list.lock().unwrap();
        list.clear();
        let u = ui_weak2.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                ui.set_transcript_segments(ModelRc::new(VecModel::from(Vec::<
                    TranscriptSegmentData,
                >::new())));
                ui.set_char_count(0);
            }
        });
    });
}

// ─── 錄音處理管線（獨立執行緒）────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn run_recording_pipeline(
    model_id: String,
    model_path: PathBuf,
    whisper_params: WhisperParams,
    sample_rate: u32,
    vad_threshold: f32,
    vad_timeout_ms: u64,
    clipboard_enabled: bool,
    inject_enabled: bool,
    opencc_enabled: bool,
    conversion_mode: String,
    audio_rx: std::sync::mpsc::Receiver<Vec<f32>>,
    level_rx: std::sync::mpsc::Receiver<f32>,
    stop_flag: Arc<AtomicBool>,
    ui_weak: slint::Weak<AppWindow>,
    transcript_list: Arc<Mutex<Vec<TranscriptSegmentData>>>,
    model_runtime: ModelRuntime,
) {
    let mut resampler = match Resampler::new(sample_rate) {
        Ok(r) => r,
        Err(e) => {
            error!("Resampler 初始化失敗: {e}");
            return;
        }
    };
    let mut vad = VoiceActivityDetector::new(vad_threshold, vad_timeout_ms);

    // 波形環形緩衝（20 條 bar）
    let mut bar_buf: Vec<f32> = vec![0.0f32; 20];
    let mut bar_idx: usize = 0;
    let mut last_wave_tick = std::time::Instant::now();
    let mut full_samples: Vec<f32> = Vec::new();

    loop {
        if stop_flag.load(Ordering::SeqCst) {
            while let Ok(chunk) = audio_rx.try_recv() {
                process_recording_chunk(
                    chunk,
                    &mut resampler,
                    &mut vad,
                    &mut full_samples,
                    &ui_weak,
                );
            }
            let _ = vad.flush();

            if full_samples.is_empty() {
                info!("停止錄音，但沒有收到音訊樣本");
            } else {
                let duration_ms = (full_samples.len() as u64 * 1000) / 16_000;
                let wav_path = match save_recording_wav(&full_samples, 16_000) {
                    Ok(path) => {
                        info!("已保留完整錄音: {} ({}ms)", path.display(), duration_ms);
                        Some(path)
                    }
                    Err(e) => {
                        error!("保存錄音失敗: {e}");
                        None
                    }
                };
                let segment = crate::audio::AudioSegment {
                    samples: std::mem::take(&mut full_samples),
                    duration_ms,
                };
                transcribe_full_recording(
                    &model_runtime,
                    model_id.clone(),
                    model_path.clone(),
                    &whisper_params,
                    segment,
                    clipboard_enabled,
                    inject_enabled,
                    opencc_enabled,
                    conversion_mode.clone(),
                    &ui_weak,
                    &transcript_list,
                    wav_path,
                );
            }
            break;
        }

        while let Ok(chunk) = audio_rx.try_recv() {
            process_recording_chunk(chunk, &mut resampler, &mut vad, &mut full_samples, &ui_weak);
        }

        // 更新音量等級 → 波形
        while let Ok(level) = level_rx.try_recv() {
            let normalized = (level * 4.0).min(1.0);
            bar_buf[bar_idx % 20] = normalized;
            bar_idx += 1;
        }

        if last_wave_tick.elapsed().as_millis() >= 80 {
            last_wave_tick = std::time::Instant::now();
            let bars = bar_buf.clone();
            let u = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_waveform_bars(ModelRc::new(VecModel::from(bars)));
                }
            });
        }

        if stop_flag.load(Ordering::SeqCst) {
            continue;
        }

        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // 清理
    info!("錄音管線結束");
    let u = ui_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = u.upgrade() {
            ui.set_waveform_bars(ModelRc::new(VecModel::from(vec![0.0f32; 20])));
            ui.set_is_processing(false);
            ui.set_audio_level(0.0);
        }
    });
}

fn process_recording_chunk(
    chunk: Vec<f32>,
    resampler: &mut Resampler,
    vad: &mut VoiceActivityDetector,
    full_samples: &mut Vec<f32>,
    ui_weak: &slint::Weak<AppWindow>,
) {
    let resampled = match resampler.process(&chunk) {
        Ok(r) => r,
        Err(e) => {
            error!("重採樣失敗: {e}");
            return;
        }
    };

    full_samples.extend_from_slice(&resampled);

    for event in vad.process(&resampled) {
        match event {
            VadEvent::SpeechStart => {
                let u = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_current_partial("聆聽中...".into());
                        ui.set_is_processing(true);
                    }
                });
            }
            VadEvent::Segment(segment) => {
                info!(
                    "VAD 偵測到語音段落，長度 {}ms；PTT 模式保留至停止後整段轉錄",
                    segment.duration_ms
                );
            }
            VadEvent::Silence => {
                let u = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_current_partial("".into());
                    }
                });
            }
            VadEvent::SpeechContinue { level } => {
                let u = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_audio_level(level);
                    }
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn transcribe_full_recording(
    runtime: &ModelRuntime,
    model_id: String,
    model_path: PathBuf,
    whisper_params: &WhisperParams,
    segment: crate::audio::AudioSegment,
    clipboard_enabled: bool,
    inject_enabled: bool,
    opencc_enabled: bool,
    conversion_mode: String,
    ui_weak: &slint::Weak<AppWindow>,
    transcript_list: &Arc<Mutex<Vec<TranscriptSegmentData>>>,
    wav_path: Option<PathBuf>,
) {
    {
        let mut cache = runtime.cache.lock().unwrap();
        if let Some(loaded) = cache
            .as_ref()
            .filter(|loaded| loaded.model_id == model_id && loaded.model_path == model_path)
        {
            info!("使用已預載模型轉錄完整錄音: {}", model_id);
            transcribe_and_update(
                &loaded.engine,
                segment,
                whisper_params,
                clipboard_enabled,
                inject_enabled,
                opencc_enabled,
                conversion_mode.clone(),
                ui_weak,
                transcript_list,
                wav_path,
            );
            return;
        }

        cache.take();
    }

    {
        let u = ui_weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                ui.set_status_text("模型尚未預載完成，正在載入...".into());
                ui.set_is_processing(true);
            }
        });
    }

    let engine = match WhisperEngine::load(&model_path, whisper_params) {
        Ok(engine) => engine,
        Err(e) => {
            let msg = format!("模型載入失敗: {e}");
            error!("{msg}");
            let u = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_status_text(msg.into());
                    ui.set_is_processing(false);
                }
            });
            return;
        }
    };

    let mut cache = runtime.cache.lock().unwrap();
    *cache = Some(LoadedWhisperModel {
        model_id,
        model_path,
        engine,
    });
    if let Some(loaded) = cache.as_ref() {
        transcribe_and_update(
            &loaded.engine,
            segment,
            whisper_params,
            clipboard_enabled,
            inject_enabled,
            opencc_enabled,
            conversion_mode,
            ui_weak,
            transcript_list,
            wav_path,
        );
    }
}

fn save_recording_wav(samples: &[f32], sample_rate: u32) -> anyhow::Result<PathBuf> {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("recordings");
    std::fs::create_dir_all(&base)?;

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = base.join(format!("ptt-{millis}.wav"));
    let mut file = std::fs::File::create(&path)?;

    let data_len = samples.len() as u32 * 2;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_len).to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * 2).to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;

    for sample in samples {
        let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        file.write_all(&pcm.to_le_bytes())?;
    }

    Ok(path)
}

// ─── 全域快捷鍵 ──────────────────────────────────────────────────────────────

fn setup_global_hotkey(ui: &AppWindow, initial_hotkey: String) -> std::sync::mpsc::Sender<String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let ui_weak = ui.as_weak();

    #[cfg(windows)]
    {
        std::thread::spawn(move || unsafe {
            use std::time::Duration;
            use winapi::um::winuser::*;
            const HOTKEY_ID: i32 = 9001;

            let (mut mods, mut vkey) = parse_global_hotkey(&initial_hotkey);
            let null_hwnd = std::ptr::null_mut::<winapi::shared::windef::HWND__>();
            RegisterHotKey(null_hwnd, HOTKEY_ID, mods, vkey);
            info!("全域快捷鍵已啟用: {}", initial_hotkey);

            loop {
                // check for hotkey update from UI
                if let Ok(new_key) = rx.try_recv() {
                    UnregisterHotKey(null_hwnd, HOTKEY_ID);
                    let (nm, nv) = parse_global_hotkey(&new_key);
                    mods = nm;
                    vkey = nv;
                    RegisterHotKey(null_hwnd, HOTKEY_ID, mods, vkey);
                    info!("快捷鍵更新: {}", new_key);
                }

                let mut msg: MSG = std::mem::zeroed();
                if PeekMessageA(&mut msg, null_hwnd, 0, 0, PM_REMOVE) != 0
                    && msg.message == WM_HOTKEY
                    && msg.wParam as i32 == HOTKEY_ID
                {
                    let u = ui_weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = u.upgrade() {
                            ui.invoke_toggle_recording();
                        }
                    });
                }

                std::thread::sleep(Duration::from_millis(10));
            }
        });
    }

    #[cfg(not(windows))]
    {
        let _ = ui_weak;
        info!("全域快捷鍵僅支援 Windows（設定: {}）", initial_hotkey);
    }

    tx
}

// ─── PTT modifier key-up 注入 ────────────────────────────────────────────────
// 在 hook 執行緒（非 hook proc 內）呼叫 SendInput，將已被吞掉的 modifier 組合鍵
// 補發 key-up，讓焦點視窗的鍵盤狀態回到乾淨狀態，避免 Ctrl/Alt 卡住。

#[cfg(windows)]
unsafe fn inject_modifier_key_ups(rel_mask: u32) {
    use winapi::ctypes::c_int;
    use winapi::um::winuser::*;

    // rel_mask: bit0=Alt bit1=Ctrl bit2=Shift（與 PTT_MODS 編碼相同）
    let mut inputs: [INPUT; 3] = std::mem::zeroed();
    let mut count: u32 = 0;

    let make_keyup = |vk: c_int| -> INPUT {
        let mut inp: INPUT = unsafe { std::mem::zeroed() };
        inp.type_ = INPUT_KEYBOARD;
        let ki = unsafe { inp.u.ki_mut() };
        ki.wVk = vk as u16;
        ki.dwFlags = KEYEVENTF_KEYUP;
        ki.dwExtraInfo = PTT_INJECTED_EXTRA_INFO;
        inp
    };

    if rel_mask & 2 != 0 {
        inputs[count as usize] = make_keyup(VK_CONTROL);
        count += 1;
    }
    if rel_mask & 1 != 0 {
        inputs[count as usize] = make_keyup(VK_MENU);
        count += 1;
    }
    if rel_mask & 4 != 0 {
        inputs[count as usize] = make_keyup(VK_SHIFT);
        count += 1;
    }

    if count > 0 {
        SendInput(
            count,
            inputs.as_mut_ptr(),
            std::mem::size_of::<INPUT>() as c_int,
        );
        info!("PTT: 注入 modifier key-up mask={:#03b}", rel_mask);
    }
}

// ─── PTT 快捷鍵（WH_KEYBOARD_LL 事件驅動）──────────────────────────────────

fn setup_ptt_hotkey(ui: &AppWindow, initial_hotkey: String) -> std::sync::mpsc::Sender<String> {
    // outer_tx is returned to callers; inner_rx is used by the hook thread.
    // The adapter thread bridges the two and posts WM_APP to wake GetMessageA.
    let (outer_tx, outer_rx) = std::sync::mpsc::channel::<String>();
    let (hotkey_tx_inner, hotkey_rx) = std::sync::mpsc::channel::<String>();

    std::thread::spawn(move || {
        for new_key in outer_rx.iter() {
            let _ = hotkey_tx_inner.send(new_key);
            #[cfg(windows)]
            {
                use winapi::um::winuser::{PostThreadMessageA, WM_APP};
                let tid = PTT_HOOK_THREAD_ID.load(Ordering::Relaxed);
                if tid != 0 {
                    unsafe {
                        PostThreadMessageA(tid, WM_APP, 0, 0);
                    }
                }
            }
        }
    });
    let hotkey_tx = outer_tx;

    // 初始化 vkey/mods 靜態值
    #[cfg(windows)]
    {
        let (mods, vkey) = parse_ptt_hotkey(&initial_hotkey);
        PTT_VKEY.store(vkey, Ordering::Relaxed);
        PTT_MODS.store(mods, Ordering::Relaxed);
        info!(
            "PTT 快捷鍵: {} (vkey={:#04x} mods={:#03b})",
            initial_hotkey, vkey, mods
        );
    }

    // 建立事件通道並存入靜態（hook callback 使用）
    let (event_tx, event_rx) = std::sync::mpsc::sync_channel::<bool>(32);
    PTT_HOOK_TX.get_or_init(|| Mutex::new(Some(event_tx)));

    // 執行緒 A：安裝 WH_KEYBOARD_LL + 訊息迴圈
    #[cfg(windows)]
    std::thread::spawn(move || unsafe {
        use winapi::um::winuser::*;

        // 強制建立此執行緒的訊息佇列（在 SetWindowsHookExA 之前必須先存在）
        let mut dummy: MSG = std::mem::zeroed();
        PeekMessageA(
            &mut dummy,
            std::ptr::null_mut(),
            WM_USER,
            WM_USER,
            PM_NOREMOVE,
        );

        // 儲存執行緒 ID，供 PostThreadMessageA 喚醒用
        PTT_HOOK_THREAD_ID.store(
            winapi::um::processthreadsapi::GetCurrentThreadId(),
            Ordering::Relaxed,
        );

        let hook = SetWindowsHookExA(WH_KEYBOARD_LL, Some(ptt_hook_proc), std::ptr::null_mut(), 0);
        if hook.is_null() {
            error!("PTT: SetWindowsHookExA 失敗");
            return;
        }
        info!(
            "PTT WH_KEYBOARD_LL hook 安裝成功（vkey={:#04x} mods={:#03b}）",
            PTT_VKEY.load(Ordering::Relaxed),
            PTT_MODS.load(Ordering::Relaxed)
        );

        let mut msg: MSG = std::mem::zeroed();
        loop {
            // GetMessageA 阻塞等待訊息（比 PeekMessage+sleep 更可靠；
            // WH_KEYBOARD_LL 透過 SendMessage 投遞，GetMessageA 會在傳回前先處理所有待辦的 sent messages）
            let ret = GetMessageA(&mut msg, std::ptr::null_mut(), 0, 0);
            if ret <= 0 {
                // 0 = WM_QUIT, -1 = 錯誤
                break;
            }
            // 每次被喚醒時檢查快捷鍵更新（包括 WM_APP 喚醒與真實按鍵事件）
            while let Ok(new_key) = hotkey_rx.try_recv() {
                let (nm, nv) = parse_ptt_hotkey(&new_key);
                PTT_MODS.store(nm, Ordering::Relaxed);
                PTT_VKEY.store(nv, Ordering::Relaxed);
                PTT_ARMED.store(false, Ordering::Relaxed);
                info!(
                    "PTT 快捷鍵更新: {} (vkey={:#04x} mods={:#03b})",
                    new_key, nv, nm
                );
            }
            // 若 hook proc 設置了待釋放的 modifier，在此注入合成 key-up 事件
            // 注意：必須在 hook proc 返回後才能呼叫 SendInput，故在訊息迴圈而非 hook proc 內執行
            let rel = PTT_PENDING_RELEASE.swap(0, Ordering::Relaxed);
            if rel != 0 {
                inject_modifier_key_ups(rel);
            }
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
    });

    // 執行緒 B：接收事件 → 呼叫 UI toggle
    let ui_weak = ui.as_weak();
    std::thread::spawn(move || {
        while let Ok(is_down) = event_rx.recv() {
            let u = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    if is_down != ui.get_is_recording() {
                        ui.invoke_toggle_recording();
                    }
                }
            });
        }
    });

    #[cfg(not(windows))]
    {
        let _ = ui.as_weak();
        info!("PTT 快捷鍵僅支援 Windows（設定: {}）", initial_hotkey);
    }

    hotkey_tx
}

// ─── 輔助函式 ─────────────────────────────────────────────────────────────────

fn conversion_mode_from_str(mode: &str) -> ConversionMode {
    match mode {
        "zh-HK" => ConversionMode::ZhHK,
        "zh-CN" => ConversionMode::ZhCN,
        "raw" => ConversionMode::Raw,
        _ => ConversionMode::ZhTW,
    }
}

fn convert_chinese_text(text: &str, opencc_enabled: bool, conversion_mode: &str) -> String {
    if !opencc_enabled {
        return text.to_string();
    }

    let mode = conversion_mode_from_str(conversion_mode);
    match OpenCCProcessor::new(mode) {
        Ok(processor) => processor.convert(text),
        Err(e) => {
            error!("OpenCC 轉換器初始化失敗: {e}");
            text.to_string()
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn transcribe_and_update(
    engine: &WhisperEngine,
    segment: crate::audio::AudioSegment,
    whisper_params: &WhisperParams,
    clipboard_enabled: bool,
    inject_enabled: bool,
    opencc_enabled: bool,
    conversion_mode: String,
    ui_weak: &slint::Weak<AppWindow>,
    transcript_list: &Arc<Mutex<Vec<TranscriptSegmentData>>>,
    wav_path: Option<PathBuf>,
) {
    let u = ui_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = u.upgrade() {
            ui.set_current_partial("⏳ 轉錄中...".into());
            ui.set_is_processing(true);
        }
    });

    match engine.transcribe(&segment, whisper_params) {
        Ok(result) => {
            let raw_text = result.text.trim().to_string();
            let text = convert_chinese_text(&raw_text, opencc_enabled, &conversion_mode);
            info!("轉錄結果: {:?}（{} 字）", text, text.chars().count());

            if text.is_empty() {
                let u = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_current_partial("".into());
                        ui.set_is_processing(false);
                    }
                });
                return;
            }

            if clipboard_enabled {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(&text);
                }
            }
            if inject_enabled {
                inject_text_to_focused_window(&text);
            }

            let ts = current_time_str();
            let seg = TranscriptSegmentData {
                text: text.clone().into(),
                timestamp: ts.into(),
            };

            let mut list = transcript_list.lock().unwrap();
            list.push(seg);
            let entries: Vec<TranscriptSegmentData> = list.clone();
            let char_total: i32 = list.iter().map(|s| s.text.chars().count() as i32).sum();
            drop(list);

            let u = ui_weak.clone();
            let status = wav_path
                .as_ref()
                .map(|p| format!("已轉錄，錄音保留於 {}", p.display()))
                .unwrap_or_else(|| "已轉錄".to_string());
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_transcript_segments(ModelRc::new(VecModel::from(entries)));
                    ui.set_char_count(char_total);
                    ui.set_current_partial("".into());
                    ui.set_is_processing(false);
                    ui.set_status_text(status.into());
                }
            });
        }
        Err(e) => {
            error!("轉錄失敗: {e}");
            let msg = format!("轉錄錯誤: {e}");
            let u = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_status_text(msg.into());
                    ui.set_current_partial("".into());
                    ui.set_is_processing(false);
                }
            });
        }
    }
}

fn current_time_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    // 轉換為 UTC+8（台灣時間）
    let h_tw = (h + 8) % 24;
    format!("{:02}:{:02}:{:02}", h_tw, m, s)
}

fn inject_text_to_focused_window(text: &str) {
    use enigo::{Enigo, Keyboard, Settings};
    match Enigo::new(&Settings::default()) {
        Ok(mut enigo) => {
            let _ = enigo.text(text);
        }
        Err(e) => {
            error!("enigo 初始化失敗: {e}");
        }
    }
}

// ─── 檔案模式 Callbacks ───────────────────────────────────────────────────────

fn setup_file_mode_callbacks(
    ui: &AppWindow,
    config: &AppConfig,
    selected_model_id: Arc<Mutex<String>>,
    model_runtime: ModelRuntime,
) -> anyhow::Result<()> {
    let whisper_params = config.whisper.clone();
    let models_dir = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    let tokio_handle = TokioHandle::current();

    // 開啟檔案對話框
    let ui_handle = ui.as_weak();
    ui.on_open_file_dialog(move || {
        let ui_handle = ui_handle.clone();
        std::thread::spawn(move || {
            let result = rfd::FileDialog::new()
                .add_filter(
                    "音訊/影片",
                    &[
                        "mp3", "mp4", "wav", "flac", "ogg", "m4a", "aac", "mkv", "mov", "avi",
                        "webm",
                    ],
                )
                .pick_file();

            if let Some(path) = result {
                let full_path = path.to_string_lossy().into_owned();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_handle.upgrade() {
                        ui.set_file_name(full_path.into());
                        ui.set_file_has_result(false);
                        ui.set_file_result("".into());
                        ui.set_file_progress(0.0);
                    }
                });
            }
        });
    });

    // 開始轉錄
    let ui_handle = ui.as_weak();
    let params = whisper_params.clone();
    let mdir = models_dir.clone();
    let model_id_arc = Arc::clone(&selected_model_id);
    let runtime = model_runtime.clone();
    let cfg = config.clone();
    let tokio_handle2 = tokio_handle.clone();
    ui.on_start_file_transcription(move || {
        let ui_weak = ui_handle.clone();

        let (file_path, model_id, fmt_str, opencc_enabled, conversion_mode) = {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let full: String = ui.get_file_name().into();
            if full.is_empty() {
                return;
            }
            let model: String = ui.get_selected_model_id().into();
            let fmt: String = ui.get_file_output_format().into();
            (
                PathBuf::from(full),
                model,
                fmt,
                ui.get_mod_opencc(),
                ui.get_conversion_mode().to_string(),
            )
        };

        if !file_path.exists() {
            return;
        }

        let ui_weak2 = ui_weak.clone();
        let params2 = params.clone();
        let mdir2 = mdir.clone();
        let model_id_arc2 = Arc::clone(&model_id_arc);
        let runtime2 = runtime.clone();
        let cfg2 = cfg.clone();
        let tokio_handle3 = tokio_handle2.clone();

        std::thread::spawn(move || {
            {
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_processing(true);
                        ui.set_file_progress(0.05);
                    }
                });
            }

            let decoded = match decode_audio_file(&file_path) {
                Ok(d) => d,
                Err(e) => {
                    let msg = format!("解碼失敗: {e}");
                    let uw = ui_weak2.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = uw.upgrade() {
                            ui.set_file_processing(false);
                            ui.set_file_result(msg.into());
                            ui.set_file_has_result(true);
                        }
                    });
                    return;
                }
            };

            {
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_progress(0.3);
                    }
                });
            }

            let id_lower = model_id.to_lowercase();
            let model_path = mdir2.join(format!("ggml-{}.bin", id_lower));

            if !model_path.exists() {
                let msg = format!(
                    "模型尚未下載: {}\n已開啟下載確認，下載完成後請再開始轉錄。",
                    model_path.display()
                );
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_processing(false);
                        ui.set_file_result(msg.into());
                        ui.set_file_has_result(true);
                    }
                });
                spawn_model_select_or_download(
                    tokio_handle3,
                    ui_weak2,
                    mdir2,
                    id_lower,
                    model_id_arc2,
                    runtime2,
                    cfg2,
                    params2.clone(),
                    true,
                );
                return;
            }

            let engine = match WhisperEngine::load(&model_path, &params2) {
                Ok(e) => e,
                Err(e) => {
                    let msg = format!("模型載入失敗: {e}");
                    let uw = ui_weak2.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = uw.upgrade() {
                            ui.set_file_processing(false);
                            ui.set_file_result(msg.into());
                            ui.set_file_has_result(true);
                        }
                    });
                    return;
                }
            };

            {
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_progress(0.5);
                    }
                });
            }

            let audio_seg = crate::audio::AudioSegment {
                samples: decoded.samples,
                duration_ms: decoded.duration_ms,
            };

            let result = match engine.transcribe(&audio_seg, &params2) {
                Ok(r) => r,
                Err(e) => {
                    let msg = format!("轉錄失敗: {e}");
                    let uw = ui_weak2.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = uw.upgrade() {
                            ui.set_file_processing(false);
                            ui.set_file_result(msg.into());
                            ui.set_file_has_result(true);
                        }
                    });
                    return;
                }
            };

            let fmt = ExportFormat::from_extension(&fmt_str);
            let text = convert_chinese_text(
                &format_transcript(&result.segments, fmt),
                opencc_enabled,
                &conversion_mode,
            );

            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak2.upgrade() {
                    ui.set_file_progress(1.0);
                    ui.set_file_processing(false);
                    ui.set_file_result(text.into());
                    ui.set_file_has_result(true);
                }
            });
        });
    });

    // 複製結果
    let ui_handle = ui.as_weak();
    ui.on_file_copy_result(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let text: String = ui.get_file_result().into();
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(text);
            }
        }
    });

    // 匯出結果
    let ui_handle = ui.as_weak();
    ui.on_file_export_result(move |fmt_shared| {
        let fmt_str: String = fmt_shared.into();
        let fmt = ExportFormat::from_extension(&fmt_str);

        if let Some(ui) = ui_handle.upgrade() {
            let text: String = ui.get_file_result().into();
            let file_name: String = ui.get_file_name().into();

            let stem = PathBuf::from(&file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("transcript")
                .to_string();

            std::thread::spawn(move || {
                let save_path = rfd::FileDialog::new()
                    .set_file_name(format!("{}.{}", stem, fmt.extension()))
                    .add_filter(fmt.extension().to_uppercase().as_str(), &[fmt.extension()])
                    .save_file();

                if let Some(path) = save_path {
                    if let Err(e) = std::fs::write(&path, &text) {
                        error!("匯出失敗: {e}");
                    } else {
                        info!("已匯出至: {}", path.display());
                    }
                }
            });
        }
    });

    Ok(())
}
