// ─── 情境模板管理（載入／儲存／UI 回呼）─────────────────────────────────────

use std::path::PathBuf;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::context_templates::templates::PostProcessSettings;
use crate::context_templates::{ContextTemplate, TemplateId, TemplateRegistry};
use crate::{AppWindow, TemplateEditEntry};

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

pub async fn setup_template_callbacks(ui: &AppWindow) {
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
