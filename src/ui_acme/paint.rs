//! 檢視描述樹、繪製與命中測試。
//!
//! 檢視（view）是「狀態 → [`View`]」的純函式；[`paint_view`] 把描述樹同時轉成
//! `Painter` 指令與 `HitTestLayer` 區域，並回傳 [`HitMap`]（節點 id → [`Action`]）。
//! 指標事件經 `ClickDetector` 產生 `PointerOutcome`，再由 app 依 [`Action`] 轉成訊息。

use acme_core::{
    Color, CursorHint, HitRegionSpec, HitTestLayer, Logical, NodeId, Painter, Point, Rect,
};

use super::msg::{FieldId, Msg, ScrollId, SliderKind};
use super::text::{Align, TextEngine};

/// 滑桿的軌道（視窗座標）與數值範圍。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderSpec {
    pub kind: SliderKind,
    pub track_x: f32,
    pub track_w: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

/// 命中區域被啟動時要做的事。
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// 左鍵點擊（按下並在同一區域放開）時送出訊息。
    Click(Msg),
    /// 左鍵按下時開始以作業系統拖曳移動視窗。
    DragMove,
    /// 按下送 `down`、放開（或取消）送 `up`（PTT 按住說話）。
    Hold { down: Msg, up: Msg },
    /// 按下 / 拖曳時依指標 x 設定數值。
    Slider(SliderSpec),
    /// 文字框：按下時取得焦點並依指標 x 放置游標；`text_x` 為文字起點（視窗座標）。
    Field { id: FieldId, text_x: f32 },
    /// 只擋住下層（浮層的卡片本體）。
    Block,
}

/// 檢視描述樹的一個元素。後面的元素畫在前面元素之上。
#[derive(Clone, Debug)]
pub enum Item {
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
    /// 在 `rect` 寬度內自動換行（由上往下）。
    Wrapped {
        rect: Rect<Logical>,
        content: String,
        size: f32,
        color: Color,
    },
    /// 不可見的命中區域；`hover` 有值時，滑鼠移入會疊上該顏色。
    Hit {
        rect: Rect<Logical>,
        radius: f32,
        action: Action,
        cursor: CursorHint,
        hover: Option<Color>,
        /// 數字越大越優先（按鈕要壓過拖曳區、浮層要壓過底層）。
        z: i32,
    },
    PushClip(Rect<Logical>),
    PopClip,
    PushOffset(f32, f32),
    PopOffset,
}

/// 捲動區（視窗座標）：滾輪落在其中時捲動 `id`，範圍 0..=max。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollArea {
    pub rect: Rect<Logical>,
    pub id: ScrollId,
    pub max: f32,
    pub z: i32,
}

/// 檢視描述樹。
#[derive(Clone, Debug, Default)]
pub struct View {
    pub items: Vec<Item>,
    pub scrolls: Vec<ScrollArea>,
    /// 取得焦點的文字框游標（視窗座標 `[x, y, w, h]`），給輸入法候選窗定位。
    pub ime_area: Option<[f32; 4]>,
    z_base: i32,
    offsets: Vec<(f32, f32)>,
}

pub fn r(x: f32, y: f32, w: f32, h: f32) -> Rect<Logical> {
    Rect::new(x, y, w.max(0.0), h.max(0.0))
}

impl View {
    pub fn new() -> Self {
        Self::default()
    }

    /// 切換到第 `n` 層（0 = 主畫面；浮層用更高層，命中測試優先）。
    pub fn layer(&mut self, n: i32) -> &mut Self {
        self.z_base = n * 10;
        self
    }

    /// 目前的累計位移（描述樹座標 → 視窗座標）。
    pub fn offset(&self) -> (f32, f32) {
        self.offsets
            .iter()
            .fold((0.0, 0.0), |(x, y), (dx, dy)| (x + dx, y + dy))
    }

    pub fn push_offset(&mut self, dx: f32, dy: f32) {
        self.offsets.push((dx, dy));
        self.items.push(Item::PushOffset(dx, dy));
    }

    pub fn pop_offset(&mut self) {
        self.offsets.pop();
        self.items.push(Item::PopOffset);
    }

    pub fn push_clip(&mut self, rect: Rect<Logical>) {
        self.items.push(Item::PushClip(rect));
    }

    pub fn pop_clip(&mut self) {
        self.items.push(Item::PopClip);
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
        self.styled_text(rect, content, size, false, color, align)
    }

    pub fn bold_text(
        &mut self,
        rect: Rect<Logical>,
        content: impl Into<String>,
        size: f32,
        color: Color,
        align: Align,
    ) -> &mut Self {
        self.styled_text(rect, content, size, true, color, align)
    }

    pub fn styled_text(
        &mut self,
        rect: Rect<Logical>,
        content: impl Into<String>,
        size: f32,
        bold: bool,
        color: Color,
        align: Align,
    ) -> &mut Self {
        let content = content.into();
        if !content.is_empty() {
            self.items.push(Item::Text {
                rect,
                content,
                size,
                bold,
                color,
                align,
            });
        }
        self
    }

    pub fn wrapped(
        &mut self,
        rect: Rect<Logical>,
        content: impl Into<String>,
        size: f32,
        color: Color,
    ) -> &mut Self {
        let content = content.into();
        if !content.is_empty() {
            self.items.push(Item::Wrapped {
                rect,
                content,
                size,
                color,
            });
        }
        self
    }

    /// 一般命中區域（`z` 相對於目前層）。
    pub fn hit(
        &mut self,
        rect: Rect<Logical>,
        radius: f32,
        action: Action,
        cursor: CursorHint,
        hover: Option<Color>,
        z: i32,
    ) -> &mut Self {
        self.items.push(Item::Hit {
            rect,
            radius,
            action,
            cursor,
            hover,
            z: self.z_base + z,
        });
        self
    }

    /// 可點擊區域：點擊時送出 `msg`。
    pub fn button(
        &mut self,
        rect: Rect<Logical>,
        radius: f32,
        msg: impl Into<Msg>,
        hover: Color,
    ) -> &mut Self {
        self.hit(
            rect,
            radius,
            Action::Click(msg.into()),
            CursorHint::Pointer,
            Some(hover),
            2,
        )
    }

    /// 拖曳區：按下時開始移動視窗。
    pub fn drag_area(&mut self, rect: Rect<Logical>) -> &mut Self {
        self.hit(rect, 0.0, Action::DragMove, CursorHint::Default, None, 0)
    }

    /// 擋住下層的區域（不做事）。
    pub fn block(&mut self, rect: Rect<Logical>, radius: f32) -> &mut Self {
        self.hit(rect, radius, Action::Block, CursorHint::Default, None, 1)
    }

    /// 宣告捲動區（`rect` 為描述樹座標，會換算成視窗座標）。
    pub fn scroll_area(&mut self, rect: Rect<Logical>, id: ScrollId, max: f32) -> &mut Self {
        let (dx, dy) = self.offset();
        self.scrolls.push(ScrollArea {
            rect: r(
                rect.origin.x.get() + dx,
                rect.origin.y.get() + dy,
                rect.size.width.get(),
                rect.size.height.get(),
            ),
            id,
            max: max.max(0.0),
            z: self.z_base,
        });
        self
    }

    /// 命中區域的數量（測試用）。
    pub fn hit_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item, Item::Hit { .. }))
            .count()
    }

    /// 找出第一個動作符合條件之命中區域的中心點（視窗座標，測試用）。
    pub fn find_hit(&self, pred: impl Fn(&Action) -> bool) -> Option<Point<Logical>> {
        let mut offsets: Vec<(f32, f32)> = Vec::new();
        for item in &self.items {
            match item {
                Item::PushOffset(dx, dy) => offsets.push((*dx, *dy)),
                Item::PopOffset => {
                    offsets.pop();
                }
                Item::Hit { rect, action, .. } if pred(action) => {
                    let (dx, dy) = offsets
                        .iter()
                        .fold((0.0, 0.0), |(x, y), (a, b)| (x + a, y + b));
                    return Some(Point::new(
                        rect.origin.x.get() + rect.size.width.get() / 2.0 + dx,
                        rect.origin.y.get() + rect.size.height.get() / 2.0 + dy,
                    ));
                }
                _ => {}
            }
        }
        None
    }
}

/// 節點 id → 動作 對照表（每次繪製重建）。
#[derive(Clone, Debug, Default)]
pub struct HitMap {
    entries: Vec<(NodeId, Action)>,
    pub scrolls: Vec<ScrollArea>,
}

impl HitMap {
    pub fn action(&self, id: NodeId) -> Option<&Action> {
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

    /// 包含 `point` 的最上層捲動區。
    pub fn scroll_at(&self, point: Point<Logical>) -> Option<ScrollArea> {
        self.scrolls
            .iter()
            .filter(|area| contains(area.rect, point))
            .max_by_key(|area| area.z)
            .copied()
    }
}

pub fn contains(rect: Rect<Logical>, p: Point<Logical>) -> bool {
    let (x, y) = (p.x.get(), p.y.get());
    let (rx, ry) = (rect.origin.x.get(), rect.origin.y.get());
    x >= rx && y >= ry && x < rx + rect.size.width.get() && y < ry + rect.size.height.get()
}

/// 把 `view` 畫進 `painter` 並重建 `hit`。回傳 id → 動作對照表。
pub fn paint_view(
    view: &View,
    text: &mut TextEngine,
    painter: &mut Painter<'_>,
    hit: &mut HitTestLayer,
    hovered: Option<NodeId>,
    scale: f32,
) -> HitMap {
    hit.clear();
    let mut map = HitMap {
        entries: Vec::new(),
        scrolls: view.scrolls.clone(),
    };
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
            } => {
                if painter.is_visible(*rect) {
                    text.draw(painter, content, *rect, *size, *bold, *color, *align, scale)
                }
            }
            Item::Wrapped {
                rect,
                content,
                size,
                color,
            } => {
                if painter.is_visible(*rect) {
                    text.draw_wrapped(painter, content, *rect, *size, *color, scale)
                }
            }
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
            Item::PushClip(rect) => {
                painter.push_clip(*rect);
                hit.push_clip(*rect);
            }
            Item::PopClip => {
                painter.pop_clip();
                hit.pop_clip();
            }
            Item::PushOffset(dx, dy) => {
                painter.push_offset(*dx, *dy);
                hit.push_offset(*dx, *dy);
            }
            Item::PopOffset => {
                painter.pop_offset();
                hit.pop_offset();
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::UiCommand;

    #[test]
    fn offsets_accumulate_and_scroll_areas_are_in_window_space() {
        let mut v = View::new();
        v.push_offset(10.0, 20.0);
        v.push_offset(1.0, 2.0);
        assert_eq!(v.offset(), (11.0, 22.0));
        v.scroll_area(r(0.0, 0.0, 50.0, 50.0), ScrollId::Settings, 100.0);
        v.button(
            r(0.0, 0.0, 10.0, 10.0),
            0.0,
            UiCommand::ClearTranscript,
            Color::rgba(0.0, 0.0, 0.0, 0.1),
        );
        v.pop_offset();
        v.pop_offset();
        assert_eq!(v.offset(), (0.0, 0.0));
        assert_eq!(v.scrolls[0].rect.origin.x.get(), 11.0);
        let center = v
            .find_hit(|a| matches!(a, Action::Click(_)))
            .expect("button");
        assert_eq!((center.x.get(), center.y.get()), (16.0, 27.0));
        assert_eq!(v.hit_count(), 1);
    }

    #[test]
    fn scroll_lookup_prefers_higher_layer() {
        let mut v = View::new();
        v.scroll_area(r(0.0, 0.0, 100.0, 100.0), ScrollId::Transcript, 10.0);
        v.layer(5);
        v.scroll_area(r(0.0, 0.0, 100.0, 100.0), ScrollId::ErrorLog, 10.0);
        let map = HitMap {
            entries: Vec::new(),
            scrolls: v.scrolls.clone(),
        };
        let hit = map.scroll_at(Point::new(5.0, 5.0)).unwrap();
        assert_eq!(hit.id, ScrollId::ErrorLog);
        assert!(map.scroll_at(Point::new(500.0, 5.0)).is_none());
    }
}
