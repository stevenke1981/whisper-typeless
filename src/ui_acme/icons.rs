//! 單色向量圖示：16 格線、1.5px 線條，只用線段、圓環與矩形（CPU 渲染器沒有彩色字形）。
//!
//! [`icon`] 把 16×16 的設計格縮放到 `rect` 的較短邊並置中。

use acme_core::{Color, Logical, Point, Rect};

use super::paint::{r, View};

/// 可用的圖示。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Gear,
    Close,
    Minimize,
    ChevronDown,
    ChevronUp,
    Window,
    Pin,
    Alert,
    Check,
    Download,
    Plus,
}

impl Icon {
    pub const ALL: [Icon; 11] = [
        Icon::Gear,
        Icon::Close,
        Icon::Minimize,
        Icon::ChevronDown,
        Icon::ChevronUp,
        Icon::Window,
        Icon::Pin,
        Icon::Alert,
        Icon::Check,
        Icon::Download,
        Icon::Plus,
    ];
}

/// 設計格線寬。
const STROKE: f32 = 1.5;

/// 16 格座標 → 視窗座標的換算。
struct Grid {
    x0: f32,
    y0: f32,
    k: f32,
}

impl Grid {
    fn p(&self, x: f32, y: f32) -> Point<Logical> {
        Point::new(self.x0 + x * self.k, self.y0 + y * self.k)
    }
    fn len(&self, v: f32) -> f32 {
        v * self.k
    }
}

/// 在 `rect` 內畫出 `icon`（16 格縮放到較短邊、置中）。
pub fn icon(v: &mut View, rect: Rect<Logical>, icon: Icon, color: Color) {
    let (rx, ry) = (rect.origin.x.get(), rect.origin.y.get());
    let (rw, rh) = (rect.size.width.get(), rect.size.height.get());
    let side = rw.min(rh);
    if side <= 0.0 {
        return;
    }
    let g = Grid {
        x0: rx + (rw - side) / 2.0,
        y0: ry + (rh - side) / 2.0,
        k: side / 16.0,
    };
    let sw = g.len(STROKE).max(1.0);
    let line = |v: &mut View, a: (f32, f32), b: (f32, f32)| {
        v.line(g.p(a.0, a.1), g.p(b.0, b.1), sw, color);
    };
    match icon {
        Icon::Gear => {
            // 粗環當齒輪本體，8 個短而粗的齒貼在外緣。
            v.ring(g.p(8.0, 8.0), g.len(5.0), g.len(2.0).max(sw), color);
            let tooth = g.len(2.5).max(sw);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let (s, c) = a.sin_cos();
                v.line(
                    g.p(8.0 + 4.5 * c, 8.0 + 4.5 * s),
                    g.p(8.0 + 6.5 * c, 8.0 + 6.5 * s),
                    tooth,
                    color,
                );
            }
        }
        Icon::Close => {
            line(v, (4.0, 4.0), (12.0, 12.0));
            line(v, (12.0, 4.0), (4.0, 12.0));
        }
        Icon::Minimize => line(v, (3.5, 8.0), (12.5, 8.0)),
        Icon::ChevronDown => {
            line(v, (4.0, 6.0), (8.0, 10.0));
            line(v, (8.0, 10.0), (12.0, 6.0));
        }
        Icon::ChevronUp => {
            line(v, (4.0, 10.0), (8.0, 6.0));
            line(v, (8.0, 6.0), (12.0, 10.0));
        }
        Icon::Window => {
            let o = g.p(2.75, 3.75);
            v.bordered(
                r(o.x.get(), o.y.get(), g.len(10.5), g.len(8.5)),
                g.len(1.5),
                Color::TRANSPARENT,
                sw,
                color,
            );
            line(v, (2.75, 6.25), (13.25, 6.25));
        }
        Icon::Pin => {
            v.ring(g.p(8.0, 5.5), g.len(3.25), sw, color);
            line(v, (8.0, 8.75), (8.0, 14.0));
        }
        Icon::Alert => {
            v.ring(g.p(8.0, 8.0), g.len(6.75), sw, color);
            line(v, (8.0, 4.5), (8.0, 8.75));
            let d = g.p(8.0, 11.25);
            v.rect(
                r(
                    d.x.get() - sw * 0.6,
                    d.y.get() - sw * 0.6,
                    sw * 1.2,
                    sw * 1.2,
                ),
                sw * 0.6,
                color,
            );
        }
        Icon::Check => {
            line(v, (3.5, 8.5), (6.5, 11.5));
            line(v, (6.5, 11.5), (12.5, 4.5));
        }
        Icon::Download => {
            line(v, (8.0, 2.5), (8.0, 10.0));
            line(v, (4.75, 7.0), (8.0, 10.25));
            line(v, (8.0, 10.25), (11.25, 7.0));
            line(v, (3.0, 13.25), (13.0, 13.25));
        }
        Icon::Plus => {
            line(v, (8.0, 3.0), (8.0, 13.0));
            line(v, (3.0, 8.0), (13.0, 8.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_acme::paint::Item;

    #[test]
    fn every_icon_draws_inside_its_rect() {
        for ic in Icon::ALL {
            let mut v = View::new();
            icon(
                &mut v,
                r(10.0, 20.0, 32.0, 24.0),
                ic,
                Color::rgba(0.0, 0.0, 0.0, 1.0),
            );
            assert!(!v.items.is_empty(), "{ic:?} 沒有畫任何東西");
            for item in &v.items {
                let pts: Vec<(f32, f32)> = match item {
                    Item::Line { from, to, .. } => {
                        vec![(from.x.get(), from.y.get()), (to.x.get(), to.y.get())]
                    }
                    Item::Ring { center, radius, .. } => vec![
                        (center.x.get() - radius, center.y.get() - radius),
                        (center.x.get() + radius, center.y.get() + radius),
                    ],
                    Item::Rect { rect, .. } => vec![
                        (rect.origin.x.get(), rect.origin.y.get()),
                        (
                            rect.origin.x.get() + rect.size.width.get(),
                            rect.origin.y.get() + rect.size.height.get(),
                        ),
                    ],
                    other => panic!("{ic:?} 用了非圖元項目 {other:?}"),
                };
                for (x, y) in pts {
                    // 置中的 24×24 方格：x 14..38，y 20..44
                    assert!((13.0..=39.0).contains(&x), "{ic:?} x={x}");
                    assert!((19.0..=45.0).contains(&y), "{ic:?} y={y}");
                }
            }
        }
    }

    #[test]
    fn empty_rect_draws_nothing() {
        let mut v = View::new();
        icon(
            &mut v,
            r(0.0, 0.0, 0.0, 10.0),
            Icon::Gear,
            Color::TRANSPARENT,
        );
        assert!(v.items.is_empty());
    }
}
