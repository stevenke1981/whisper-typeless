//! 文字輔助：塑形並繪製 zh-TW（CJK）文字。

use acme_core::{
    AtlasUpload, Color, GlyphDraw, GlyphFormat, Logical, Painter, Point, Rect, TextPrimitive,
};
use acme_text::{
    AtlasFormat, FontSystem, GlyphAtlas, GlyphAtlasConfig, TextConstraints, TextStyle,
};

/// 水平對齊方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
}

/// 優先使用的繁中字型家族；找不到時由 cosmic-text 的系統後備字型接手。
pub const CJK_FAMILY: &str = "Microsoft JhengHei";

/// 擁有字型系統與（針對 CJK 加大頁面預算的）字形圖集。
pub struct TextEngine {
    fonts: FontSystem,
    atlas: GlyphAtlas,
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

    pub fn style(size: f32, bold: bool) -> TextStyle {
        TextStyle {
            font_size: size,
            line_height: size * 1.3,
            family: CJK_FAMILY.into(),
            bold,
            italic: false,
        }
    }

    /// 量測文字寬度（邏輯像素）。
    pub fn measure_width(&mut self, content: &str, size: f32, bold: bool) -> f32 {
        let style = Self::style(size, bold);
        self.fonts
            .measure(content, &style, TextConstraints::default())
            .width
    }

    /// 在 `rect` 內垂直置中塑形，回傳可交給 `Painter` 的 `TextPrimitive`。
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
        let style = Self::style(size, bold);
        let measured = self
            .fonts
            .measure(content, &style, TextConstraints::default());
        let (rx, ry) = (rect.origin.x.get(), rect.origin.y.get());
        let (rw, rh) = (rect.size.width.get(), rect.size.height.get());
        let x = match align {
            Align::Start => rx,
            Align::Center => rx + (rw - measured.width) / 2.0,
        };
        let y = ry + (rh - measured.height) / 2.0 + measured.baseline;
        let layout = self
            .fonts
            .shape(content, &style, TextConstraints::default(), scale);
        let prepared = self.fonts.prepare(&layout, &mut self.atlas);

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
            origin: Point::new(x, y),
            color,
            glyphs,
            uploads,
        }
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
        let primitive = self.primitive(content, rect, size, bold, color, align, scale);
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
        if engine.fonts.diagnostics().system_faces > 0 {
            assert!(!p.glyphs.is_empty(), "no glyphs shaped");
        }
        assert!(engine.measure_width("按 Ctrl+Alt+L 說話", 14.0, false) >= 0.0);
    }
}
