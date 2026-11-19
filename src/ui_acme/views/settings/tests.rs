//! 設定面板（六個分頁）的描述樹測試：不需要字型以外的任何資源。

use super::*;
use crate::core::{AppViewState, ModelEntryView, TemplateEntryView};
use crate::ui_acme::controls::set_text;
use crate::ui_acme::local::UiLocal;
use crate::ui_acme::msg::{FieldId, ModKey, SliderKind};
use crate::ui_acme::paint::Item;
use crate::ui_acme::text::TextEngine;
use crate::ui_acme::theme::Theme;

fn build_sized(s: &AppViewState, ui: &UiLocal, w: f32, h: f32) -> View {
    let mut text = TextEngine::new();
    let mut cx = Ctx {
        s,
        ui,
        t: Theme::for_mode(s.dark_mode),
        text: &mut text,
        scale: 1.0,
    };
    let mut v = View::new();
    panel(&mut v, &mut cx, r(0.0, 0.0, w, h));
    v
}

fn build(s: &AppViewState, tab: SettingsTab) -> View {
    build_sized(s, &on_tab(tab), 560.0, 620.0)
}

fn on_tab(tab: SettingsTab) -> UiLocal {
    UiLocal {
        settings_tab: tab,
        ..UiLocal::default()
    }
}

fn texts(v: &View) -> Vec<String> {
    v.items
        .iter()
        .filter_map(|i| match i {
            Item::Text { content, .. } | Item::Wrapped { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect()
}

fn actions(v: &View) -> Vec<Action> {
    v.items
        .iter()
        .filter_map(|i| match i {
            Item::Hit { action, .. } => Some(action.clone()),
            _ => None,
        })
        .collect()
}

fn has_click(v: &View, msg: Msg) -> bool {
    actions(v).contains(&Action::Click(msg))
}

fn has_field(v: &View, id: FieldId) -> bool {
    actions(v)
        .iter()
        .any(|a| matches!(a, Action::Field { id: f, .. } if *f == id))
}

fn models() -> Vec<ModelEntryView> {
    vec![
        ModelEntryView {
            id: "small".into(),
            display_name: "Small".into(),
            is_downloaded: true,
            size_mb: 466,
            quality: 3,
            download_url: "https://huggingface.co/ggml-small.bin".into(),
        },
        ModelEntryView {
            id: "base".into(),
            display_name: "Base".into(),
            is_downloaded: true,
            size_mb: 142,
            quality: 2,
            download_url: "https://example.invalid/base.bin".into(),
        },
        ModelEntryView {
            id: "large-v3".into(),
            display_name: "Large v3".into(),
            is_downloaded: false,
            size_mb: 3100,
            quality: 5,
            download_url: "https://example.invalid/large.bin".into(),
        },
    ]
}

fn templates() -> Vec<TemplateEntryView> {
    vec![
        TemplateEntryView {
            name: "會議".into(),
            prompt: "以下是會議紀錄。".into(),
            is_builtin: true,
        },
        TemplateEntryView {
            name: "我的模板".into(),
            prompt: "自訂提示".into(),
            is_builtin: false,
        },
    ]
}

fn rich() -> AppViewState {
    AppViewState {
        model_entries: models(),
        template_entries: templates(),
        selected_model_id: "small".into(),
        error_log: vec!["10:01 錯誤".into()],
        save_status: "✓ 已儲存".into(),
        ..AppViewState::default()
    }
}

fn openai() -> AppViewState {
    AppViewState {
        engine: "openai".into(),
        ..rich()
    }
}

fn settings_scroll_max(v: &View) -> f32 {
    v.scrolls
        .iter()
        .find(|a| a.id == ScrollId::Settings)
        .map(|a| a.max)
        .expect("settings scroll area")
}

// ─── 框架：分頁列、標題列 ─────────────────────────────────────────────────────

#[test]
fn every_tab_shows_tab_bar_with_current_selected() {
    for tab in SettingsTab::ALL {
        let v = build(&rich(), tab);
        let all = texts(&v);
        for other in SettingsTab::ALL {
            assert!(
                all.iter().any(|t| t == other.label()),
                "{tab:?} 少了 {other:?}"
            );
            // 目前分頁不可再點，其他分頁都可點
            assert_eq!(
                has_click(&v, Msg::SettingsTab(other)),
                other != tab,
                "{tab:?} / {other:?}"
            );
        }
        assert!(has_click(&v, Msg::Core(UiCommand::SetSettingsOpen(false))));
    }
}

#[test]
fn no_save_button_module_section_or_urls_anywhere() {
    for state in [rich(), openai()] {
        for tab in SettingsTab::ALL {
            let mut ui = on_tab(tab);
            ui.edit_template(1, &templates()[1]);
            let v = build_sized(&state, &ui, 560.0, 620.0);
            for t in texts(&v) {
                assert!(!t.contains("儲存設定"), "{tab:?}: {t}");
                assert!(!t.contains("功能模組"), "{tab:?}: {t}");
                assert!(!t.contains("://"), "{tab:?}: {t}");
            }
            assert!(!has_click(&v, Msg::Core(UiCommand::SaveSettings)));
            // 不再顯示的模組開關（欄位保留、不改值）
            for key in [ModKey::Vad, ModKey::Inject, ModKey::History] {
                for on in [true, false] {
                    assert!(!has_click(&v, Msg::Setting(SettingEdit::Module(key, on))));
                }
            }
        }
    }
}

#[test]
fn save_status_pill_follows_core_status() {
    assert_eq!(save_pill(""), None);
    let (label, tone, msg) = save_pill("✓ 已儲存").unwrap();
    assert_eq!((label, tone, msg), ("已自動儲存", Tone::Success, None));
    let (label, tone, msg) = save_pill("✗ 儲存失敗").unwrap();
    assert_eq!(label, "儲存失敗，請查看錯誤紀錄");
    assert_eq!(tone, Tone::Danger);
    assert_eq!(msg, Some(Msg::ToggleErrorLog));

    let v = build(&rich(), SettingsTab::General);
    assert!(texts(&v).iter().any(|t| t == "已自動儲存"));
}

#[test]
fn error_log_opened_from_settings_is_drawn_on_top() {
    let s = AppViewState {
        show_error_log: true,
        ..rich()
    };
    let v = build(&s, SettingsTab::Advanced);
    let z_of = |pred: &dyn Fn(&Action) -> bool| {
        v.items
            .iter()
            .filter_map(|i| match i {
                Item::Hit { action, z, .. } if pred(action) => Some(*z),
                _ => None,
            })
            .max()
    };
    let panel_z = z_of(&|a| *a == Action::Click(Msg::ToggleErrorLog)).unwrap();
    let log_z =
        z_of(&|a| *a == Action::Click(Msg::Core(UiCommand::ClearErrorLog))).expect("錯誤紀錄浮層");
    assert!(log_z > panel_z);
}

// ─── 一般 ───────────────────────────────────────────────────────────────────

#[test]
fn general_tab_has_hotkeys_window_and_appearance() {
    let s = rich();
    let v = build(&s, SettingsTab::General);
    assert!(has_field(&v, FieldId::Hotkey));
    assert!(has_field(&v, FieldId::PttHotkey));
    assert!(has_click(&v, Msg::Setting(SettingEdit::PttMode(false))));
    assert!(has_click(
        &v,
        Msg::Setting(SettingEdit::CloseToTray(!s.close_to_tray))
    ));
    assert!(has_click(
        &v,
        Msg::Setting(SettingEdit::MinimizeToTray(!s.minimize_to_tray))
    ));
    assert!(has_click(
        &v,
        Msg::Setting(SettingEdit::StartMinimized(!s.start_minimized))
    ));
    assert!(has_click(&v, Msg::Core(UiCommand::SetDarkMode(true))));
    assert!(!has_click(&v, Msg::Core(UiCommand::SetDarkMode(false))));
    assert!(has_click(
        &v,
        Msg::Setting(SettingEdit::Module(ModKey::Waveform, !s.mod_waveform))
    ));
    assert!(has_click(
        &v,
        Msg::Setting(SettingEdit::Module(ModKey::Context, !s.mod_context))
    ));
}

#[test]
fn ptt_hotkey_field_is_disabled_without_ptt_mode() {
    let s = AppViewState {
        ptt_mode: false,
        ..rich()
    };
    let mut ui = on_tab(SettingsTab::General);
    set_text(&mut ui.fields.ptt_hotkey, "Ctrl+Alt+L");
    let v = build_sized(&s, &ui, 560.0, 620.0);
    assert!(has_field(&v, FieldId::Hotkey));
    assert!(!has_field(&v, FieldId::PttHotkey));
    assert!(
        texts(&v).iter().any(|t| t == "Ctrl+Alt+L"),
        "停用時仍顯示目前值"
    );
}

#[test]
fn window_toggles_flip_only_their_own_flag() {
    let toggles = general::window_toggles(true, false, false);
    assert_eq!(toggles[0].2, Msg::Setting(SettingEdit::CloseToTray(false)));
    assert_eq!(
        toggles[1].2,
        Msg::Setting(SettingEdit::MinimizeToTray(true))
    );
    assert_eq!(
        toggles[2].2,
        Msg::Setting(SettingEdit::StartMinimized(true))
    );
    assert!(toggles.iter().all(|(label, _, _)| label.contains("系統匣")));
}

// ─── 辨識 ───────────────────────────────────────────────────────────────────

#[test]
fn local_engine_shows_engine_choice_and_privacy_only() {
    let v = build(&rich(), SettingsTab::Engine);
    assert!(has_click(
        &v,
        Msg::Setting(SettingEdit::Engine("openai".into()))
    ));
    assert!(has_click(&v, Msg::Core(UiCommand::SetPrivacyMode(true))));
    assert!(!has_field(&v, FieldId::OpenAiKey));
    assert!(!texts(&v).iter().any(|t| t == "我同意把語音傳送到 OpenAI"));
    assert!(!has_click(
        &v,
        Msg::Setting(SettingEdit::OpenAiConsent(true))
    ));
}

#[test]
fn privacy_mode_disables_engine_choice_with_explanation() {
    let s = AppViewState {
        privacy_mode: true,
        ..openai()
    };
    let v = build(&s, SettingsTab::Engine);
    assert!(!has_click(
        &v,
        Msg::Setting(SettingEdit::Engine("local".into()))
    ));
    assert!(texts(&v).iter().any(|t| t.contains("隱私模式開啟中")));
    assert!(has_click(&v, Msg::Core(UiCommand::SetPrivacyMode(false))));
}

#[test]
fn privacy_mode_greys_out_the_cloud_card() {
    let mut ui = on_tab(SettingsTab::Engine);
    set_text(&mut ui.fields.openai_key, "sk-secret-xyz");
    let s = AppViewState {
        privacy_mode: true,
        openai_consent: true,
        openai_key_set: true,
        ..openai()
    };
    let v = build_sized(&s, &ui, 560.0, 620.0);
    assert!(texts(&v).iter().any(|t| t == "我同意把語音傳送到 OpenAI"));
    assert!(!has_click(
        &v,
        Msg::Setting(SettingEdit::OpenAiConsent(false))
    ));
    assert!(!has_field(&v, FieldId::OpenAiKey));
    assert!(!has_click(&v, Msg::OpenAiKeySave));
    assert!(!has_click(&v, Msg::Core(UiCommand::ClearOpenAiKey)));
    assert!(!has_click(
        &v,
        Msg::Setting(SettingEdit::OpenAiModel("gpt-transcribe".into()))
    ));
    let t = Theme::for_mode(false);
    let amber = v.items.iter().any(|i| {
        matches!(i, Item::Rect { fill, border: Some((_, line)), .. }
            if *fill == t.warning_soft && *line == t.warning_line)
    });
    assert!(!amber, "隱私模式下雲端卡片不用琥珀色");
    // 關掉隱私模式就恢復可操作的琥珀卡片。
    let v = build_sized(
        &AppViewState {
            privacy_mode: false,
            ..s
        },
        &ui,
        560.0,
        620.0,
    );
    assert!(has_field(&v, FieldId::OpenAiKey));
    assert!(has_click(&v, Msg::Core(UiCommand::ClearOpenAiKey)));
}

#[test]
fn openai_engine_shows_consent_key_and_model_in_order() {
    let mut ui = on_tab(SettingsTab::Engine);
    set_text(&mut ui.fields.openai_key, "sk-secret-xyz");
    let v = build_sized(&openai(), &ui, 560.0, 620.0);
    let acts = actions(&v);
    let pos = |a: &Action| acts.iter().position(|x| x == a).unwrap();
    let consent = pos(&Action::Click(Msg::Setting(SettingEdit::OpenAiConsent(
        true,
    ))));
    let save = pos(&Action::Click(Msg::OpenAiKeySave));
    let model = pos(&Action::Click(Msg::Setting(SettingEdit::OpenAiModel(
        "gpt-transcribe".into(),
    ))));
    assert!(consent < save && save < model);
    // 未設定金鑰：紅色「未設定」、沒有「移除金鑰」
    assert!(!has_click(&v, Msg::Core(UiCommand::ClearOpenAiKey)));
    let all = texts(&v);
    assert!(all.iter().any(|t| t == "未設定"));
    assert!(!all.iter().any(|t| t == "移除金鑰"));
    // 金鑰欄位只畫遮蔽字元
    assert!(!all.iter().any(|t| t.contains("sk-secret")));
    assert!(all.iter().any(|t| t == &"•".repeat("sk-secret-xyz".len())));
}

#[test]
fn key_save_needs_input_and_key_set_offers_removal() {
    let s = AppViewState {
        openai_key_set: true,
        openai_consent: true,
        ..openai()
    };
    let v = build(&s, SettingsTab::Engine);
    assert!(has_click(&v, Msg::Core(UiCommand::ClearOpenAiKey)));
    assert!(!has_click(&v, Msg::OpenAiKeySave), "空白欄位不能儲存");
    let all = texts(&v);
    assert!(all.iter().any(|t| t == "已設定"));
    assert!(all.iter().any(|t| t == "移除金鑰"));
}

// ─── 模型 ───────────────────────────────────────────────────────────────────

#[test]
fn models_tab_lists_models_without_urls() {
    let v = build(&rich(), SettingsTab::Models);
    let all = texts(&v);
    assert!(all.iter().any(|t| t == "使用中"));
    assert!(all.iter().any(|t| t == "準確度 ★★★☆☆ · 466 MB"));
    assert!(!all.iter().any(|t| t.contains("http")));
    // 使用中不可再點；其他：已下載 =「使用」、未下載 =「下載」
    let pick = |id: &str| Msg::Core(UiCommand::ModelSelectOrDownload(id.into()));
    assert!(!has_click(&v, pick("small")));
    assert!(has_click(&v, pick("base")));
    assert!(has_click(&v, pick("large-v3")));
    assert!(all.iter().any(|t| t == "使用") && all.iter().any(|t| t == "下載"));
    assert!(has_click(&v, Msg::Core(UiCommand::OpenModelManager)));
}

#[test]
fn downloading_model_shows_progress_and_blocks_other_picks() {
    let s = AppViewState {
        downloading_id: "large-v3".into(),
        download_progress: 0.42,
        ..rich()
    };
    let v = build(&s, SettingsTab::Models);
    assert!(texts(&v).iter().any(|t| t == "42%"));
    assert!(!actions(&v).iter().any(|a| matches!(
        a,
        Action::Click(Msg::Core(UiCommand::ModelSelectOrDownload(_)))
    )));
}

#[test]
fn gpu_toggle_is_disabled_and_off_without_gpu_backend() {
    let (title, hint, on, msg) = models::gpu_row(None, true);
    assert_eq!(title, "使用 GPU 加速");
    assert!(hint.contains("此版本不含 GPU"));
    assert!(!on && msg.is_none());
    let (title, _, on, msg) = models::gpu_row(Some("Vulkan"), true);
    assert_eq!(title, "使用 GPU 加速（Vulkan）");
    assert!(on);
    assert_eq!(msg, Some(Msg::Setting(SettingEdit::UseGpu(false))));
    // 實際建置：依編入的後端決定
    let v = build(&rich(), SettingsTab::Models);
    let gpu_enabled = crate::whisper::gpu_backend_name().is_some();
    assert_eq!(
        actions(&v)
            .iter()
            .any(|a| matches!(a, Action::Click(Msg::Setting(SettingEdit::UseGpu(_))))),
        gpu_enabled
    );
}

#[test]
fn model_detail_clamps_quality() {
    let mut m = models()[0].clone();
    m.quality = 9;
    assert_eq!(models::model_detail(&m), "準確度 ★★★★★ · 466 MB");
    m.quality = -1;
    assert_eq!(models::model_detail(&m), "準確度 ☆☆☆☆☆ · 466 MB");
}

// ─── 輸出 ───────────────────────────────────────────────────────────────────

#[test]
fn chinese_output_selection_tracks_opencc_module() {
    let mut s = rich();
    assert_eq!(output::chinese_output(&s), "zh-TW");
    // 模組開著：只改轉換模式
    assert_eq!(
        output::chinese_output_msg(&s, "zh-HK"),
        Msg::Setting(SettingEdit::ConversionMode("zh-HK".into()))
    );
    // 選「不轉換」：轉換模式與模組一起改
    match output::chinese_output_msg(&s, "raw") {
        Msg::Core(UiCommand::UpdateSettings(u)) => {
            assert_eq!(u.conversion_mode, "raw");
            assert!(!u.mod_opencc);
        }
        other => panic!("{other:?}"),
    }
    s.mod_opencc = false;
    assert_eq!(output::chinese_output(&s), "raw");
    match output::chinese_output_msg(&s, "zh-CN") {
        Msg::Core(UiCommand::UpdateSettings(u)) => {
            assert_eq!(u.conversion_mode, "zh-CN");
            assert!(u.mod_opencc);
            assert_eq!(u.hotkey, s.hotkey, "其他欄位不變");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn output_tab_has_text_and_delivery_options() {
    let s = rich();
    let v = build(&s, SettingsTab::Output);
    assert!(has_click(
        &v,
        Msg::Setting(SettingEdit::ConversionMode("zh-HK".into()))
    ));
    assert!(has_click(
        &v,
        Msg::Setting(SettingEdit::AppendNewline(!s.append_newline))
    ));
    assert!(has_click(
        &v,
        Msg::Core(UiCommand::SetInjectEnabled(!s.inject_enabled))
    ));
    assert!(has_click(
        &v,
        Msg::Core(UiCommand::SetClipboardEnabled(!s.clipboard_enabled))
    ));
    let all = texts(&v);
    for label in ["台灣繁體", "香港繁體", "簡體", "不轉換"] {
        assert!(all.iter().any(|t| t == label), "{label}");
    }
}

// ─── 模板 ───────────────────────────────────────────────────────────────────

#[test]
fn templates_tab_lists_and_edits() {
    let s = rich();
    let v = build(&s, SettingsTab::Templates);
    assert!(has_click(&v, Msg::TemplateEditSelect(0)));
    assert!(has_click(&v, Msg::TemplateEditSelect(1)));
    assert!(has_click(&v, Msg::TemplateEditNew));
    assert!(texts(&v).iter().any(|t| t == "內建"));
    assert!(!has_field(&v, FieldId::TemplateName), "未選取時沒有編輯卡");

    let mut ui = on_tab(SettingsTab::Templates);
    ui.edit_template(1, &templates()[1]);
    let v = build_sized(&s, &ui, 560.0, 620.0);
    assert!(has_field(&v, FieldId::TemplateName));
    assert!(has_field(&v, FieldId::TemplatePrompt));
    assert!(texts(&v).iter().any(|t| t == "4 字"), "提示詞字數");
    assert!(has_click(&v, Msg::TemplateEditSave));
    assert!(has_click(&v, Msg::TemplateEditDelete));

    ui.edit_template(0, &templates()[0]);
    let v = build_sized(&s, &ui, 560.0, 620.0);
    assert!(has_click(&v, Msg::TemplateEditSave));
    assert!(!has_click(&v, Msg::TemplateEditDelete), "內建模板不能刪除");
}

// ─── 進階 ───────────────────────────────────────────────────────────────────

#[test]
fn advanced_tab_has_sliders_steppers_and_error_log() {
    let s = rich();
    let v = build(&s, SettingsTab::Advanced);
    let sliders: Vec<SliderKind> = actions(&v)
        .iter()
        .filter_map(|a| match a {
            Action::Slider(spec) => Some(spec.kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        sliders,
        vec![SliderKind::VadThreshold, SliderKind::Temperature]
    );
    assert!(has_click(&v, Msg::Setting(SettingEdit::Threads(5))));
    assert!(has_click(
        &v,
        Msg::Setting(SettingEdit::SilenceTimeout(1600.0))
    ));
    assert!(has_click(&v, Msg::ToggleErrorLog));
    let all = texts(&v);
    assert!(all.iter().any(|t| t == "1.5 秒"));
    assert!(all.iter().any(|t| t == "共 1 筆"));
    assert_eq!(advanced::seconds_label(300.0), "0.3 秒");
}

// ─── 版面 ───────────────────────────────────────────────────────────────────

#[test]
fn short_tabs_fit_without_scrolling_and_long_ones_scroll() {
    if !TextEngine::new().has_fonts() {
        return;
    }
    for tab in [
        SettingsTab::General,
        SettingsTab::Engine,
        SettingsTab::Output,
        SettingsTab::Advanced,
    ] {
        let v = build(&rich(), tab);
        assert_eq!(settings_scroll_max(&v), 0.0, "{tab:?} 不應需要捲動");
    }
    // 浮動面板（380×540）的一般頁內容較長，可以捲動
    let v = build_sized(&rich(), &on_tab(SettingsTab::General), 380.0, 540.0);
    assert!(settings_scroll_max(&v) > 0.0);
}

#[test]
fn narrow_panel_keeps_controls_inside_the_window() {
    for tab in SettingsTab::ALL {
        for state in [rich(), openai()] {
            let v = build_sized(&state, &on_tab(tab), 380.0, 540.0);
            for item in &v.items {
                if let Item::Hit { rect, action, .. } = item {
                    if matches!(action, Action::Block | Action::DragMove) {
                        continue;
                    }
                    let right = rect.origin.x.get() + rect.size.width.get();
                    assert!(
                        right <= 380.0 - SP3 + 0.5,
                        "{tab:?} {action:?} 超出右緣：{right}"
                    );
                    assert!(rect.origin.x.get() >= SP3, "{tab:?} {action:?}");
                }
            }
        }
    }
}
