//! 無視窗的 UI 快照工具（設計 / 實作者用來「看」AcmeUI 畫面）。
//!
//! 走與正式視窗相同的路徑：同一組 view 函式、同一個 [TextEngine] 與系統字型，
//! 以 CPU 渲染器畫成 PNG。預設忽略，需明確執行：
//!
//! SNAPSHOT_DIR=<資料夾> cargo test --no-default-features --lib ui_snapshots -- --ignored
//!
//! 不碰音訊 / GPU / 網路 / 模型檔；PNG 由內建的最小編碼器寫出（不增加相依）。

use super::tests::{app_with, models, templates};
use super::*;
use crate::core::TranscriptSegment;
use crate::ui_acme::local::UiLocal;
use crate::ui_acme::msg::SettingsTab;
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::TextEngine;
use crate::ui_acme::theme::Theme;
use crate::ui_acme::views::Ctx;
use acme_render_cpu::CpuRenderer;
use std::path::Path;

const FULL: (f32, f32) = (560.0, 620.0);
const BAR: (f32, f32) = (380.0, 52.0);
const PANEL: (f32, f32) = (380.0, 540.0);
const SCALES: [f32; 2] = [1.5, 1.0];

// ─── 最小 PNG 編碼器（Sub 濾波 + 固定 Huffman 的 deflate，只用距離 1 的連續比對） ──

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &d in data {
        a = (a + d as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Bits {
    fn put(&mut self, value: u32, count: u32) {
        self.acc |= value << self.n;
        self.n += count;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// Huffman 碼要由高位元先寫。
    fn code(&mut self, code: u32, len: u32) {
        let mut rev = 0;
        for i in 0..len {
            rev |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.put(rev, len);
    }
    fn symbol(&mut self, sym: u32) {
        match sym {
            0..=143 => self.code(0x30 + sym, 8),
            144..=255 => self.code(0x190 + sym - 144, 9),
            256..=279 => self.code(sym - 256, 7),
            _ => self.code(0xC0 + sym - 280, 8),
        }
    }
}

const LEN_BASE: [u32; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u32; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut w = Bits {
        out: Vec::new(),
        acc: 0,
        n: 0,
    };
    w.put(1, 1); // BFINAL
    w.put(1, 2); // 固定 Huffman
    let mut i = 0;
    while i < data.len() {
        let mut run = 0;
        if i > 0 {
            while i + run < data.len() && run < 258 && data[i + run] == data[i - 1] {
                run += 1;
            }
        }
        if run >= 3 {
            let idx = LEN_BASE.iter().rposition(|&b| b <= run as u32).unwrap();
            w.symbol(257 + idx as u32);
            w.put(run as u32 - LEN_BASE[idx], LEN_EXTRA[idx]);
            w.code(0, 5); // 距離 1
            i += run;
        } else {
            w.symbol(data[i] as u32);
            i += 1;
        }
    }
    w.symbol(256);
    if w.n > 0 {
        w.out.push(w.acc as u8);
    }
    w.out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend((body.len() as u32).to_be_bytes());
    let mut c = kind.to_vec();
    c.extend(body);
    out.extend(&c);
    out.extend(crc32(&c).to_be_bytes());
}

fn encode_png(width: u32, height: u32, pixels: &[u32]) -> Vec<u8> {
    let stride = width as usize * 3;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for row in pixels.chunks(width as usize) {
        let rgb: Vec<u8> = row
            .iter()
            .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
            .collect();
        raw.push(1); // Sub
        for (i, &v) in rgb.iter().enumerate() {
            raw.push(if i >= 3 {
                v.wrapping_sub(rgb[i - 3])
            } else {
                v
            });
        }
    }
    let mut z = vec![0x78, 0x01];
    z.extend(deflate(&raw));
    z.extend(adler32(&raw).to_be_bytes());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend(width.to_be_bytes());
    ihdr.extend(height.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    png
}

// ─── 假資料 ─────────────────────────────────────────────────────────────────

fn segments() -> Vec<TranscriptSegment> {
    [
        ("10:02", "大家早安，今天我們先回顧上週的進度。"),
        (
            "10:03",
            "語音辨識的準確率已經提升到九成五，延遲也降到一秒以內。",
        ),
        ("10:05", "接下來請小明報告雲端轉錄與隱私模式的測試結果。"),
        (
            "10:07",
            "目前所有的語音都只在本機處理，只有模型下載會連網。",
        ),
        (
            "10:09",
            "下週一前請大家確認浮動視窗在一百五十趴縮放下的顯示。",
        ),
        ("10:12", "如果沒有其他問題，今天的會議就到這裡，謝謝大家。"),
    ]
    .into_iter()
    .map(|(t, s)| TranscriptSegment {
        text: s.into(),
        timestamp: t.into(),
    })
    .collect()
}

/// 與實際程式相同的五個內建模板（核心用的顯示名稱）+ 兩個自訂模板。
fn snap_templates() -> Vec<crate::core::TemplateEntryView> {
    let builtin = ["會議記錄", "口語對話", "技術討論", "醫療紀錄", "法律文書"];
    let mut list: Vec<_> = builtin
        .iter()
        .map(|name| crate::core::TemplateEntryView {
            name: (*name).into(),
            prompt: format!("以下是{name}。"),
            is_builtin: true,
        })
        .collect();
    let custom = templates()[1].clone();
    list.push(custom);
    list.push(crate::core::TemplateEntryView {
        name: "客戶拜訪逐字稿".into(),
        prompt: "業務拜訪客戶的對話，常見詞：報價、交期、規格。".into(),
        is_builtin: false,
    });
    list
}

fn base() -> AppViewState {
    AppViewState {
        model_entries: models(),
        template_entries: snap_templates(),
        // 模板列以 id 比對選取狀態（D14）。
        current_template: "meeting".into(),
        device_label: "CPU".into(),
        error_log: vec![
            "10:01:12 模型下載失敗：連線逾時，請稍後再試".into(),
            "10:03:40 找不到麥克風裝置，已改用預設輸入".into(),
            "10:08:05 OpenAI 轉錄失敗：金鑰無效（已退回本機轉錄）".into(),
        ],
        ..AppViewState::default()
    }
}

fn with_text(mut s: AppViewState) -> AppViewState {
    s.transcript_segments = segments();
    s.char_count = s
        .transcript_segments
        .iter()
        .map(|x| x.text.chars().count() as i32)
        .sum();
    s
}

fn recording(mut s: AppViewState) -> AppViewState {
    s.is_recording = true;
    s.status_text = "錄音中".into();
    s.audio_level = 0.6;
    s.waveform_bars = (0..20)
        .map(|i| 0.15 + 0.8 * ((i as f32 * 0.7).sin().abs()))
        .collect();
    s.current_partial = "接下來我們討論一下下一個版本的".into();
    s
}

fn processing(mut s: AppViewState) -> AppViewState {
    s.is_processing = true;
    s.status_text = "辨識中".into();
    s
}

// ─── 繪製 ───────────────────────────────────────────────────────────────────

struct Shooter<'a> {
    dir: &'a Path,
    n: u32,
    written: Vec<String>,
}

impl Shooter<'_> {
    fn render(&mut self, app: &mut AcmeApp, size: (f32, f32), scale: f32, name: &str) {
        let scene = app.build_scene(size.0, size.1, scale).expect("scene");
        let (pw, ph) = (
            (size.0 * scale).round() as u32,
            (size.1 * scale).round() as u32,
        );
        let mut cpu = CpuRenderer::new(pw, ph, scale).expect("cpu renderer");
        cpu.render_scene(&scene).expect("render");
        let png = encode_png(pw, ph, cpu.pixels());
        let file = format!("{name}@{scale}x.png");
        std::fs::write(self.dir.join(&file), png).expect("write png");
        self.written.push(file);
    }

    /// 同一個畫面各畫 1.5x / 1.0x。
    fn shot(&mut self, name: &str, state: AppViewState, size: (f32, f32)) {
        self.shot_with(name, state, size, |_| {});
    }

    fn shot_with(
        &mut self,
        name: &str,
        state: AppViewState,
        size: (f32, f32),
        setup: impl Fn(&mut AcmeApp),
    ) {
        self.n += 1;
        let name = format!("{:02}_{name}", self.n);
        for scale in SCALES {
            let (mut app, _n) = app_with(state.clone());
            app.ui.anim = 3;
            setup(&mut app);
            self.render(&mut app, size, scale, &name);
        }
    }

    /// 設定面板由上往下捲過全部內容（每步約一個視窗高）。
    fn settings_series(
        &mut self,
        name: &str,
        state: AppViewState,
        size: (f32, f32),
        setup: impl Fn(&mut AcmeApp),
    ) {
        for scale in SCALES {
            let (mut app, _n) = app_with(state.clone());
            setup(&mut app);
            app.build_scene(size.0, size.1, scale).expect("scene");
            let area = app
                .hitmap
                .scrolls
                .iter()
                .find(|a| a.id == ScrollId::Settings)
                .copied()
                .expect("settings scroll area");
            let step = (area.rect.size.height.get() * 0.85).max(1.0);
            let mut offsets = vec![0.0f32];
            while *offsets.last().unwrap() + 0.5 < area.max {
                let next = (offsets.last().unwrap() + step).min(area.max);
                offsets.push(next);
            }
            // 每頁用全新的 app + 渲染器：字形圖集是增量上傳，不能跨渲染器重用。
            for (k, off) in offsets.iter().enumerate() {
                let (mut page, _n) = app_with(state.clone());
                setup(&mut page);
                page.ui.set_scroll(ScrollId::Settings, *off);
                let n = self.n + 1 + k as u32;
                self.render(&mut page, size, scale, &format!("{n:02}_{name}_p{}", k + 1));
            }
            if scale == SCALES[SCALES.len() - 1] {
                self.n += offsets.len() as u32;
            }
        }
    }
    /// 契約元件總覽（不經 app 畫面；給 WP-2 / WP-3 對照元件外觀）。
    fn gallery(&mut self, name: &str, dark: bool) {
        self.n += 1;
        let name = format!("{:02}_{name}", self.n);
        for scale in SCALES {
            let state = AppViewState {
                dark_mode: dark,
                ..base()
            };
            let ui = UiLocal::default();
            let mut text = TextEngine::new();
            let theme = Theme::for_mode(dark);
            let mut v = View::new();
            {
                let mut cx = Ctx {
                    s: &state,
                    ui: &ui,
                    t: theme,
                    text: &mut text,
                    scale,
                };
                draw_gallery(&mut v, &mut cx);
            }
            let mut painter = Painter::with_clear(theme.bg);
            let mut hit = HitTestLayer::new();
            paint_view(&v, &mut text, &mut painter, &mut hit, None, scale);
            let scene = painter.finish_checked().expect("scene");
            let (pw, ph) = (
                (FULL.0 * scale).round() as u32,
                (FULL.1 * scale).round() as u32,
            );
            let mut cpu = CpuRenderer::new(pw, ph, scale).expect("cpu renderer");
            cpu.render_scene(&scene).expect("render");
            let file = format!("{name}@{scale}x.png");
            std::fs::write(self.dir.join(&file), encode_png(pw, ph, cpu.pixels()))
                .expect("write png");
            self.written.push(file);
        }
    }
}

fn draw_gallery(v: &mut View, cx: &mut Ctx<'_>) {
    use crate::ui_acme::icons::{icon, Icon};
    use crate::ui_acme::theme::Tone;
    use crate::ui_acme::views::widgets::*;
    let t = cx.t;
    let noop = || Msg::Core(UiCommand::ClearTranscript);
    // 按鈕
    let mut x0 = 16.0;
    for (label, kind, on) in [
        ("主要", Btn::Primary, true),
        ("次要", Btn::Secondary, true),
        ("幽靈", Btn::Ghost, true),
        ("刪除", Btn::Danger, true),
        ("停用", Btn::Primary, false),
    ] {
        btn(
            v,
            cx,
            r(x0, 16.0, 88.0, CTRL_MD),
            label,
            None,
            kind,
            on.then(noop),
        );
        x0 += 96.0;
    }
    btn(
        v,
        cx,
        r(16.0, 56.0, 120.0, CTRL_MD),
        "下載",
        Some(Icon::Download),
        Btn::Secondary,
        Some(noop()),
    );
    btn(
        v,
        cx,
        r(144.0, 56.0, 200.0, CTRL_LG),
        "開始轉錄",
        None,
        Btn::Primary,
        Some(noop()),
    );
    // 圖示 + 圖示按鈕
    for (i, ic) in Icon::ALL.into_iter().enumerate() {
        let x = 16.0 + i as f32 * 32.0;
        icon_btn(v, t, r(x, 108.0, 28.0, 28.0), ic, i == 6, noop());
        icon(v, r(x + 6.0, 142.0, 16.0, 16.0), ic, t.text);
    }
    // 開關
    toggle(v, t, r(16.0, 168.0, 44.0, 32.0), true, Some(noop()));
    toggle(v, t, r(72.0, 168.0, 44.0, 32.0), false, Some(noop()));
    toggle(v, t, r(128.0, 168.0, 44.0, 32.0), true, None);
    toggle(v, t, r(184.0, 168.0, 44.0, 32.0), false, None);
    // 分段
    segmented_fill(
        v,
        cx,
        r(248.0, 168.0, 296.0, CTRL_MD),
        &[
            ("a", "台灣繁體"),
            ("b", "香港繁體"),
            ("c", "簡體"),
            ("d", "不轉換"),
        ],
        "a",
        true,
        |_| noop(),
    );
    segmented_fill(
        v,
        cx,
        r(16.0, 208.0, 200.0, CTRL_MD),
        &[("local", "本機"), ("openai", "OpenAI 雲端")],
        "local",
        false,
        |_| noop(),
    );
    // 膠囊、鍵帽
    let mut px = 16.0;
    for (label, tone) in [
        ("本機 · small", Tone::Neutral),
        ("下載中", Tone::Accent),
        ("錄音中", Tone::Recording),
        ("已設定", Tone::Success),
        ("雲端 · OpenAI", Tone::Warning),
        ("未設定", Tone::Danger),
    ] {
        px += status_pill(v, cx, px, 248.0, label, tone, None) + 6.0;
    }
    let mut cxx = 16.0;
    cxx += toggle_chip(v, cx, cxx, 280.0, "自動貼上", true, noop()) + 8.0;
    cxx += toggle_chip(v, cx, cxx, 280.0, "剪貼簿", false, noop()) + 16.0;
    cxx += keycap(v, cx, cxx, 283.0, "Ctrl+Shift+W") + 8.0;
    keycap(v, cx, cxx, 283.0, "Ctrl+Alt+L");
    // 卡片
    for (i, tone) in [Tone::Neutral, Tone::Accent, Tone::Recording, Tone::Warning]
        .into_iter()
        .enumerate()
    {
        card(
            v,
            t,
            r(16.0 + i as f32 * 134.0, 320.0, 126.0, 40.0),
            RADIUS_LG,
            tone,
        );
    }
    // 群組 + 設定列
    let g = group_begin(v, cx, 16.0, 372.0, 528.0, "快捷鍵", Tone::Neutral);
    let mut y = g.y;
    let (h1, c1) = setting_row(v, cx, &g, y, "開始／停止錄音", None, true, false);
    v.bordered(c1, RADIUS_SM, t.bg2, 1.0, t.border);
    y += h1;
    let (h2, c2) = setting_row(
        v,
        cx,
        &g,
        y,
        "按住說話",
        Some("按住快捷鍵時錄音，放開就送出辨識"),
        true,
        true,
    );
    toggle(
        v,
        t,
        r(
            c2.origin.x.get() + c2.size.width.get() - TOGGLE_W,
            c2.origin.y.get(),
            TOGGLE_W,
            CTRL_MD,
        ),
        true,
        Some(noop()),
    );
    y += h2;
    let (h3, _c3) = setting_row(v, cx, &g, y, "按住說話快捷鍵", None, false, true);
    y += h3;
    let next = group_end(v, g, y);
    let g2 = group_begin(v, cx, 16.0, next, 528.0, "OpenAI 雲端", Tone::Warning);
    let (h4, c4) = setting_row(
        v,
        cx,
        &g2,
        g2.y,
        "我同意把語音傳送到 OpenAI",
        Some("需要網路，費用由你的 API 帳號支付；檔案轉錄仍使用本機模型。"),
        true,
        false,
    );
    toggle(
        v,
        t,
        r(
            c4.origin.x.get() + c4.size.width.get() - TOGGLE_W,
            c4.origin.y.get(),
            TOGGLE_W,
            CTRL_MD,
        ),
        false,
        Some(noop()),
    );
    let y_end = g2.y + h4;
    group_end(v, g2, y_end);
}

#[test]
#[ignore = "手動執行：SNAPSHOT_DIR=<dir> cargo test --lib ui_snapshots -- --ignored"]
fn ui_snapshots() {
    let dir = std::env::var("SNAPSHOT_DIR").expect("請設定 SNAPSHOT_DIR");
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).expect("create SNAPSHOT_DIR");
    let mut s = Shooter {
        dir,
        n: 0,
        written: Vec::new(),
    };
    let float = |mut st: AppViewState| {
        st.floating_mode = true;
        st
    };
    let expanded = |mut st: AppViewState| {
        st.floating_mode = true;
        st.floating_expanded = true;
        st
    };
    let settings = |mut st: AppViewState| {
        st.settings_open = true;
        st
    };
    let dark = |mut st: AppViewState| {
        st.dark_mode = true;
        st
    };

    let tab = |t: SettingsTab| move |app: &mut AcmeApp| app.ui.settings_tab = t;
    let needs_model = |st: AppViewState| AppViewState {
        selected_model_id: "large-v3".into(),
        model_name: "Large v3".into(),
        ..st
    };
    let cloud = |st: AppViewState| AppViewState {
        engine: "openai".into(),
        openai_consent: true,
        openai_key_set: true,
        ..st
    };

    // 浮動列
    s.shot("overlay_idle", float(base()), BAR);
    s.shot("overlay_recording", float(recording(base())), BAR);
    s.shot("overlay_processing", float(processing(base())), BAR);
    s.shot("overlay_needs_model", float(needs_model(base())), BAR);
    s.shot("overlay_expanded_idle", expanded(base()), PANEL);
    s.shot(
        "overlay_expanded_recording",
        recording(expanded(with_text(base()))),
        PANEL,
    );
    s.shot(
        "overlay_expanded_file",
        expanded(AppViewState {
            file_mode: true,
            ..base()
        }),
        PANEL,
    );
    s.shot(
        "overlay_expanded_dark",
        dark(expanded(with_text(base()))),
        PANEL,
    );
    // 錯誤紀錄改為展開尺寸（收合時 Msg::ToggleErrorLog 會先展開）。
    s.shot(
        "overlay_error_log",
        expanded(AppViewState {
            show_error_log: true,
            ..base()
        }),
        PANEL,
    );

    // 完整視窗：即時
    s.shot("live_idle_empty", base(), FULL);
    s.shot("live_recording", recording(base()), FULL);
    s.shot("live_transcripts", with_text(base()), FULL);
    s.shot(
        "live_recording_transcripts",
        recording(with_text(base())),
        FULL,
    );
    s.shot("live_processing", processing(with_text(base())), FULL);
    s.shot(
        "live_model_downloading",
        AppViewState {
            downloading_id: "large-v3".into(),
            download_progress: 0.42,
            status_text: "下載模型中 42%".into(),
            ..base()
        },
        FULL,
    );
    s.shot("live_needs_model", needs_model(base()), FULL);
    s.shot(
        "live_ptt_off_hotkey",
        AppViewState {
            ptt_mode: false,
            ..with_text(base())
        },
        FULL,
    );
    s.shot("live_cloud_active", cloud(with_text(base())), FULL);
    s.shot(
        "live_cloud_incomplete",
        AppViewState {
            engine: "openai".into(),
            openai_consent: false,
            openai_key_set: false,
            ..with_text(base())
        },
        FULL,
    );
    s.shot(
        "live_privacy",
        AppViewState {
            privacy_mode: true,
            ..with_text(base())
        },
        FULL,
    );
    s.shot(
        "live_no_errors",
        AppViewState {
            error_log: Vec::new(),
            ..with_text(base())
        },
        FULL,
    );
    s.shot(
        "live_output_off",
        AppViewState {
            clipboard_enabled: false,
            inject_enabled: false,
            ..with_text(base())
        },
        FULL,
    );
    s.shot(
        "live_error_log",
        AppViewState {
            show_error_log: true,
            ..with_text(base())
        },
        FULL,
    );

    // 檔案分頁
    let file = |st: AppViewState| AppViewState {
        file_mode: true,
        ..st
    };
    s.shot("file_empty", file(base()), FULL);
    s.shot(
        "file_selected",
        file(AppViewState {
            file_name: "C:\\錄音\\2024-第三季-產品規劃會議-完整錄音.mp3".into(),
            ..base()
        }),
        FULL,
    );
    s.shot(
        "file_processing",
        file(AppViewState {
            file_name: "C:\\錄音\\訪談.mp3".into(),
            file_processing: true,
            file_progress: 0.37,
            ..base()
        }),
        FULL,
    );
    s.shot(
        "file_result",
        file(AppViewState {
            file_name: "C:\\錄音\\訪談.mp3".into(),
            file_has_result: true,
            file_result: "這是一段訪談的轉錄結果，內容包含多個段落與標點符號。".repeat(12),
            file_output_format: "txt".into(),
            ..base()
        }),
        FULL,
    );

    // 錯誤紀錄（空）
    s.shot(
        "error_log_empty",
        AppViewState {
            show_error_log: true,
            error_log: Vec::new(),
            ..base()
        },
        FULL,
    );

    // 設定：每個分頁一張（模型與模板頁可捲動）
    s.shot_with(
        "settings_general",
        settings(base()),
        FULL,
        tab(SettingsTab::General),
    );
    s.shot_with(
        "settings_engine_local",
        settings(base()),
        FULL,
        tab(SettingsTab::Engine),
    );
    s.shot_with(
        "settings_engine_cloud",
        settings(cloud(base())),
        FULL,
        tab(SettingsTab::Engine),
    );
    s.shot_with(
        "settings_engine_cloud_no_key",
        settings(AppViewState {
            openai_key_set: false,
            ..cloud(base())
        }),
        FULL,
        tab(SettingsTab::Engine),
    );
    s.shot_with(
        "settings_engine_privacy",
        settings(AppViewState {
            privacy_mode: true,
            ..cloud(base())
        }),
        FULL,
        tab(SettingsTab::Engine),
    );
    s.settings_series(
        "settings_models",
        settings(AppViewState {
            device_label: "Vulkan".into(),
            ..base()
        }),
        FULL,
        tab(SettingsTab::Models),
    );
    s.shot_with(
        "settings_models_downloading",
        settings(AppViewState {
            downloading_id: "large-v3".into(),
            download_progress: 0.42,
            ..base()
        }),
        FULL,
        tab(SettingsTab::Models),
    );
    s.shot_with(
        "settings_output",
        settings(base()),
        FULL,
        tab(SettingsTab::Output),
    );
    s.settings_series(
        "settings_templates",
        settings(base()),
        FULL,
        tab(SettingsTab::Templates),
    );
    s.settings_series("settings_templates_edit", settings(base()), FULL, |app| {
        app.ui.settings_tab = SettingsTab::Templates;
        app.ui.edit_template(5, &snap_templates()[5]);
    });
    s.shot_with(
        "settings_advanced",
        settings(AppViewState {
            save_status: "✓ 已儲存".into(),
            ..base()
        }),
        FULL,
        tab(SettingsTab::Advanced),
    );
    s.shot_with(
        "settings_general_dark",
        dark(settings(base())),
        FULL,
        tab(SettingsTab::General),
    );
    s.shot_with(
        "overlay_settings_general",
        settings(expanded(base())),
        PANEL,
        tab(SettingsTab::General),
    );

    // 契約元件總覽
    s.gallery("widgets_gallery", false);
    s.gallery("widgets_gallery_dark", true);

    // 深色主視窗
    s.shot("dark_live_idle", dark(base()), FULL);
    s.shot(
        "dark_live_recording",
        dark(recording(with_text(base()))),
        FULL,
    );
    s.shot("dark_file", dark(file(base())), FULL);
    s.shot(
        "dark_error_log",
        dark(AppViewState {
            show_error_log: true,
            ..base()
        }),
        FULL,
    );

    let index = s.written.join("\n");
    std::fs::write(dir.join("index.txt"), &index).expect("write index");
    println!("{} snapshots written to {}", s.written.len(), dir.display());
}

#[test]
fn snapshot_png_encoder_roundtrip_shape() {
    let px = vec![0x0011_2233u32; 40 * 3];
    let png = encode_png(40, 3, &px);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert!(png.len() < 200, "flat image should compress: {}", png.len());
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
}
