//! 文字輔助：塑形並繪製 zh-TW（CJK）文字，含自動換行、省略號與量測快取。

use std::collections::HashMap;

use acme_core::{
    AtlasUpload, Color, GlyphDraw, GlyphFormat, Logical, Painter, Point, Rect, TextPrimitive,
};
use acme_text::{
    AtlasFormat, FontSystem, GlyphAtlas, GlyphAtlasConfig, TextConstraints, TextLayout, TextStyle,
    TextWrap,
};

/// 水平對齊方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

/// 優先使用的繁中字型家族；找不到時由 cosmic-text 的系統後備字型接手。
pub const CJK_FAMILY: &str = "Microsoft JhengHei";

/// 省略號。
pub const ELLIPSIS: &str = "…";

/// 快取上限（超過就整個清空；畫面上的字串數量遠小於此）。
const CACHE_LIMIT: usize = 2048;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LayoutKey {
    content: String,
    size: u32,
    bold: bool,
    wrap_width: Option<u32>,
    scale: u32,
}

/// 擁有字型系統與（針對 CJK 加大頁面預算的）字形圖集。
pub struct TextEngine {
    fonts: FontSystem,
    atlas: GlyphAtlas,
    cache: HashMap<LayoutKey, TextLayout>,
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    pub fn new() -> Self {
        // CJK 字形數量大：2048 見方頁面、最多 8 頁（上限）。頁面是按需配置。
        let atlas = GlyphAtlas::with_config(GlyphAtlasConfig {
            page_width: 2048,
            page_height: 2048,
            max_pages: GlyphAtlasConfig::MAX_PAGES_CAP,
        });
        Self {
            fonts: FontSystem::new(),
            atlas,
            cache: HashMap::new(),
        }
    }

    /// 每個 frame 開始時呼叫（圖集的 LRU 計時）。
    pub fn begin_frame(&mut self) {
        self.atlas.begin_frame();
    }

    /// GPU 資源重建後呼叫，使圖集內容失效。
    pub fn clear(&mut self) {
        self.atlas.clear();
    }

    /// 字型系統（給 acme-textinput 量測游標位置）。
    pub fn fonts_mut(&mut self) -> &mut FontSystem {
        &mut self.fonts
    }

    /// 是否有任何系統字型（CI 機器可能沒有）。
    pub fn has_fonts(&self) -> bool {
        self.fonts.diagnostics().system_faces > 0
    }

    pub fn style(size: f32, bold: bool) -> TextStyle {
        TextStyle {
            font_size: size,
            line_height: size * 1.3,
            family: CJK_FAMILY.into(),
            bold,
            italic: false,
        }
    }

    fn constraints(wrap_width: Option<f32>) -> TextConstraints {
        match wrap_width {
            Some(w) => TextConstraints {
                max_width: Some(w.max(1.0)),
                wrap: TextWrap::WordOrGlyph,
            },
            None => TextConstraints::default(),
        }
    }

    fn layout(
        &mut self,
        content: &str,
        size: f32,
        bold: bool,
        wrap_width: Option<f32>,
        scale: f32,
    ) -> &TextLayout {
        let key = LayoutKey {
            content: content.to_string(),
            size: size.to_bits(),
            bold,
            wrap_width: wrap_width.map(f32::to_bits),
            scale: scale.to_bits(),
        };
        if self.cache.len() >= CACHE_LIMIT && !self.cache.contains_key(&key) {
            self.cache.clear();
        }
        let fonts = &mut self.fonts;
        self.cache.entry(key).or_insert_with(|| {
            fonts.shape(
                content,
                &Self::style(size, bold),
                Self::constraints(wrap_width),
                scale,
            )
        })
    }

    /// 單行文字寬度（邏輯像素）。
    pub fn measure_width(&mut self, content: &str, size: f32, bold: bool) -> f32 {
        if content.is_empty() {
            return 0.0;
        }
        self.layout(content, size, bold, None, 1.0).width
    }

    /// 在寬度 `width` 內自動換行後的高度（邏輯像素）。
    pub fn measure_wrapped(&mut self, content: &str, size: f32, width: f32) -> f32 {
        if content.is_empty() {
            return size * 1.3;
        }
        let layout = self.layout(content, size, false, Some(width), 1.0);
        layout.height.max(size * 1.3)
    }

    /// 超出 `max_width` 時截斷並加上省略號。
    pub fn elide(&mut self, content: &str, size: f32, bold: bool, max_width: f32) -> String {
        if self.measure_width(content, size, bold) <= max_width {
            return content.to_string();
        }
        // boundaries[k] = 前 k 個字元的位元組長度。
        let boundaries: Vec<usize> = content
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(content.len()))
            .collect();
        let style = Self::style(size, bold);
        let (mut lo, mut hi) = (0usize, boundaries.len().saturating_sub(2));
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let candidate = format!("{}{ELLIPSIS}", &content[..boundaries[mid]]);
            let w = self
                .fonts
                .measure(&candidate, &style, TextConstraints::default())
                .width;
            if w <= max_width {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        if lo == 0 {
            return ELLIPSIS.to_string();
        }
        format!("{}{ELLIPSIS}", &content[..boundaries[lo]])
    }

    fn primitive_from_layout(
        &mut self,
        key_layout: TextLayout,
        origin: Point<Logical>,
        color: Color,
    ) -> TextPrimitive {
        let prepared = self.fonts.prepare(&key_layout, &mut self.atlas);
        let uploads = prepared
            .uploads
            .into_iter()
            .map(|region| AtlasUpload {
                page: region.page,
                origin: [region.x, region.y],
                size: [region.width, region.height],
                format: map_format(region.format),
                pixels: region.pixels,
            })
            .collect();
        let glyphs = prepared
            .glyphs
            .into_iter()
            .map(|glyph| GlyphDraw {
                x: glyph.x as f32,
                y: glyph.y as f32,
                width: glyph.width as f32,
                height: glyph.height as f32,
                atlas_x: glyph.atlas_x,
                atlas_y: glyph.atlas_y,
                atlas_page: glyph.atlas_page,
                format: map_format(glyph.format),
            })
            .collect();
        TextPrimitive {
            origin,
            color,
            glyphs,
            uploads,
        }
    }

    /// 單行：在 `rect` 內垂直置中塑形，回傳可交給 `Painter` 的 `TextPrimitive`。
    #[allow(clippy::too_many_arguments)]
    pub fn primitive(
        &mut self,
        content: &str,
        rect: Rect<Logical>,
        size: f32,
        bold: bool,
        color: Color,
        align: Align,
        scale: f32,
    ) -> TextPrimitive {
        let layout = self.layout(content, size, bold, None, scale).clone();
        let (rx, ry) = (rect.origin.x.get(), rect.origin.y.get());
        let (rw, rh) = (rect.size.width.get(), rect.size.height.get());
        let x = match align {
            Align::Start => rx,
            Align::Center => rx + (rw - layout.width) / 2.0,
            Align::End => rx + rw - layout.width,
        };
        let y = ry + (rh - layout.height) / 2.0 + layout.baseline;
        self.primitive_from_layout(layout, Point::new(x, y), color)
    }

    /// 多行：在 `rect` 寬度內自動換行，由上往下排。
    pub fn wrapped_primitive(
        &mut self,
        content: &str,
        rect: Rect<Logical>,
        size: f32,
        color: Color,
        scale: f32,
    ) -> TextPrimitive {
        let width = rect.size.width.get();
        let layout = self
            .layout(content, size, false, Some(width), scale)
            .clone();
        let origin = Point::new(rect.origin.x.get(), rect.origin.y.get() + layout.baseline);
        self.primitive_from_layout(layout, origin, color)
    }

    /// 塑形並直接畫到 `painter`。
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        painter: &mut Painter<'_>,
        content: &str,
        rect: Rect<Logical>,
        size: f32,
        bold: bool,
        color: Color,
        align: Align,
        scale: f32,
    ) {
        if content.is_empty() {
            return;
        }
        let primitive = self.primitive(content, rect, size, bold, color, align, scale);
        painter.text_primitive(primitive);
    }

    /// 自動換行並畫到 `painter`。
    pub fn draw_wrapped(
        &mut self,
        painter: &mut Painter<'_>,
        content: &str,
        rect: Rect<Logical>,
        size: f32,
        color: Color,
        scale: f32,
    ) {
        if content.is_empty() {
            return;
        }
        let primitive = self.wrapped_primitive(content, rect, size, color, scale);
        painter.text_primitive(primitive);
    }
}

fn map_format(format: AtlasFormat) -> GlyphFormat {
    match format {
        AtlasFormat::Alpha8 => GlyphFormat::Alpha8,
        AtlasFormat::Rgba8 => GlyphFormat::Rgba8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zh_tw_text_shapes_to_glyphs_when_fonts_exist() {
        let mut engine = TextEngine::new();
        let rect = Rect::<Logical>::new(0.0, 0.0, 200.0, 30.0);
        let p = engine.primitive(
            "按 Ctrl+Alt+L 說話",
            rect,
            14.0,
            false,
            Color::rgba(1.0, 1.0, 1.0, 1.0),
            Align::Start,
            1.0,
        );
        // CI 機器可能沒有任何系統字型；有字型時一定要有字形。
        if engine.has_fonts() {
            assert!(!p.glyphs.is_empty(), "no glyphs shaped");
        }
        assert!(engine.measure_width("按 Ctrl+Alt+L 說話", 14.0, false) >= 0.0);
    }

    #[test]
    fn wrapping_grows_height_and_elide_shrinks_width() {
        let mut engine = TextEngine::new();
        if !engine.has_fonts() {
            return;
        }
        let long = "這是一段很長的繁體中文辨識結果，用來確認自動換行會產生多行文字。".repeat(3);
        let one = engine.measure_wrapped("短", 15.0, 300.0);
        let many = engine.measure_wrapped(&long, 15.0, 120.0);
        assert!(many > one * 2.0, "wrapped {many} vs single {one}");

        let elided = engine.elide(&long, 13.0, false, 80.0);
        assert!(elided.ends_with(ELLIPSIS));
        assert!(engine.measure_width(&elided, 13.0, false) <= 80.0 + 0.5);
        assert_eq!(engine.elide("短", 13.0, false, 80.0), "短");
    }
}
