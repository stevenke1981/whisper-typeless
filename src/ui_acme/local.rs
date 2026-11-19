//! 只屬於畫面層、不進核心的狀態（捲動位置、文字框焦點、模板編輯選取、置頂…）。

use super::controls::{set_text, Fields};
use super::msg::{FieldId, ScrollId};
use crate::core::{AppViewState, TemplateEntryView};

/// 正在編輯的模板。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateEdit {
    /// 清單中的索引；`None` = 新增。
    pub index: Option<usize>,
    /// 原始名稱（新增時為空字串），送 `TemplateSave` / `TemplateDelete` 用。
    pub original: String,
    pub builtin: bool,
}

/// 畫面層本地狀態。
#[derive(Clone, Debug)]
pub struct UiLocal {
    /// 浮動視窗是否置頂（預設是）。
    pub pinned: bool,
    /// 各捲動區的位移（由上往下，邏輯像素）。
    pub scroll: [f32; 4],
    /// 轉錄紀錄是否黏在最底部（新段落自動捲到底）。
    pub follow_transcript: bool,
    /// 取得焦點的文字框。
    pub focus: Option<FieldId>,
    pub fields: Fields,
    pub template_edit: Option<TemplateEdit>,
    /// 動畫影格（錄音中的脈動）。
    pub anim: u32,
}

impl Default for UiLocal {
    fn default() -> Self {
        Self {
            pinned: true,
            scroll: [0.0; 4],
            follow_transcript: true,
            focus: None,
            fields: Fields::new(),
            template_edit: None,
            anim: 0,
        }
    }
}

impl UiLocal {
    pub fn scroll(&self, id: ScrollId) -> f32 {
        self.scroll[id.index()]
    }

    pub fn set_scroll(&mut self, id: ScrollId, value: f32) {
        self.scroll[id.index()] = value.max(0.0);
    }

    pub fn text_focused(&self) -> bool {
        self.focus.is_some()
    }

    /// 沒有焦點的熱鍵欄位跟著核心狀態走（例如載入設定後）。
    pub fn sync_fields(&mut self, s: &AppViewState) {
        for (id, value) in [
            (FieldId::Hotkey, &s.hotkey),
            (FieldId::PttHotkey, &s.ptt_hotkey),
        ] {
            if self.focus != Some(id) && self.fields.get(id).text != *value {
                set_text(self.fields.get_mut(id), value);
            }
        }
        // 模板清單變動後，選取索引可能超出範圍。
        if let Some(edit) = &mut self.template_edit {
            if let Some(i) = edit.index {
                match s.template_entries.get(i) {
                    Some(t) if t.name == edit.original => {}
                    _ => {
                        edit.index = s
                            .template_entries
                            .iter()
                            .position(|t| t.name == edit.original);
                    }
                }
            }
        }
    }

    /// 開始編輯既有模板。
    pub fn edit_template(&mut self, index: usize, entry: &TemplateEntryView) {
        self.template_edit = Some(TemplateEdit {
            index: Some(index),
            original: entry.name.clone(),
            builtin: entry.is_builtin,
        });
        set_text(&mut self.fields.template_name, &entry.name);
        set_text(&mut self.fields.template_prompt, &entry.prompt);
        self.fields.template_name.readonly = entry.is_builtin;
    }

    /// 開始新增模板。
    pub fn new_template(&mut self) {
        self.template_edit = Some(TemplateEdit {
            index: None,
            original: String::new(),
            builtin: false,
        });
        set_text(&mut self.fields.template_name, "新模板");
        set_text(&mut self.fields.template_prompt, "");
        self.fields.template_name.readonly = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_fields_follow_core_unless_focused() {
        let mut ui = UiLocal::default();
        let mut s = AppViewState::default();
        ui.sync_fields(&s);
        assert_eq!(ui.fields.hotkey.text, "Ctrl+Shift+W");
        ui.focus = Some(FieldId::Hotkey);
        s.hotkey = "F8".into();
        ui.sync_fields(&s);
        assert_eq!(ui.fields.hotkey.text, "Ctrl+Shift+W");
        ui.focus = None;
        ui.sync_fields(&s);
        assert_eq!(ui.fields.hotkey.text, "F8");
    }

    #[test]
    fn template_edit_tracks_renamed_index() {
        let mut ui = UiLocal::default();
        let entries = [
            TemplateEntryView {
                name: "會議".into(),
                prompt: "p".into(),
                is_builtin: true,
            },
            TemplateEntryView {
                name: "我的".into(),
                prompt: "q".into(),
                is_builtin: false,
            },
        ];
        ui.edit_template(1, &entries[1]);
        assert_eq!(ui.fields.template_prompt.text, "q");
        assert!(!ui.fields.template_name.readonly);
        let s = AppViewState {
            template_entries: vec![entries[1].clone()],
            ..AppViewState::default()
        };
        ui.sync_fields(&s);
        assert_eq!(ui.template_edit.as_ref().unwrap().index, Some(0));
        ui.edit_template(0, &entries[0]);
        assert!(ui.fields.template_name.readonly);
        ui.new_template();
        assert_eq!(ui.fields.template_name.text, "新模板");
        assert_eq!(ui.template_edit.as_ref().unwrap().index, None);
    }
}
