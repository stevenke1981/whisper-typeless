//! [`AppViewState`] ↔ `AppWindow` 屬性同步。

use slint::{ModelRc, SharedString, VecModel};

use crate::core::{AppViewState, ModelEntryView, TemplateEntryView, TranscriptSegment};
use crate::{AppWindow, ModelEntry, TemplateEditEntry, TranscriptSegmentData};

fn ss(v: &str) -> SharedString {
    v.into()
}

fn strings(v: &[String]) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = v.iter().map(|s| ss(s)).collect();
    ModelRc::new(VecModel::from(items))
}

fn floats(v: &[f32]) -> ModelRc<f32> {
    ModelRc::new(VecModel::from(v.to_vec()))
}

fn segments(v: &[TranscriptSegment]) -> ModelRc<TranscriptSegmentData> {
    let items: Vec<TranscriptSegmentData> = v
        .iter()
        .map(|s| TranscriptSegmentData {
            text: ss(&s.text),
            timestamp: ss(&s.timestamp),
        })
        .collect();
    ModelRc::new(VecModel::from(items))
}

fn model_entries(v: &[ModelEntryView]) -> ModelRc<ModelEntry> {
    let items: Vec<ModelEntry> = v
        .iter()
        .map(|m| ModelEntry {
            id: ss(&m.id),
            display_name: ss(&m.display_name),
            is_downloaded: m.is_downloaded,
            size_mb: m.size_mb,
            quality: m.quality,
            download_url: ss(&m.download_url),
        })
        .collect();
    ModelRc::new(VecModel::from(items))
}

fn template_entries(v: &[TemplateEntryView]) -> ModelRc<TemplateEditEntry> {
    let items: Vec<TemplateEditEntry> = v
        .iter()
        .map(|t| TemplateEditEntry {
            name: ss(&t.name),
            prompt: ss(&t.prompt),
            is_builtin: t.is_builtin,
        })
        .collect();
    ModelRc::new(VecModel::from(items))
}

/// 讀回使用者在 Slint 直接修改的屬性（雙向綁定 / Slint 內部指派）。
///
/// 只讀純量屬性；清單（轉錄、模型、模板、錯誤紀錄）一律由核心擁有。
pub(super) fn pull(ui: &AppWindow, s: &mut AppViewState) {
    s.floating_mode = ui.get_floating_mode();
    s.floating_expanded = ui.get_floating_expanded();
    s.show_error_log = ui.get_show_error_log();
    s.settings_open = ui.get_settings_open();
    s.dark_mode = ui.get_dark_mode();
    s.current_template = ui.get_current_template().to_string();
    s.selected_model_id = ui.get_selected_model_id().to_string();

    s.mod_vad = ui.get_mod_vad();
    s.mod_context = ui.get_mod_context();
    s.mod_opencc = ui.get_mod_opencc();
    s.mod_inject = ui.get_mod_inject();
    s.mod_waveform = ui.get_mod_waveform();
    s.mod_history = ui.get_mod_history();
    s.clipboard_enabled = ui.get_clipboard_enabled();
    s.inject_enabled = ui.get_inject_enabled();

    s.vad_threshold = ui.get_vad_threshold();
    s.silence_timeout = ui.get_silence_timeout();
    s.n_threads = ui.get_n_threads();
    s.use_gpu = ui.get_use_gpu();
    s.temperature = ui.get_temperature();
    s.conversion_mode = ui.get_conversion_mode().to_string();
    s.append_newline = ui.get_append_newline();
    s.privacy_mode = ui.get_privacy_mode();
    s.hotkey = ui.get_hotkey().to_string();
    s.ptt_hotkey = ui.get_ptt_hotkey().to_string();
    s.ptt_mode = ui.get_ptt_mode();
    s.save_status = ui.get_save_status().to_string();

    s.file_mode = ui.get_file_mode();
    s.file_name = ui.get_file_name().to_string();
    s.file_progress = ui.get_file_progress();
    s.file_processing = ui.get_file_processing();
    s.file_has_result = ui.get_file_has_result();
    s.file_result = ui.get_file_result().to_string();
    s.file_output_format = ui.get_file_output_format().to_string();
}

/// 把狀態推到 AppWindow；`last` 為上次推送的狀態（`None` = 全部推送）。
pub(super) fn push(ui: &AppWindow, s: &AppViewState, last: Option<&AppViewState>) {
    macro_rules! sync {
        ($($field:ident => $setter:ident($conv:expr)),* $(,)?) => {
            $(
                if last.map_or(true, |l| l.$field != s.$field) {
                    ui.$setter($conv(&s.$field));
                }
            )*
        };
    }
    let copy_b = |v: &bool| *v;
    let copy_f = |v: &f32| *v;
    let copy_i = |v: &i32| *v;
    let text = |v: &String| ss(v);

    sync! {
        app_title => set_app_title(text),
        floating_mode => set_floating_mode(copy_b),
        floating_expanded => set_floating_expanded(copy_b),
        show_error_log => set_show_error_log(copy_b),
        error_log => set_error_log(|v: &Vec<String>| strings(v)),
        settings_open => set_settings_open(copy_b),
        dark_mode => set_dark_mode(copy_b),

        is_recording => set_is_recording(copy_b),
        is_processing => set_is_processing(copy_b),
        current_template => set_current_template(text),
        current_partial => set_current_partial(text),
        transcript_segments => set_transcript_segments(|v: &Vec<TranscriptSegment>| segments(v)),
        audio_level => set_audio_level(copy_f),
        waveform_bars => set_waveform_bars(|v: &Vec<f32>| floats(v)),
        status_text => set_status_text(text),
        char_count => set_char_count(copy_i),
        device_label => set_device_label(text),

        model_name => set_model_name(text),
        model_entries => set_model_entries(|v: &Vec<ModelEntryView>| model_entries(v)),
        selected_model_id => set_selected_model_id(text),
        downloading_id => set_downloading_id(text),
        download_progress => set_download_progress(copy_f),

        mod_vad => set_mod_vad(copy_b),
        mod_context => set_mod_context(copy_b),
        mod_opencc => set_mod_opencc(copy_b),
        mod_inject => set_mod_inject(copy_b),
        mod_waveform => set_mod_waveform(copy_b),
        mod_history => set_mod_history(copy_b),
        clipboard_enabled => set_clipboard_enabled(copy_b),
        inject_enabled => set_inject_enabled(copy_b),

        vad_threshold => set_vad_threshold(copy_f),
        silence_timeout => set_silence_timeout(copy_f),
        n_threads => set_n_threads(copy_i),
        use_gpu => set_use_gpu(copy_b),
        temperature => set_temperature(copy_f),
        conversion_mode => set_conversion_mode(text),
        append_newline => set_append_newline(copy_b),
        privacy_mode => set_privacy_mode(copy_b),
        hotkey => set_hotkey(text),
        ptt_hotkey => set_ptt_hotkey(text),
        ptt_mode => set_ptt_mode(copy_b),
        save_status => set_save_status(text),

        file_mode => set_file_mode(copy_b),
        file_name => set_file_name(text),
        file_progress => set_file_progress(copy_f),
        file_processing => set_file_processing(copy_b),
        file_has_result => set_file_has_result(copy_b),
        file_result => set_file_result(text),
        file_output_format => set_file_output_format(text),

        template_entries => set_template_entries(|v: &Vec<TemplateEntryView>| template_entries(v)),
    }
}
