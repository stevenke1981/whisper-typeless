//! 色彩主題：數值取自 `ui/styles/tokens.slint`（Tokens / DarkTokens / GlassDark）。
//!
//! 所有檢視只透過 [`Theme`] 取色，不在檢視裡散落硬編碼顏色。

use acme_core::Color;

/// `#rrggbb` → 不透明顏色。
pub const fn hex(rgb: u32) -> Color {
    rgba8(rgb >> 16, (rgb >> 8) & 0xff, rgb & 0xff, 1.0)
}

/// 0–255 的 RGB 加上 0–1 的 alpha。
pub const fn rgba8(r: u32, g: u32, b: u32, a: f32) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a,
    }
}

/// 同一顏色換 alpha。
pub const fn with_alpha(c: Color, a: f32) -> Color {
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a,
    }
}

/// 一組完整的畫面顏色。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub bg: Color,
    pub bg2: Color,
    pub bg3: Color,
    pub text: Color,
    pub text2: Color,
    pub placeholder: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub accent_line: Color,
    pub danger: Color,
    pub danger_soft: Color,
    pub danger_line: Color,
    pub success: Color,
    pub success_soft: Color,
    pub success_line: Color,
    pub warning: Color,
    /// 錄音中卡片 / 列底（`recording` 的淡色）。
    pub recording_soft: Color,
    /// 危險文字（比 `danger` 深，可放在淺底上）。
    pub danger_text: Color,
    pub success_text: Color,
    pub warning_text: Color,
    pub warning_soft: Color,
    pub warning_line: Color,
    pub border: Color,
    pub border_strong: Color,
    pub recording: Color,
    pub recording_pulse: Color,
    pub glass_bg: Color,
    pub glass_stroke: Color,
    pub glass_hairline: Color,
    pub chip_bg: Color,
    pub chip_bg_active: Color,
    pub on_accent: Color,
    pub scrim: Color,
    pub hover: Color,
    pub overlay_bg: Color,
}

const WHITE: Color = hex(0xffffff);
const RED: Color = hex(0xff3b30);
const GREEN: Color = hex(0x34c759);

/// 淺色（Tokens + Liquid Glass 淺色）。
pub const LIGHT: Theme = Theme {
    dark: false,
    bg: hex(0xf5f5f7),
    bg2: hex(0xffffff),
    bg3: hex(0xe8e8ed),
    text: hex(0x1d1d1f),
    text2: hex(0x6e6e73),
    placeholder: hex(0x8e8e93),
    accent: hex(0x0071e3),
    accent_soft: rgba8(0, 113, 227, 0.07),
    accent_line: rgba8(0, 113, 227, 0.25),
    danger: RED,
    danger_soft: rgba8(255, 59, 48, 0.12),
    danger_line: rgba8(255, 59, 48, 0.3),
    success: GREEN,
    success_soft: rgba8(52, 199, 89, 0.15),
    success_line: rgba8(52, 199, 89, 0.45),
    warning: hex(0xff9f0a),
    recording_soft: rgba8(255, 59, 48, 0.10),
    danger_text: hex(0xd70015),
    success_text: hex(0x1a7f37),
    warning_text: hex(0x9a5b00),
    warning_soft: rgba8(255, 159, 10, 0.14),
    warning_line: rgba8(255, 159, 10, 0.40),
    border: rgba8(0, 0, 0, 0.1),
    border_strong: rgba8(0, 0, 0, 0.2),
    recording: RED,
    recording_pulse: rgba8(255, 59, 48, 0.3),
    glass_bg: rgba8(255, 255, 255, 0.82),
    glass_stroke: rgba8(255, 255, 255, 0.55),
    glass_hairline: rgba8(0, 0, 0, 0.06),
    chip_bg: rgba8(0, 0, 0, 0.045),
    chip_bg_active: rgba8(0, 113, 227, 0.14),
    on_accent: WHITE,
    scrim: rgba8(0, 0, 0, 0.45),
    hover: rgba8(0, 0, 0, 0.06),
    overlay_bg: rgba8(255, 255, 255, 0.72),
};

/// 深色（DarkTokens + GlassDark）。
pub const DARK: Theme = Theme {
    dark: true,
    bg: hex(0x1c1c1e),
    bg2: hex(0x2c2c2e),
    bg3: hex(0x3a3a3c),
    text: hex(0xf5f5f7),
    text2: hex(0x98989d),
    placeholder: hex(0x8e8e93),
    accent: hex(0x0a84ff),
    accent_soft: rgba8(10, 132, 255, 0.12),
    accent_line: rgba8(10, 132, 255, 0.35),
    danger: RED,
    danger_soft: rgba8(255, 59, 48, 0.15),
    danger_line: rgba8(255, 59, 48, 0.35),
    success: hex(0x30d158),
    success_soft: rgba8(48, 209, 88, 0.18),
    success_line: rgba8(48, 209, 88, 0.45),
    warning: hex(0xff9f0a),
    recording_soft: rgba8(255, 69, 58, 0.18),
    danger_text: hex(0xff6961),
    success_text: hex(0x30d158),
    warning_text: hex(0xffb340),
    warning_soft: rgba8(255, 159, 10, 0.20),
    warning_line: rgba8(255, 159, 10, 0.45),
    border: rgba8(255, 255, 255, 0.1),
    border_strong: rgba8(255, 255, 255, 0.18),
    recording: hex(0xff453a),
    recording_pulse: rgba8(255, 69, 58, 0.3),
    glass_bg: rgba8(28, 28, 30, 0.88),
    glass_stroke: rgba8(255, 255, 255, 0.10),
    glass_hairline: rgba8(255, 255, 255, 0.07),
    chip_bg: rgba8(255, 255, 255, 0.06),
    chip_bg_active: rgba8(10, 132, 255, 0.22),
    on_accent: WHITE,
    scrim: rgba8(0, 0, 0, 0.45),
    hover: rgba8(255, 255, 255, 0.08),
    overlay_bg: rgba8(28, 28, 30, 0.72),
};

/// 語意色調：狀態卡、膠囊、群組卡片共用。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Recording,
    Success,
    Warning,
    Danger,
}

/// 一個色調的四種用色。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneColors {
    /// 放在 `soft` 底上的文字色。
    pub fg: Color,
    /// 淡色底（卡片、膠囊）。
    pub soft: Color,
    /// 1px 邊框。
    pub line: Color,
    /// 實心（圓點、色條、進度條）。
    pub solid: Color,
}

impl Theme {
    pub fn for_mode(dark: bool) -> &'static Theme {
        if dark {
            &DARK
        } else {
            &LIGHT
        }
    }

    /// 色調 → 顏色。`Neutral` 的 `soft` 是 chip 底；卡片的中性底請用 `bg2` + `border`。
    pub fn tone(&self, tone: Tone) -> ToneColors {
        match tone {
            Tone::Neutral => ToneColors {
                fg: self.text2,
                soft: self.chip_bg,
                line: self.border,
                solid: self.text2,
            },
            Tone::Accent => ToneColors {
                fg: self.accent,
                soft: self.accent_soft,
                line: self.accent_line,
                solid: self.accent,
            },
            Tone::Recording => ToneColors {
                fg: self.danger_text,
                soft: self.recording_soft,
                line: self.danger_line,
                solid: self.recording,
            },
            Tone::Success => ToneColors {
                fg: self.success_text,
                soft: self.success_soft,
                line: self.success_line,
                solid: self.success,
            },
            Tone::Warning => ToneColors {
                fg: self.warning_text,
                soft: self.warning_soft,
                line: self.warning_line,
                solid: self.warning,
            },
            Tone::Danger => ToneColors {
                fg: self.danger_text,
                soft: self.danger_soft,
                line: self.danger_line,
                solid: self.danger,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_matches_tokens() {
        let c = hex(0x0071e3);
        assert_eq!(c.r, 0.0);
        assert!((c.g - 113.0 / 255.0).abs() < 1e-6);
        assert!((c.b - 227.0 / 255.0).abs() < 1e-6);
        assert_eq!(c.a, 1.0);
        assert_eq!(with_alpha(c, 0.5).a, 0.5);
    }

    #[test]
    fn modes_differ_and_are_opaque_where_needed() {
        assert!(!Theme::for_mode(false).dark);
        assert!(Theme::for_mode(true).dark);
        for t in [&LIGHT, &DARK] {
            for c in [t.bg, t.bg2, t.bg3, t.text, t.accent] {
                assert_eq!(c.a, 1.0);
            }
        }
        assert_ne!(LIGHT.bg, DARK.bg);
    }

    #[test]
    fn tones_map_to_distinct_solids() {
        for t in [&LIGHT, &DARK] {
            assert_eq!(t.tone(Tone::Accent).solid, t.accent);
            assert_eq!(t.tone(Tone::Warning).fg, t.warning_text);
            assert_eq!(t.tone(Tone::Recording).soft, t.recording_soft);
            assert_ne!(t.tone(Tone::Success).solid, t.tone(Tone::Danger).solid);
            // 說明文字不可用 placeholder。
            assert_ne!(t.tone(Tone::Neutral).fg, t.placeholder);
        }
        assert_eq!(LIGHT.placeholder, hex(0x8e8e93));
    }
}
