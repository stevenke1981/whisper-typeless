//! 情境模板管理（載入／儲存 templates.json；UI 無關）。

use std::path::PathBuf;

use tokio::runtime::Handle as TokioHandle;

use crate::context_templates::templates::PostProcessSettings;
use crate::context_templates::{ContextTemplate, TemplateId, TemplateRegistry};
use crate::core::event::CoreEvent;
use crate::core::state::TemplateEntryView;
use crate::core::Notifier;

const BUILTIN_TEMPLATE_NAMES: &[&str] =
    &["會議記錄", "口語對話", "技術討論", "醫療紀錄", "法律文書"];

fn templates_json_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("templates.json")
}

pub(crate) async fn load_templates_from_disk() -> Vec<ContextTemplate> {
    let path = templates_json_path();
    if path.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            if let Ok(list) = serde_json::from_str::<Vec<ContextTemplate>>(&content) {
                return list;
            }
        }
    }
    let registry = TemplateRegistry::new();
    // 以穩定 id 為鍵的 registry；UI 仍以顯示名稱呈現
    registry.all().into_iter().cloned().collect()
}

async fn save_templates_to_disk(templates: &[ContextTemplate]) {
    let path = templates_json_path();
    if let Ok(json) = serde_json::to_string_pretty(templates) {
        let _ = tokio::fs::write(&path, json).await;
    }
}

pub(crate) fn templates_to_entries(templates: &[ContextTemplate]) -> Vec<TemplateEntryView> {
    templates
        .iter()
        .map(|t| {
            let name = t.id.display_name().to_string();
            let is_builtin = BUILTIN_TEMPLATE_NAMES.contains(&name.as_str());
            TemplateEntryView {
                name,
                prompt: t.initial_prompt.clone(),
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

/// 在清單中新增或修改模板（純函式）。內建模板只改提示詞、不改名。
pub(crate) fn upsert_template(
    templates: &mut Vec<ContextTemplate>,
    original: &str,
    name: String,
    prompt: String,
) {
    if let Some(t) = templates
        .iter_mut()
        .find(|t| t.id.display_name() == original)
    {
        if original != name && !BUILTIN_TEMPLATE_NAMES.contains(&original) {
            t.id = TemplateId::Custom(name);
        }
        t.initial_prompt = prompt;
    } else {
        templates.push(make_custom_template(name, prompt));
    }
}

pub(crate) fn spawn_save_template(
    tokio: &TokioHandle,
    notifier: Notifier,
    original: String,
    name: String,
    prompt: String,
) {
    tokio.spawn(async move {
        let mut templates = load_templates_from_disk().await;
        upsert_template(&mut templates, &original, name, prompt);
        save_templates_to_disk(&templates).await;
        notifier.send(CoreEvent::TemplatesUpdated {
            entries: templates_to_entries(&templates),
            status: Some("模板已儲存".into()),
        });
    });
}

pub(crate) fn spawn_delete_template(tokio: &TokioHandle, notifier: Notifier, name: String) {
    tokio.spawn(async move {
        let mut templates = load_templates_from_disk().await;
        templates.retain(|t| t.id.display_name() != name);
        save_templates_to_disk(&templates).await;
        notifier.send(CoreEvent::TemplatesUpdated {
            entries: templates_to_entries(&templates),
            status: None,
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtins() -> Vec<ContextTemplate> {
        TemplateRegistry::new().all().into_iter().cloned().collect()
    }

    #[test]
    fn builtin_entries_are_flagged() {
        let entries = templates_to_entries(&builtins());
        assert!(!entries.is_empty());
        assert!(entries.iter().any(|e| e.is_builtin));
    }

    #[test]
    fn upsert_adds_then_renames_custom() {
        let mut list = builtins();
        let n = list.len();
        upsert_template(&mut list, "新模板", "新模板".into(), "p1".into());
        assert_eq!(list.len(), n + 1);
        upsert_template(&mut list, "新模板", "改名".into(), "p2".into());
        assert_eq!(list.len(), n + 1);
        let e = templates_to_entries(&list);
        let renamed = e.iter().find(|e| e.name == "改名").expect("renamed");
        assert_eq!(renamed.prompt, "p2");
        assert!(!renamed.is_builtin);
    }

    #[test]
    fn builtin_keeps_name_on_upsert() {
        let mut list = builtins();
        let entries = templates_to_entries(&list);
        let Some(builtin) = entries.iter().find(|e| e.is_builtin) else {
            return;
        };
        let name = builtin.name.clone();
        upsert_template(&mut list, &name, "別名".into(), "新提示".into());
        let e = templates_to_entries(&list);
        assert!(e.iter().any(|e| e.name == name && e.prompt == "新提示"));
        assert!(!e.iter().any(|e| e.name == "別名"));
    }
}
