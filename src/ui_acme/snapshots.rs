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

fn base() -> AppViewState {
    AppViewState {
        model_entries: models(),
        template_entries: templates(),
        current_template: "會議".into(),
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

    // 浮動列
    s.shot("overlay_idle", float(base()), BAR);
    s.shot("overlay_recording", float(recording(base())), BAR);
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
    s.shot(
        "overlay_error_log",
        float(AppViewState {
            show_error_log: true,
            ..base()
        }),
        BAR,
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
    s.shot(
        "live_processing",
        AppViewState {
            is_processing: true,
            status_text: "辨識中".into(),
            ..with_text(base())
        },
        FULL,
    );
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
    s.shot(
        "live_ptt_off_hotkey",
        AppViewState {
            ptt_mode: false,
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

    // 設定：由上到下
    s.settings_series("settings_light", settings(base()), FULL, |_| {});
    s.settings_series(
        "settings_cloud_consent",
        settings(AppViewState {
            engine: "openai".into(),
            openai_consent: true,
            openai_key_set: true,
            ..base()
        }),
        FULL,
        |_| {},
    );
    s.settings_series("settings_dark", dark(settings(base())), FULL, |_| {});
    s.settings_series(
        "settings_tray_gpu",
        settings(AppViewState {
            close_to_tray: true,
            minimize_to_tray: true,
            start_minimized: true,
            use_gpu: true,
            device_label: "Vulkan".into(),
            save_status: "✓ 已儲存".into(),
            ..base()
        }),
        FULL,
        |_| {},
    );
    // 模板編輯器（選到自訂模板，並捲到底）
    s.settings_series("settings_template_editor", settings(base()), FULL, |app| {
        app.ui.edit_template(1, &templates()[1]);
    });
    s.shot_with(
        "settings_template_editor_builtin",
        settings(base()),
        FULL,
        |app| {
            app.ui.edit_template(0, &templates()[0]);
            app.ui.set_scroll(ScrollId::Settings, 100000.0);
        },
    );
    s.shot_with(
        "settings_template_editor_new",
        settings(base()),
        FULL,
        |app| {
            app.ui.new_template();
            app.ui.set_scroll(ScrollId::Settings, 100000.0);
        },
    );
    // 浮動面板內的設定
    s.shot("overlay_settings", settings(expanded(base())), PANEL);
    s.shot(
        "overlay_settings_dark",
        dark(settings(expanded(base()))),
        PANEL,
    );

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
