// ─── 全域快捷鍵（RegisterHotKey / PeekMessageA 事件驅動）────────────────────
//
// 提供 WM_HOTKEY 式的全域快捷鍵，使用 winapi RegisterHotKey + PeekMessageA
// 訊息迴圈監聽切換錄音的快捷鍵，支援執行中動態更換按鍵組合。

use crate::AppWindow;
use slint::ComponentHandle;
use tracing::info;

/// 解析 Windows 虛擬按鍵碼（VK_*）
#[cfg(windows)]
pub fn parse_windows_vkey(raw_key: &str, fallback: u32) -> u32 {
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

/// 解析「Ctrl+Alt+W」格式的快捷鍵字串，回傳 (modifiers, vkey)
#[cfg(windows)]
pub fn parse_global_hotkey(s: &str) -> (u32, u32) {
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

/// 啟動全域快捷鍵監聽執行緒（RegisterHotKey + PeekMessageA）
///
/// 回傳 `Sender<String>`，送出新的快捷鍵字串可執行中動態更換。
pub fn setup_global_hotkey(
    ui: &AppWindow,
    initial_hotkey: String,
) -> std::sync::mpsc::Sender<String> {
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
