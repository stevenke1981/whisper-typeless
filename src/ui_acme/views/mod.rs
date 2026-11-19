//! 檢視：`AppViewState`（＋畫面本地狀態）→ [`View`] 描述樹的純函式。
//!
//! - 浮動模式：緊湊列 380×52，展開為 380×540（快速操作、模板列、波形、轉錄 / 檔案模式）。
//! - 完整模式：標題列、即時 / 檔案分頁、模板列、波形、轉錄、控制列、狀態列。
//! - 浮層：錯誤紀錄、設定面板（兩種模式共用）。

pub mod file;
pub mod live;
pub mod overlay;
pub mod settings;
pub mod widgets;

use super::local::UiLocal;
use super::paint::{r, View};
use super::text::TextEngine;
use super::theme::Theme;
use crate::core::AppViewState;

/// 視窗尺寸（邏輯像素），與 Slint 版一致。
pub const FULL_W: f32 = 560.0;
pub const FULL_H: f32 = 680.0;
pub const BAR_W: f32 = 380.0;
pub const BAR_H: f32 = 52.0;
pub const EXPANDED_H: f32 = 540.0;

/// 情境模板列（與 Slint `AppWindow.templates` 相同）。
pub const TEMPLATES: [(&str, &str, &str); 6] = [
    ("auto", "🔮", "自動"),
    ("meeting", "🏢", "會議"),
    ("casual", "💬", "口語"),
    ("technical", "💻", "技術"),
    ("medical", "🏥", "醫療"),
    ("legal", "⚖", "法律"),
];

/// 模板 id → 顯示名稱。
pub fn template_label(id: &str) -> &str {
    TEMPLATES
        .iter()
        .find(|(tid, _, _)| *tid == id)
        .map(|(_, _, name)| *name)
        .unwrap_or(id)
}

/// 目前狀態下的視窗邏輯尺寸。
pub fn window_size(s: &AppViewState) -> (f32, f32) {
    if !s.floating_mode {
        (FULL_W, FULL_H)
    } else if s.floating_expanded {
        (BAR_W, EXPANDED_H)
    } else {
        (BAR_W, BAR_H)
    }
}

/// 建構檢視時需要的一切。
pub struct Ctx<'a> {
    pub s: &'a AppViewState,
    pub ui: &'a UiLocal,
    pub t: &'static Theme,
    pub text: &'a mut TextEngine,
    pub scale: f32,
}

/// 根檢視。
pub fn build(cx: &mut Ctx<'_>, width: f32, height: f32) -> View {
    let mut v = View::new();
    if cx.s.floating_mode {
        live::floating(&mut v, cx, width, height);
    } else {
        live::full(&mut v, cx, width, height);
    }
    if cx.s.show_error_log {
        v.layer(4);
        overlay::error_log(&mut v, cx, width, height);
    }
    if cx.s.settings_open {
        v.layer(6);
        settings::panel(&mut v, cx, r(0.0, 0.0, width, height));
    }
    v
}
