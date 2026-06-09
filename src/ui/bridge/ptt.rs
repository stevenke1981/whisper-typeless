// ─── PTT WH_KEYBOARD_LL 靜態狀態（hook callback 無法捕獲環境）──────────────

use crate::AppWindow;
use slint::ComponentHandle;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use tracing::{error, info};

pub static PTT_VKEY: AtomicU32 = AtomicU32::new(0);
pub static PTT_MODS: AtomicU32 = AtomicU32::new(0); // bit0=Alt bit1=Ctrl bit2=Shift
pub static PTT_HOOK_ENABLED: AtomicBool = AtomicBool::new(true);
// 防止 key repeat 重複觸發；KEYUP 時不需重新檢查 modifier（已可能釋放）
static PTT_ARMED: AtomicBool = AtomicBool::new(false);
// 在 hook proc 內部追蹤 modifier 狀態，避免 GetAsyncKeyState 非同步延遲問題
static PTT_CTRL_HELD: AtomicBool = AtomicBool::new(false);
static PTT_ALT_HELD: AtomicBool = AtomicBool::new(false);
static PTT_SHIFT_HELD: AtomicBool = AtomicBool::new(false);
// SyncSender 從 hook callback 送出 is_down 事件
static PTT_HOOK_TX: OnceLock<Mutex<Option<mpsc::SyncSender<bool>>>> = OnceLock::new();
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

pub fn setup_ptt_hotkey(ui: &AppWindow, initial_hotkey: String) -> mpsc::Sender<String> {
    // outer_tx is returned to callers; inner_rx is used by the hook thread.
    // The adapter thread bridges the two and posts WM_APP to wake GetMessageA.
    let (outer_tx, outer_rx) = mpsc::channel::<String>();
    let (hotkey_tx_inner, hotkey_rx) = mpsc::channel::<String>();

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
    let (event_tx, event_rx) = mpsc::sync_channel::<bool>(32);
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
