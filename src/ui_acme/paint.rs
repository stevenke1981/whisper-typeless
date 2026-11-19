//! 檢視描述樹、繪製與命中測試 → 訊息路由。
//!
//! 檢視（view）是「狀態 → `View<M>`」的純函式；`paint_view` 把描述樹同時轉成
//! `Painter` 指令與 `HitTestLayer` 區域，並回傳 `HitMap`（節點 id → `Action`）。
//! 指標事件經 `ClickDetector` 產生 `PointerOutcome`，再由 `route` 轉回 `Action`。

use acme_core::{
    Color, CursorHint, HitRegionSpec, HitTestLayer, Logical, NodeId, Painter, PointerOutcome, Rect,
};

use super::text::{Align, TextEngine};

/// 命中區域被啟動時要做的事。
#[derive(Clone, Debug, PartialEq)]
pub enum Action<M> {
    /// 送出應用程式訊息（點擊時）。
    Message(M),
    /// 開始以作業系統拖曳移動視窗（按下時）。
    DragMove,
}

/// 檢視描述樹的一個元素。後面的元素畫在前面元素之上。
#[derive(Clone, Debug)]
pub enum Item<M> {
    Rect {
        rect: Rect<Logical>,
        radius: f32,
        fill: Color,
        border: Option<(f32, Color)>,
    },
    Text {
        rect: Rect<Logical>,
        content: String,
        size: f32,
        bold: bool,
        color: Color,
        align: Align,
    },
    /// 不可見的命中區域；`hover` 有值時，滑鼠移入會疊上該顏色。
    Hit {
        rect: Rect<Logical>,
        radius: f32,
        action: Action<M>,
        cursor: CursorHint,
        hover: Option<Color>,
        /// 數字越大越優先（按鈕要壓過拖曳區）。
        z: i32,
    },
}

/// 檢視描述樹。
#[derive(Clone, Debug)]
pub struct View<M> {
    pub items: Vec<Item<M>>,
}

impl<M> Default for View<M> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<M> View<M> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rect(&mut self, rect: Rect<Logical>, radius: f32, fill: Color) -> &mut Self {
        self.items.push(Item::Rect {
            rect,
            radius,
            fill,
            border: None,
        });
        self
    }

    pub fn bordered(
        &mut self,
        rect: Rect<Logical>,
        radius: f32,
        fill: Color,
        width: f32,
        border: Color,
    ) -> &mut Self {
        self.items.push(Item::Rect {
            rect,
            radius,
            fill,
            border: Some((width, border)),
        });
        self
    }

    pub fn text(
        &mut self,
        rect: Rect<Logical>,
        content: impl Into<String>,
        size: f32,
        color: Color,
        align: Align,
    ) -> &mut Self {
        self.items.push(Item::Text {
            rect,
            content: content.into(),
            size,
            bold: false,
            color,
            align,
        });
        self
    }

    /// 可點擊區域：點擊時送出 `message`。
    pub fn button(
        &mut self,
        rect: Rect<Logical>,
        radius: f32,
        message: M,
        hover: Color,
    ) -> &mut Self {
        self.items.push(Item::Hit {
            rect,
            radius,
            action: Action::Message(message),
            cursor: CursorHint::Pointer,
            hover: Some(hover),
            z: 1,
        });
        self
    }

    /// 拖曳區：按下時開始移動視窗。
    pub fn drag_area(&mut self, rect: Rect<Logical>) -> &mut Self {
        self.items.push(Item::Hit {
            rect,
            radius: 0.0,
            action: Action::DragMove,
            cursor: CursorHint::Grab,
            hover: None,
            z: 0,
        });
        self
    }
}

/// 節點 id → 動作 對照表（每次繪製重建）。
#[derive(Clone, Debug)]
pub struct HitMap<M> {
    entries: Vec<(NodeId, Action<M>)>,
}

impl<M> Default for HitMap<M> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<M> HitMap<M> {
    pub fn action(&self, id: NodeId) -> Option<&Action<M>> {
        self.entries
            .iter()
            .find(|(entry, _)| *entry == id)
            .map(|(_, action)| action)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 第一個符合條件的區域 id（測試與除錯用）。
    pub fn find(&self, pred: impl Fn(&Action<M>) -> bool) -> Option<NodeId> {
        self.entries
            .iter()
            .find(|(_, action)| pred(action))
            .map(|(id, _)| *id)
    }
}

/// 把 `view` 畫進 `painter` 並重建 `hit`。回傳 id → 動作對照表。
pub fn paint_view<M: Clone>(
    view: &View<M>,
    text: &mut TextEngine,
    painter: &mut Painter<'_>,
    hit: &mut HitTestLayer,
    hovered: Option<NodeId>,
    scale: f32,
) -> HitMap<M> {
    hit.clear();
    let mut map = HitMap::default();
    let mut next_id = 1u64;
    for item in &view.items {
        match item {
            Item::Rect {
                rect,
                radius,
                fill,
                border,
            } => match border {
                Some((width, color)) => {
                    painter.rounded_rect_bordered(*rect, *radius, *fill, *width, *color)
                }
                None => painter.rounded_rect(*rect, *radius, *fill),
            },
            Item::Text {
                rect,
                content,
                size,
                bold,
                color,
                align,
            } => text.draw(painter, content, *rect, *size, *bold, *color, *align, scale),
            Item::Hit {
                rect,
                radius,
                action,
                cursor,
                hover,
                z,
            } => {
                let id = NodeId::new(next_id);
                next_id += 1;
                if let (Some(color), true) = (hover, hovered == Some(id)) {
                    painter.rounded_rect(*rect, *radius, *color);
                }
                hit.push_region(
                    HitRegionSpec::rounded_rect(id, *rect, *radius)
                        .enabled(true)
                        .z_layer(*z)
                        .cursor(*cursor),
                );
                map.entries.push((id, action.clone()));
            }
        }
    }
    map
}

/// 命中測試結果 → 動作：左鍵點擊 `Message`，或左鍵按下 `DragMove`。
pub fn route<M: Clone>(map: &HitMap<M>, outcome: &PointerOutcome) -> Option<Action<M>> {
    match outcome {
        PointerOutcome::Clicked { id, button: 0, .. } => match map.action(*id)? {
            action @ Action::Message(_) => Some(action.clone()),
            Action::DragMove => None,
        },
        PointerOutcome::Pressed { id, button: 0, .. } => match map.action(*id)? {
            Action::DragMove => Some(Action::DragMove),
            Action::Message(_) => None,
        },
        _ => None,
    }
}
