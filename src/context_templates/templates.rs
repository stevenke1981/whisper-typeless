use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;

use super::TemplateId;
use crate::whisper::WhisperParams;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PostProcessSettings {
    pub add_punctuation: bool,
    pub formal_style: bool,
    pub remove_fillers: Vec<String>,
    pub preserve_english_terms: bool,
}

impl Default for PostProcessSettings {
    fn default() -> Self {
        Self {
            add_punctuation: true,
            formal_style: false,
            remove_fillers: vec!["嗯".into(), "啊".into(), "那個".into(), "就是說".into()],
            preserve_english_terms: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTemplate {
    pub id: TemplateId,
    pub initial_prompt: String,
    pub language_override: Option<String>,
    pub temperature_override: Option<f32>,
    pub postprocess: PostProcessSettings,
    pub keywords: Vec<String>,
}

impl ContextTemplate {
    pub fn apply_to_params(&self, params: &mut WhisperParams) {
        if !self.initial_prompt.is_empty() {
            params.initial_prompt = Some(self.initial_prompt.clone());
        }
        if let Some(lang) = &self.language_override {
            params.language = Some(lang.clone());
        }
        if let Some(temp) = self.temperature_override {
            params.temperature = temp;
        }
    }
}

/// TOML 檔的原始結構（id 為穩定英文 id）。
#[derive(Debug, Deserialize)]
struct TemplateToml {
    id: String,
    #[serde(default)]
    initial_prompt: String,
    #[serde(default)]
    language_override: Option<String>,
    #[serde(default)]
    temperature_override: Option<f32>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    postprocess: PostProcessSettings,
}

/// 內建模板（編譯期嵌入，單一 exe 即可運作）。
const BUILTIN_TOMLS: [&str; 5] = [
    include_str!("../../templates/meeting.toml"),
    include_str!("../../templates/casual.toml"),
    include_str!("../../templates/technical.toml"),
    include_str!("../../templates/medical.toml"),
    include_str!("../../templates/legal.toml"),
];

/// 解析單一 TOML 模板。
pub fn parse_template_toml(src: &str) -> Result<ContextTemplate, toml::de::Error> {
    let raw: TemplateToml = toml::from_str(src)?;
    Ok(ContextTemplate {
        id: TemplateId::from_stable_id(raw.id.trim()),
        initial_prompt: raw.initial_prompt,
        language_override: raw.language_override,
        temperature_override: raw.temperature_override,
        postprocess: raw.postprocess,
        keywords: raw.keywords,
    })
}

/// 以穩定 id（非本地化顯示名稱）為鍵的模板註冊表。
pub struct TemplateRegistry {
    templates: HashMap<String, ContextTemplate>,
}

impl TemplateRegistry {
    /// 內建 + <exe 目錄>/templates + <設定目錄>/templates（後者覆蓋前者）。
    pub fn new() -> Self {
        let mut dirs_list = Vec::new();
        if let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("templates")))
        {
            dirs_list.push(dir);
        }
        if let Some(dir) = dirs::config_dir() {
            dirs_list.push(dir.join("whisper-typeless").join("templates"));
        }
        Self::with_dirs(&dirs_list)
    }

    /// 僅內建模板。
    pub fn builtin_only() -> Self {
        Self::with_dirs(&[])
    }

    /// 內建模板後依序載入各目錄的 .toml；同 id 後者覆蓋前者。
    pub fn with_dirs(dirs: &[PathBuf]) -> Self {
        let mut reg = Self {
            templates: HashMap::new(),
        };
        reg.load_builtins();
        for dir in dirs {
            reg.load_dir(dir);
        }
        reg
    }

    fn load_builtins(&mut self) {
        for src in BUILTIN_TOMLS {
            match parse_template_toml(src) {
                Ok(t) => {
                    self.templates.insert(t.id.stable_id(), t);
                }
                Err(e) => warn!("內建模板解析失敗: {e}"),
            }
        }
    }

    fn load_dir(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("toml"))
            .collect();
        paths.sort();
        for path in paths {
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    warn!("讀取模板失敗 {}: {e}", path.display());
                    continue;
                }
            };
            match parse_template_toml(&text) {
                Ok(t) => {
                    self.templates.insert(t.id.stable_id(), t);
                }
                Err(e) => warn!("模板 TOML 無效，已略過 {}: {e}", path.display()),
            }
        }
    }

    pub fn get(&self, id: &TemplateId) -> Option<&ContextTemplate> {
        self.templates.get(&id.stable_id())
    }

    pub fn get_casual(&self) -> &ContextTemplate {
        self.get(&TemplateId::Casual)
            .expect("casual template must exist")
    }

    pub fn all(&self) -> Vec<&ContextTemplate> {
        let order = [
            TemplateId::Meeting,
            TemplateId::Casual,
            TemplateId::Technical,
            TemplateId::Medical,
            TemplateId::Legal,
        ];
        let mut out: Vec<&ContextTemplate> = order.iter().filter_map(|id| self.get(id)).collect();
        // 使用者額外模板接在內建之後，依 id 排序
        let mut extra: Vec<&ContextTemplate> = self
            .templates
            .values()
            .filter(|t| !order.contains(&t.id))
            .collect();
        extra.sort_by_key(|t| t.id.stable_id());
        out.extend(extra);
        out
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whisper::WhisperParams;

    #[test]
    fn templates_all_returns_ordered() {
        let reg = TemplateRegistry::new();
        let all = reg.all();
        assert_eq!(all.len(), 5);
        // Order should be: Meeting, Casual, Technical, Medical, Legal
        assert_eq!(all[0].id, TemplateId::Meeting);
        assert_eq!(all[1].id, TemplateId::Casual);
        assert_eq!(all[4].id, TemplateId::Legal);
    }

    #[test]
    fn templates_get_casual_exists() {
        let reg = TemplateRegistry::new();
        let t = reg.get_casual();
        assert_eq!(t.id, TemplateId::Casual);
        assert!(!t.initial_prompt.is_empty());
    }

    #[test]
    fn templates_apply_prompt_to_params() {
        let reg = TemplateRegistry::new();
        let t = reg.get(&TemplateId::Meeting).unwrap();
        let mut params = WhisperParams {
            initial_prompt: None,
            ..Default::default()
        };
        t.apply_to_params(&mut params);
        assert_eq!(
            params.initial_prompt.as_deref(),
            Some("這是一段商業會議的記錄，包含專業術語、人名和公司名稱。請準確轉錄所有內容。")
        );
    }

    #[test]
    fn templates_apply_language_override() {
        let reg = TemplateRegistry::new();
        let t = reg.get(&TemplateId::Meeting).unwrap();
        let mut params = WhisperParams {
            language: Some("en".into()),
            ..Default::default()
        };
        t.apply_to_params(&mut params);
        assert_eq!(params.language.as_deref(), Some("zh"));
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wt_tpl_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn embedded_tomls_all_parse() {
        for src in BUILTIN_TOMLS {
            let t = parse_template_toml(src).expect("embedded toml must parse");
            assert!(!t.initial_prompt.is_empty());
        }
    }

    #[test]
    fn all_five_ids_present() {
        let reg = TemplateRegistry::builtin_only();
        for id in [
            TemplateId::Meeting,
            TemplateId::Casual,
            TemplateId::Technical,
            TemplateId::Medical,
            TemplateId::Legal,
        ] {
            assert!(reg.get(&id).is_some(), "missing {:?}", id);
        }
    }

    #[test]
    fn registry_keyed_by_stable_id() {
        let reg = TemplateRegistry::builtin_only();
        assert!(reg.templates.contains_key("meeting"));
        assert!(!reg.templates.contains_key("會議記錄"));
    }

    #[test]
    fn stable_id_roundtrip() {
        for id in [
            TemplateId::Meeting,
            TemplateId::Legal,
            TemplateId::Custom("foo".into()),
        ] {
            assert_eq!(TemplateId::from_stable_id(&id.stable_id()), id);
        }
    }

    #[test]
    fn user_dir_overrides_builtin() {
        let dir = tmp_dir("ovr");
        std::fs::write(
            dir.join("meeting.toml"),
            "id = \"meeting\"\ninitial_prompt = \"custom meeting\"\n",
        )
        .unwrap();
        let reg = TemplateRegistry::with_dirs(std::slice::from_ref(&dir));
        let t = reg.get(&TemplateId::Meeting).unwrap();
        assert_eq!(t.initial_prompt, "custom meeting");
        assert_eq!(reg.all().len(), 5);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn later_dir_wins_over_earlier() {
        let a = tmp_dir("a");
        let b = tmp_dir("b");
        std::fs::write(a.join("x.toml"), "id = \"x\"\ninitial_prompt = \"A\"").unwrap();
        std::fs::write(b.join("x.toml"), "id = \"x\"\ninitial_prompt = \"B\"").unwrap();
        let reg = TemplateRegistry::with_dirs(&[a.clone(), b.clone()]);
        let t = reg.get(&TemplateId::Custom("x".into())).unwrap();
        assert_eq!(t.initial_prompt, "B");
        let _ = std::fs::remove_dir_all(a);
        let _ = std::fs::remove_dir_all(b);
    }

    #[test]
    fn user_dir_extends_with_new_template() {
        let dir = tmp_dir("ext");
        std::fs::write(
            dir.join("sales.toml"),
            "id = \"sales\"\ninitial_prompt = \"業務\"\nkeywords = [\"報價\"]",
        )
        .unwrap();
        let reg = TemplateRegistry::with_dirs(std::slice::from_ref(&dir));
        assert_eq!(reg.all().len(), 6);
        assert!(reg.get(&TemplateId::Custom("sales".into())).is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_toml_is_skipped() {
        let dir = tmp_dir("bad");
        std::fs::write(dir.join("bad.toml"), "this is = = not toml [[").unwrap();
        std::fs::write(dir.join("noid.toml"), "initial_prompt = \"no id\"").unwrap();
        std::fs::write(dir.join("note.txt"), "id = \"ignored\"").unwrap();
        let reg = TemplateRegistry::with_dirs(std::slice::from_ref(&dir));
        assert_eq!(reg.all().len(), 5);
        assert!(reg.get(&TemplateId::Custom("ignored".into())).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_dir_is_ignored() {
        let reg = TemplateRegistry::with_dirs(&[PathBuf::from("Z:/definitely/not/here")]);
        assert_eq!(reg.all().len(), 5);
    }
}
