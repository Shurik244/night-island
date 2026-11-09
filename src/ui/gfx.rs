use std::cell::RefCell;
use std::rc::Rc;

use windows::Foundation::Size;
use windows::Graphics::DirectX::{DirectXAlphaMode, DirectXPixelFormat};
use windows::Graphics::SizeInt32;
use windows::UI::Composition::{CompositionDrawingSurface, CompositionGraphicsDevice, Compositor, SpriteVisual};
use windows::Win32::Foundation::{HMODULE, POINT, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_BEZIER_SEGMENT, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW,
    D2D1_FIGURE_END_CLOSED, D2D1_FILL_MODE_WINDING,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateDevice,
    D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory, ID2D1Geometry, ID2D1GeometrySink,
    ID2D1PathGeometry, ID2D1RenderTarget,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Composition::{ICompositionDrawingSurfaceInterop, ICompositorInterop};
use windows::core::{BOOL, Interface, PCWSTR, Result, w};
use windows_numerics::Matrix3x2;

mod liquid;
#[cfg(test)]
mod tests;
pub use liquid::{ElasticBody, ElasticPull, LiquidBody, cursor_pull};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EdgeShape {
    Spikes,
    Bars,
    Wave,
}

#[derive(Clone, Copy)]
pub struct RingPt {
    pub x: f32,
    pub y: f32,
    pub nx: f32,
    pub ny: f32,
    pub d: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Single,
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub base: (u8, u8, u8),
    pub tone: Tone,
    pub flip: bool,
}

impl Palette {
    pub fn at(&self, t: f32, alpha: f32) -> D2D1_COLOR_F {
        let t = if self.flip && self.tone == Tone::Horizontal { 1.0 - t } else { t };
        let (r, g, b) = crate::platform::media::gradient(self.base, t.clamp(0.0, 1.0));
        D2D1_COLOR_F { r: f32::from(r) / 255.0, g: f32::from(g) / 255.0, b: f32::from(b) / 255.0, a: alpha }
    }
}

pub type D2D1Colour = D2D1_COLOR_F;

pub const WHITE: D2D1_COLOR_F = D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

pub struct Gpu {
    gfx: CompositionGraphicsDevice,
    factory: ID2D1Factory,
    dwrite: IDWriteFactory,
    icons: PCWSTR,
    vector_icons: Rc<RefCell<super::icons::Cache>>,
    weather: std::cell::RefCell<crate::ui::weather_icons::WeatherIcons>,
}

impl Gpu {
    pub fn compositor(&self) -> Result<Compositor> {
        self.gfx.Compositor()
    }

    pub fn popup(&self) -> Self {
        Self {
            gfx: self.gfx.clone(),
            factory: self.factory.clone(),
            dwrite: self.dwrite.clone(),
            icons: self.icons,
            vector_icons: Rc::clone(&self.vector_icons),
            weather: std::cell::RefCell::default(),
        }
    }

    pub fn new(compositor: &Compositor) -> Result<Self> {
        unsafe {
            let mut d3d: Option<ID3D11Device> = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
            let dxgi: IDXGIDevice = d3d.ok_or_else(windows::core::Error::empty)?.cast()?;
            let d2d: ID2D1Device = D2D1CreateDevice(&dxgi, None)?;
            let gfx = compositor.cast::<ICompositorInterop>()?.CreateGraphicsDevice(&d2d)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let icons = pick_icon_font(&dwrite);
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            Ok(Self { gfx, factory, dwrite, icons, vector_icons: Rc::default(), weather: RefCell::default() })
        }
    }

    pub fn layer(&self, compositor: &Compositor, size: (i32, i32)) -> Result<Layer> {
        let surface = self.gfx.CreateDrawingSurface(
            Size { Width: size.0 as f32, Height: size.1 as f32 },
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            DirectXAlphaMode::Premultiplied,
        )?;
        let sprite = compositor.CreateSpriteVisual()?;
        sprite.SetBrush(&compositor.CreateSurfaceBrushWithSurface(&surface)?)?;
        sprite.SetSize(windows_numerics::Vector2 { X: size.0 as f32, Y: size.1 as f32 })?;
        Ok(Layer { surface, sprite, size })
    }

    pub fn dwrite(&self) -> &IDWriteFactory {
        &self.dwrite
    }

    pub fn icons(&self) -> PCWSTR {
        self.icons
    }
}

pub struct Layer {
    surface: CompositionDrawingSurface,
    pub sprite: SpriteVisual,
    size: (i32, i32),
}

impl Gpu {
    pub fn paint_dc(
        &self,
        hdc: windows::Win32::Graphics::Gdi::HDC,
        size: (i32, i32),
        f: impl FnOnce(&Painter),
    ) -> Result<()> {
        self.paint_dc_target(&self.create_dc_target()?, hdc, size, f)
    }

    pub(super) fn create_dc_target(&self) -> Result<windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget> {
        use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT};
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
        unsafe {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            self.factory.CreateDCRenderTarget(&props)
        }
    }

    pub(super) fn paint_dc_target(
        &self,
        dc_target: &windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget,
        hdc: windows::Win32::Graphics::Gdi::HDC,
        size: (i32, i32),
        f: impl FnOnce(&Painter),
    ) -> Result<()> {
        unsafe {
            let rect = windows::Win32::Foundation::RECT { left: 0, top: 0, right: size.0, bottom: size.1 };
            dc_target.BindDC(hdc, &rect)?;
            let target: ID2D1RenderTarget = dc_target.cast()?;
            dc_target.BeginDraw();
            dc_target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            dc_target.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
            f(&self.painter(&target));
            dc_target.EndDraw(None, None)
        }
    }

    fn painter<'a>(&'a self, dc: &'a ID2D1RenderTarget) -> Painter<'a> {
        Painter {
            dc,
            dwrite: self.dwrite(),
            icons: self.icons(),
            vector_icons: &self.vector_icons,
            weather: &self.weather,
        }
    }
}

impl Layer {
    pub fn size(&self) -> (i32, i32) {
        self.size
    }

    pub fn resize(&mut self, size: (i32, i32)) -> Result<()> {
        if size == self.size {
            return Ok(());
        }
        self.surface.Resize(SizeInt32 { Width: size.0, Height: size.1 })?;
        self.sprite.SetSize(windows_numerics::Vector2 { X: size.0 as f32, Y: size.1 as f32 })?;
        self.size = size;
        Ok(())
    }

    pub fn surface_brush(&self, compositor: &Compositor) -> Result<windows::UI::Composition::CompositionSurfaceBrush> {
        compositor.CreateSurfaceBrushWithSurface(&self.surface)
    }

    pub fn paint(&self, gpu: &Gpu, f: impl FnOnce(&Painter)) -> Result<()> {
        unsafe {
            let interop: ICompositionDrawingSurfaceInterop = self.surface.cast()?;
            let mut offset = POINT::default();
            let dc: ID2D1DeviceContext = interop.BeginDraw(None, &mut offset)?;
            dc.SetTransform(&Matrix3x2::translation(offset.x as f32, offset.y as f32));
            dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            dc.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
            let target: ID2D1RenderTarget = dc.cast()?;
            f(&gpu.painter(&target));
            interop.EndDraw()
        }
    }

    pub fn paint_region(
        &self,
        gpu: &Gpu,
        rect: (f32, f32, f32, f32),
        f: impl FnOnce(&Painter) -> Result<()>,
    ) -> Result<()> {
        let rect = RECT {
            left: (rect.0.floor() as i32).max(0),
            top: (rect.1.floor() as i32).max(0),
            right: (rect.2.ceil() as i32).min(self.size.0),
            bottom: (rect.3.ceil() as i32).min(self.size.1),
        };
        if rect.right <= rect.left || rect.bottom <= rect.top {
            return Ok(());
        }
        unsafe {
            let interop: ICompositionDrawingSurfaceInterop = self.surface.cast()?;
            let mut offset = POINT::default();
            let dc: ID2D1DeviceContext = interop.BeginDraw(Some(&rect), &mut offset)?;
            dc.SetTransform(&Matrix3x2::translation((offset.x - rect.left) as f32, (offset.y - rect.top) as f32));
            let clip = D2D_RECT_F {
                left: rect.left as f32,
                top: rect.top as f32,
                right: rect.right as f32,
                bottom: rect.bottom as f32,
            };
            dc.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
            dc.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
            let target = dc.cast::<ID2D1RenderTarget>();
            let painted = target.and_then(|target| f(&gpu.painter(&target)));
            dc.PopAxisAlignedClip();
            let ended = interop.EndDraw();
            painted.and(ended)
        }
    }
}

#[derive(Default)]
pub struct IconBitmapCache {
    domain: Option<windows::core::IUnknown>,
    bitmaps: std::collections::HashMap<
        usize,
        (std::sync::Arc<crate::platform::apps::Icon>, windows::Win32::Graphics::Direct2D::ID2D1Bitmap),
    >,
}

impl IconBitmapCache {
    #[cfg(test)]
    pub fn identities(&self) -> Vec<(usize, usize)> {
        let mut entries: Vec<_> =
            self.bitmaps.iter().map(|(key, (_, bitmap))| (*key, bitmap.as_raw() as usize)).collect();
        entries.sort_unstable();
        entries
    }

    pub fn retain(&mut self, icons: impl Iterator<Item = usize>) {
        let live: std::collections::HashSet<_> = icons.collect();
        self.bitmaps.retain(|key, _| live.contains(key));
    }
}

pub struct Painter<'a> {
    dc: &'a ID2D1RenderTarget,
    dwrite: &'a IDWriteFactory,
    icons: PCWSTR,
    vector_icons: &'a RefCell<super::icons::Cache>,
    weather: &'a std::cell::RefCell<crate::ui::weather_icons::WeatherIcons>,
}

impl Painter<'_> {
    pub fn weather_svg(&self, name: &'static str, rect: (f32, f32, f32, f32), animated: bool) -> Result<()> {
        self.weather.borrow_mut().draw(self.dc, name, rect, animated)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &self,
        text: &str,
        rect: (f32, f32, f32, f32),
        size: f32,
        weight: DWRITE_FONT_WEIGHT,
        align: DWRITE_TEXT_ALIGNMENT,
        color: D2D1_COLOR_F,
    ) -> Result<()> {
        self.text_in(w!("Segoe UI"), &crate::ui::i18n::tr(text), rect, size, weight, align, color)
    }

    pub fn text_wrapped(
        &self,
        text: &str,
        rect: (f32, f32, f32, f32),
        size: f32,
        weight: DWRITE_FONT_WEIGHT,
        color: D2D1_COLOR_F,
    ) -> Result<()> {
        use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS;
        let text = crate::ui::i18n::tr(text);
        unsafe {
            let format = crate::ui::fonts::format(self.dwrite, size, weight)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
            let wide: Vec<u16> = text.encode_utf16().collect();
            let layout =
                self.dwrite.CreateTextLayout(&wide, &format, (rect.2 - rect.0).max(1.0), (rect.3 - rect.1).max(1.0))?;
            let mut m = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut m)?;
            let y = rect.1 + ((rect.3 - rect.1) - m.height).max(0.0) / 2.0;
            self.draw_layout(rect.0, y, &layout, color)
        }
    }

    pub fn text_wrapped_trimmed(
        &self,
        text: &str,
        rect: (f32, f32, f32, f32),
        size: f32,
        weight: DWRITE_FONT_WEIGHT,
        color: D2D1_COLOR_F,
    ) -> Result<()> {
        unsafe {
            let format = crate::ui::fonts::format(self.dwrite, size, weight)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
            let sign = self.dwrite.CreateEllipsisTrimmingSign(&format)?;
            format.SetTrimming(
                &DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                },
                &sign,
            )?;
            let wide: Vec<u16> = text.encode_utf16().collect();
            let layout =
                self.dwrite.CreateTextLayout(&wide, &format, (rect.2 - rect.0).max(1.0), (rect.3 - rect.1).max(1.0))?;
            self.push_clip(rect);
            let result = self.draw_layout(rect.0, rect.1, &layout, color);
            self.pop_clip();
            result
        }
    }

    pub fn text_scrolled(
        &self,
        text: &str,
        rect: (f32, f32, f32, f32),
        size: f32,
        scroll: f32,
        color: D2D1_COLOR_F,
    ) -> Result<f32> {
        use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS;
        unsafe {
            let format = crate::ui::fonts::format(self.dwrite, size, DWRITE_FONT_WEIGHT_NORMAL)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
            let wide: Vec<u16> = text.encode_utf16().collect();
            let layout = self.dwrite.CreateTextLayout(&wide, &format, (rect.2 - rect.0).max(1.0), 100_000.0)?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics)?;
            let max = (metrics.height - (rect.3 - rect.1)).max(0.0);
            self.push_clip(rect);
            let result = self.draw_layout(rect.0, rect.1 - scroll.clamp(0.0, max), &layout, color);
            self.pop_clip();
            result?;
            Ok(max)
        }
    }

    pub fn md_layout(
        &self,
        doc: &crate::ui::md::Doc,
        width: f32,
        size: f32,
        italic: bool,
    ) -> Result<IDWriteTextLayout> {
        use crate::ui::md::Style;
        use windows::Win32::Graphics::DirectWrite::{
            DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_WEIGHT_BOLD, DWRITE_TEXT_RANGE,
        };
        let layout = make_layout(self.dwrite, &doc.text, width, size)?;
        unsafe {
            let all = doc.text.encode_utf16().count() as u32;
            if italic && all > 0 {
                layout.SetFontStyle(DWRITE_FONT_STYLE_ITALIC, DWRITE_TEXT_RANGE { startPosition: 0, length: all })?;
            }
            for span in &doc.spans {
                let range = DWRITE_TEXT_RANGE { startPosition: span.start as u32, length: span.len as u32 };
                match span.style {
                    Style::Bold => layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range)?,
                    Style::Italic => layout.SetFontStyle(DWRITE_FONT_STYLE_ITALIC, range)?,
                    Style::Code | Style::Block => {
                        layout.SetFontFamilyName(w!("Consolas"), range)?;
                        layout.SetFontSize(size * 0.92, range)?;
                    }
                    Style::Heading(level) => {
                        layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range)?;
                        layout.SetFontSize(size * (1.55 - 0.13 * f32::from(level)), range)?;
                    }
                    Style::Link => layout.SetUnderline(true, range)?,
                }
            }
        }
        Ok(layout)
    }

    pub fn layout_height(&self, layout: &IDWriteTextLayout) -> f32 {
        use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS;
        let mut m = DWRITE_TEXT_METRICS::default();
        unsafe {
            let _ = layout.GetMetrics(&mut m);
        }
        m.height
    }

    pub fn md_draw(
        &self,
        layout: &IDWriteTextLayout,
        doc: &crate::ui::md::Doc,
        x: f32,
        y: f32,
        color: D2D1_COLOR_F,
        code_bg: D2D1_COLOR_F,
    ) -> Result<()> {
        use crate::ui::md::Style;
        use windows::Win32::Graphics::DirectWrite::DWRITE_HIT_TEST_METRICS;
        for span in &doc.spans {
            if !matches!(span.style, Style::Code | Style::Block) {
                continue;
            }
            let mut boxes = [DWRITE_HIT_TEST_METRICS::default(); 24];
            let mut count = 0u32;
            let _ = unsafe {
                layout.HitTestTextRange(span.start as u32, span.len as u32, x, y, Some(&mut boxes), &mut count)
            };
            for b in boxes.iter().take((count as usize).min(boxes.len())) {
                let pad = if span.style == Style::Block { 4.0 } else { 2.0 };
                self.fill_round_rect((b.left - pad, b.top, b.left + b.width + pad, b.top + b.height), 4.0, code_bg)?;
            }
        }
        self.draw_layout(x, y, layout, color)
    }

    pub fn highlight(
        &self,
        layout: &IDWriteTextLayout,
        origin: (f32, f32),
        start: u32,
        len: u32,
        color: D2D1_COLOR_F,
    ) -> Result<()> {
        use windows::Win32::Graphics::DirectWrite::DWRITE_HIT_TEST_METRICS;
        let mut boxes = [DWRITE_HIT_TEST_METRICS::default(); 48];
        let mut count = 0u32;
        let _ = unsafe { layout.HitTestTextRange(start, len, origin.0, origin.1, Some(&mut boxes), &mut count) };
        for b in boxes.iter().take((count as usize).min(boxes.len())) {
            self.fill_round_rect((b.left - 1.0, b.top, b.left + b.width + 1.0, b.top + b.height), 3.0, color)?;
        }
        Ok(())
    }

    pub fn svg(&self, data: &[u8], bounds: [f32; 4], rect: (f32, f32, f32, f32)) -> Result<()> {
        use windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_F;
        use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext5;
        let Ok(dc5) = self.dc.cast::<ID2D1DeviceContext5>() else { return Ok(()) };
        unsafe {
            let Some(stream) = windows::Win32::UI::Shell::SHCreateMemStream(Some(data)) else { return Ok(()) };
            let doc = dc5.CreateSvgDocument(&stream, D2D_SIZE_F { width: 64.0, height: 64.0 })?;
            let (bw, bh) = ((bounds[2] - bounds[0]).max(1.0), (bounds[3] - bounds[1]).max(1.0));
            let (rw, rh) = (rect.2 - rect.0, rect.3 - rect.1);
            let k = (rw / bw).min(rh / bh);
            let dx = rect.0 + (rw - bw * k) / 2.0 - bounds[0] * k;
            let dy = rect.1 + (rh - bh * k) / 2.0 - bounds[1] * k;
            let mut old = Matrix3x2::default();
            dc5.GetTransform(&mut old);
            let local = Matrix3x2 { M11: k, M12: 0.0, M21: 0.0, M22: k, M31: dx, M32: dy };
            dc5.SetTransform(&(local * old));
            dc5.DrawSvgDocument(&doc);
            dc5.SetTransform(&old);
        }
        Ok(())
    }

    fn icon(
        &self,
        view_box: &str,
        path: &str,
        rect: (f32, f32, f32, f32),
        size: f32,
        color: D2D1_COLOR_F,
    ) -> Result<()> {
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{view_box}\" width=\"64\" height=\"64\">             <path fill=\"#{:02x}{:02x}{:02x}\" fill-opacity=\"{:.3}\" d=\"{path}\"/></svg>",
            byte(color.r),
            byte(color.g),
            byte(color.b),
            color.a.clamp(0.0, 1.0)
        );
        let (cx, cy) = ((rect.0 + rect.2) / 2.0, (rect.1 + rect.3) / 2.0);
        let half = size * 1.25 / 2.0;
        self.svg(svg.as_bytes(), [0.0, 0.0, 64.0, 64.0], (cx - half, cy - half, cx + half, cy + half))
    }

    pub fn glyph(&self, ch: char, rect: (f32, f32, f32, f32), size: f32, color: D2D1_COLOR_F) -> Result<()> {
        if !super::icons::legacy() && self.vector_icons.borrow_mut().draw(self.dc, ch, rect, size, color)? {
            return Ok(());
        }
        if let Some((view_box, path)) = super::icons::lookup(ch)
            && self.dc.cast::<windows::Win32::Graphics::Direct2D::ID2D1DeviceContext5>().is_ok()
        {
            return self.icon(view_box, path, rect, size, color);
        }
        let mut buf = [0u8; 4];
        self.text_in(
            self.icons,
            ch.encode_utf8(&mut buf),
            rect,
            size,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            color,
        )
    }

    pub fn dwrite(&self) -> &IDWriteFactory {
        self.dwrite
    }

    pub fn draw_layout(&self, x: f32, y: f32, layout: &IDWriteTextLayout, color: D2D1_COLOR_F) -> Result<()> {
        unsafe {
            let brush = self.dc.CreateSolidColorBrush(&color, None)?;
            self.dc.DrawTextLayout(
                windows_numerics::Vector2 { X: x, Y: y },
                layout,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
        }
        Ok(())
    }

    pub fn offscreen(
        &self,
        size: (f32, f32),
        origin: (f32, f32),
        f: impl FnOnce(&Painter) -> Result<()>,
    ) -> Result<windows::Win32::Graphics::Direct2D::ID2D1Bitmap> {
        use windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_F;
        use windows::Win32::Graphics::Direct2D::D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE;
        unsafe {
            let target = self.dc.CreateCompatibleRenderTarget(
                Some(&D2D_SIZE_F { width: size.0.max(1.0), height: size.1.max(1.0) }),
                None,
                None,
                D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE,
            )?;
            let rt: ID2D1RenderTarget = target.cast()?;
            rt.BeginDraw();
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            rt.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
            rt.SetTransform(&Matrix3x2::translation(-origin.0, -origin.1));
            let painted = f(&Painter {
                dc: &rt,
                dwrite: self.dwrite,
                icons: self.icons,
                vector_icons: self.vector_icons,
                weather: self.weather,
            });
            rt.EndDraw(None, None)?;
            painted?;
            target.GetBitmap()
        }
    }

    pub fn draw_bitmap(
        &self,
        bitmap: &windows::Win32::Graphics::Direct2D::ID2D1Bitmap,
        dest: (f32, f32, f32, f32),
        opacity: f32,
    ) {
        use windows::Win32::Graphics::Direct2D::D2D1_BITMAP_INTERPOLATION_MODE_LINEAR;
        unsafe {
            let size = bitmap.GetSize();
            self.dc.DrawBitmap(
                bitmap,
                Some(&D2D_RECT_F { left: dest.0, top: dest.1, right: dest.2, bottom: dest.3 }),
                opacity,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                Some(&D2D_RECT_F { left: 0.0, top: 0.0, right: size.width, bottom: size.height }),
            );
        }
    }

    pub fn push_offset(&self, dx: f32, dy: f32) -> Matrix3x2 {
        let mut old = Matrix3x2::identity();
        unsafe {
            self.dc.GetTransform(&mut old);
            self.dc.SetTransform(&Matrix3x2 { M31: old.M31 + dx, M32: old.M32 + dy, ..old });
        }
        old
    }

    pub fn rotated<R>(
        &self,
        rect: (f32, f32, f32, f32),
        ccw: bool,
        f: impl FnOnce(&Painter, (f32, f32, f32, f32)) -> R,
    ) -> R {
        let (l, t, r, b) = rect;
        let local = (0.0, 0.0, b - t, r - l);
        let mut old = Matrix3x2::identity();
        unsafe { self.dc.GetTransform(&mut old) };
        let (tx, ty) = if ccw { (l, b) } else { (r, t) };
        let (m12, m21) = if ccw { (-1.0, 1.0) } else { (1.0, -1.0) };
        unsafe {
            self.dc.SetTransform(&Matrix3x2 {
                M11: 0.0,
                M12: m12,
                M21: m21,
                M22: 0.0,
                M31: tx + old.M31,
                M32: ty + old.M32,
            });
        }
        let result = f(self, local);
        unsafe { self.dc.SetTransform(&old) };
        result
    }

    pub fn set_transform(&self, m: &Matrix3x2) {
        unsafe { self.dc.SetTransform(m) };
    }

    pub fn with_opacity(
        &self,
        bounds: (f32, f32, f32, f32),
        alpha: f32,
        paint: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        if alpha >= 0.999 {
            return paint();
        }
        if alpha <= 0.001 {
            return Ok(());
        }
        let parameters = windows::Win32::Graphics::Direct2D::D2D1_LAYER_PARAMETERS {
            contentBounds: D2D_RECT_F { left: bounds.0, top: bounds.1, right: bounds.2, bottom: bounds.3 },
            opacity: alpha,
            ..Default::default()
        };
        unsafe {
            self.dc.PushLayer(&parameters, None::<&windows::Win32::Graphics::Direct2D::ID2D1Layer>);
        }
        let result = paint();
        unsafe {
            self.dc.PopLayer();
        }
        result
    }

    pub fn push_clip(&self, r: (f32, f32, f32, f32)) {
        unsafe {
            self.dc.PushAxisAlignedClip(
                &D2D_RECT_F { left: r.0, top: r.1, right: r.2, bottom: r.3 },
                D2D1_ANTIALIAS_MODE_ALIASED,
            );
        }
    }

    pub fn pop_clip(&self) {
        unsafe { self.dc.PopAxisAlignedClip() };
    }

    pub fn fill_rect(&self, r: (f32, f32, f32, f32), color: D2D1_COLOR_F) -> Result<()> {
        unsafe {
            let brush = self.dc.CreateSolidColorBrush(&color, None)?;
            self.dc.FillRectangle(&D2D_RECT_F { left: r.0, top: r.1, right: r.2, bottom: r.3 }, &brush);
        }
        Ok(())
    }

    fn ear_geometry(&self, size: f32, left: bool) -> Result<ID2D1PathGeometry> {
        unsafe {
            let factory: ID2D1Factory = self.dc.GetFactory()?;
            let geometry = factory.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            let r = size;
            let k = 0.5523 * r;
            let mx = |x: f32| if left { x } else { r - x };
            let pt = |x: f32, y: f32| windows_numerics::Vector2 { X: mx(x), Y: y };
            sink.BeginFigure(pt(0.0, 0.0), D2D1_FIGURE_BEGIN_FILLED);
            sink.AddLine(pt(r, 0.0));
            sink.AddLine(pt(r, r));
            sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: pt(r, r - k), point2: pt(k, 0.0), point3: pt(0.0, 0.0) });
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            Ok(geometry)
        }
    }

    pub fn fill_ear(&self, size: f32, left: bool, color: D2D1_COLOR_F) -> Result<()> {
        unsafe {
            let brush = self.dc.CreateSolidColorBrush(&color, None)?;
            self.dc.FillGeometry(&self.ear_geometry(size, left)?, &brush, None);
        }
        Ok(())
    }

    pub fn fill_ear_progress(
        &self,
        size: f32,
        left: bool,
        range: (f32, f32),
        fraction: f32,
        base: (u8, u8, u8),
    ) -> Result<()> {
        let end = range.0 + (range.1 - range.0) * fraction.clamp(0.0, 1.0);
        if end <= 0.0 {
            return Ok(());
        }
        let brush = self.progress_brush((range.0, 0.0, range.1, size), base, false)?;
        let geometry = self.ear_geometry(size, left)?;
        self.push_clip((0.0, 0.0, end.min(size), size));
        unsafe {
            self.dc.FillGeometry(&geometry, &brush, None);
        }
        self.pop_clip();
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_spikes(
        &self,
        width: f32,
        seam: f32,
        levels: &[f32],
        depth: f32,
        color: D2D1_COLOR_F,
        tip: D2D1_COLOR_F,
        shape: EdgeShape,
        palette: Palette,
    ) -> Result<ID2D1PathGeometry> {
        use windows::Win32::Graphics::Direct2D::Common::D2D1_GRADIENT_STOP;
        use windows::Win32::Graphics::Direct2D::{
            D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
        };
        unsafe {
            let factory: ID2D1Factory = self.dc.GetFactory()?;
            let pt = |x: f32, y: f32| windows_numerics::Vector2 { X: x, Y: y };
            let top = seam - 1.0;
            let slot = width / levels.len().max(1) as f32;
            let half = slot * 0.5;
            let tinted = palette.tone != Tone::Single || (tip.r, tip.g, tip.b) != (color.r, color.g, color.b);
            let stops_for = |t0: f32, t1: f32| {
                let stop = |position: f32, alpha: f32| {
                    let colour = match palette.tone {
                        Tone::Single => D2D1_COLOR_F { a: alpha, ..tip },
                        Tone::Horizontal => palette.at(t0, alpha),
                        Tone::Vertical => palette.at(t1 * position, alpha),
                    };
                    D2D1_GRADIENT_STOP { position, color: colour }
                };
                [stop(0.0, 0.0), stop(0.55, 0.85), stop(1.0, 1.0)]
            };
            let solid = self.dc.CreateSolidColorBrush(&color, None)?;

            if shape == EdgeShape::Wave {
                let n = levels.len();
                let mut ys: Vec<f32> = (0..n)
                    .map(|i| {
                        let near = |k: isize| levels[(i as isize + k).clamp(0, n as isize - 1) as usize];
                        (near(-1) * 0.25 + levels[i] * 0.5 + near(1) * 0.25).clamp(0.0, 1.0) * depth
                    })
                    .collect();
                for (i, y) in ys.iter_mut().enumerate() {
                    let edge = (i.min(n - 1 - i) as f32 / 2.5).min(1.0);
                    *y *= edge;
                }
                let mut points: Vec<(f32, f32)> = vec![(0.0, seam)];
                points.extend(ys.iter().enumerate().map(|(i, d)| ((i as f32 + 0.5) * slot, seam + d)));
                points.push((width, seam));
                let geometry = factory.CreatePathGeometry()?;
                let sink = geometry.Open()?;
                sink.SetFillMode(D2D1_FILL_MODE_WINDING);
                sink.BeginFigure(pt(0.0, top), D2D1_FIGURE_BEGIN_FILLED);
                sink.AddLine(pt(points[0].0, points[0].1));
                for i in 0..points.len() - 1 {
                    let p0 = points[i.saturating_sub(1)];
                    let (p1, p2) = (points[i], points[i + 1]);
                    let p3 = points[(i + 2).min(points.len() - 1)];
                    sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: pt(p1.0 + (p2.0 - p0.0) / 6.0, (p1.1 + (p2.1 - p0.1) / 6.0).max(seam)),
                        point2: pt(p2.0 - (p3.0 - p1.0) / 6.0, (p2.1 - (p3.1 - p1.1) / 6.0).max(seam)),
                        point3: pt(p2.0, p2.1),
                    });
                }
                sink.AddLine(pt(width, top));
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                sink.Close()?;
                self.dc.FillGeometry(&geometry, &solid, None);
                if tinted {
                    let reach = ys.iter().copied().fold(1.0, f32::max);
                    if palette.tone == Tone::Horizontal {
                        let stops: Vec<D2D1_GRADIENT_STOP> = (0..=6)
                            .map(|k| D2D1_GRADIENT_STOP {
                                position: k as f32 / 6.0,
                                color: palette.at(k as f32 / 6.0, 0.9),
                            })
                            .collect();
                        let along =
                            self.dc.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
                        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                            startPoint: pt(0.0, seam),
                            endPoint: pt(width, seam),
                        };
                        let brush = self.dc.CreateLinearGradientBrush(&props, None, &along)?;
                        self.dc.FillGeometry(&geometry, &brush, None);
                    } else {
                        let own = self.dc.CreateGradientStopCollection(
                            &stops_for(0.0, 1.0),
                            D2D1_GAMMA_2_2,
                            D2D1_EXTEND_MODE_CLAMP,
                        )?;
                        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                            startPoint: pt(width / 2.0, seam + reach * 0.1),
                            endPoint: pt(width / 2.0, seam + reach),
                        };
                        let brush = self.dc.CreateLinearGradientBrush(&props, None, &own)?;
                        self.dc.FillGeometry(&geometry, &brush, None);
                    }
                }
                return Ok(geometry);
            }

            let bars = shape == EdgeShape::Bars;
            let extent = if bars { slot * 0.34 } else { half };
            let outline = |sink: &ID2D1GeometrySink, cx: f32, d: f32| {
                if bars {
                    let r = extent.min(d * 0.5);
                    sink.AddLine(pt(cx - extent, top));
                    sink.AddLine(pt(cx - extent, seam + d - r));
                    sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: pt(cx - extent, seam + d - r * 0.45),
                        point2: pt(cx - extent * 0.45, seam + d),
                        point3: pt(cx, seam + d),
                    });
                    sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: pt(cx + extent * 0.45, seam + d),
                        point2: pt(cx + extent, seam + d - r * 0.45),
                        point3: pt(cx + extent, seam + d - r),
                    });
                    sink.AddLine(pt(cx + extent, top));
                } else {
                    sink.AddLine(pt(cx - half, top));
                    sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: pt(cx - half * 0.45, top),
                        point2: pt(cx - half * 0.18, seam + d),
                        point3: pt(cx, seam + d),
                    });
                    sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: pt(cx + half * 0.18, seam + d),
                        point2: pt(cx + half * 0.45, top),
                        point3: pt(cx + half, top),
                    });
                }
            };
            let spikes: Vec<(f32, f32)> = levels
                .iter()
                .enumerate()
                .map(|(i, l)| ((i as f32 + 0.5) * slot, l.clamp(0.0, 1.0) * depth))
                .filter(|&(_, d)| d >= 0.8)
                .collect();
            let geometry = factory.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            sink.SetFillMode(D2D1_FILL_MODE_WINDING);
            sink.BeginFigure(pt(0.0, top), D2D1_FIGURE_BEGIN_FILLED);
            for &(cx, d) in &spikes {
                outline(&sink, cx, d);
            }
            sink.AddLine(pt(width, top));
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            self.dc.FillGeometry(&geometry, &solid, None);
            if !tinted {
                return Ok(geometry);
            }
            for &(cx, d) in &spikes {
                let one = factory.CreatePathGeometry()?;
                let sink = one.Open()?;
                sink.BeginFigure(pt(cx - extent, top), D2D1_FIGURE_BEGIN_FILLED);
                outline(&sink, cx, d);
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                sink.Close()?;
                let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: pt(cx, seam + d * 0.15),
                    endPoint: pt(cx, seam + d),
                };
                let own = self.dc.CreateGradientStopCollection(
                    &stops_for(cx / width.max(1.0), d / depth.max(1.0)),
                    D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                )?;
                let brush = self.dc.CreateLinearGradientBrush(&props, None, &own)?;
                self.dc.FillGeometry(&one, &brush, None);
            }
            Ok(geometry)
        }
    }

    pub fn fill_ring(
        &self,
        points: &[RingPt],
        aura: bool,
        color: D2D1_COLOR_F,
        ray: f32,
        palette: Palette,
    ) -> Result<()> {
        use windows::Win32::Graphics::Direct2D::Common::D2D1_FILL_MODE_ALTERNATE;
        let n = points.len();
        if n < 3 {
            return Ok(());
        }
        let reach = points.iter().map(|p| p.d).fold(1.0, f32::max);
        let (x_min, x_max) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
        let across = |x: f32| ((x - x_min) / (x_max - x_min).max(1.0)).clamp(0.0, 1.0);
        unsafe {
            let factory: ID2D1Factory = self.dc.GetFactory()?;
            let pt = |x: f32, y: f32| windows_numerics::Vector2 { X: x, Y: y };
            if !aura {
                for p in points.iter().filter(|p| p.d >= 0.8) {
                    let alpha = (0.4 + 0.6 * (p.d / reach)).clamp(0.0, 1.0);
                    let ray_colour = match palette.tone {
                        Tone::Single => color,
                        Tone::Horizontal => palette.at(across(p.x), 1.0),
                        Tone::Vertical => palette.at(p.d / reach, 1.0),
                    };
                    let brush = self.dc.CreateSolidColorBrush(&D2D1_COLOR_F { a: alpha, ..ray_colour }, None)?;
                    let (tx, ty) = (-p.ny, p.nx);
                    let half = ray / 2.0;
                    let (bx, by) = (p.x - p.nx * 2.0, p.y - p.ny * 2.0);
                    let (ex, ey) = (p.x + p.nx * (p.d - half), p.y + p.ny * (p.d - half));
                    let quad = factory.CreatePathGeometry()?;
                    let sink = quad.Open()?;
                    sink.BeginFigure(pt(bx + tx * half, by + ty * half), D2D1_FIGURE_BEGIN_FILLED);
                    sink.AddLine(pt(ex + tx * half, ey + ty * half));
                    sink.AddLine(pt(ex - tx * half, ey - ty * half));
                    sink.AddLine(pt(bx - tx * half, by - ty * half));
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    sink.Close()?;
                    self.dc.FillGeometry(&quad, &brush, None);
                    self.dc.FillEllipse(&D2D1_ELLIPSE { point: pt(ex, ey), radiusX: half, radiusY: half }, &brush);
                }
                return Ok(());
            }
            let outer: Vec<(f32, f32)> =
                points.iter().map(|p| (p.x + p.nx * p.d.max(0.5), p.y + p.ny * p.d.max(0.5))).collect();
            let band = factory.CreatePathGeometry()?;
            let sink = band.Open()?;
            sink.SetFillMode(D2D1_FILL_MODE_ALTERNATE);
            sink.BeginFigure(pt(outer[0].0, outer[0].1), D2D1_FIGURE_BEGIN_FILLED);
            let edge = factory.CreatePathGeometry()?;
            let edge_sink = edge.Open()?;
            edge_sink.BeginFigure(pt(outer[0].0, outer[0].1), D2D1_FIGURE_BEGIN_HOLLOW);
            for i in 0..n {
                let p0 = outer[(i + n - 1) % n];
                let (p1, p2) = (outer[i], outer[(i + 1) % n]);
                let p3 = outer[(i + 2) % n];
                let seg = D2D1_BEZIER_SEGMENT {
                    point1: pt(p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0),
                    point2: pt(p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0),
                    point3: pt(p2.0, p2.1),
                };
                sink.AddBezier(&seg);
                edge_sink.AddBezier(&seg);
            }
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            edge_sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            edge_sink.Close()?;
            sink.BeginFigure(pt(points[0].x, points[0].y), D2D1_FIGURE_BEGIN_FILLED);
            for p in &points[1..] {
                sink.AddLine(pt(p.x, p.y));
            }
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            let width = (ray * 0.55).max(1.2);
            match palette.tone {
                Tone::Single => {
                    let fill = self.dc.CreateSolidColorBrush(&D2D1_COLOR_F { a: 0.38, ..color }, None)?;
                    self.dc.FillGeometry(&band, &fill, None);
                    let line = self.dc.CreateSolidColorBrush(&D2D1_COLOR_F { a: 0.95, ..color }, None)?;
                    self.dc.DrawGeometry(&edge, &line, width, None);
                }
                Tone::Horizontal => {
                    use windows::Win32::Graphics::Direct2D::Common::D2D1_GRADIENT_STOP;
                    use windows::Win32::Graphics::Direct2D::{
                        D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
                    };
                    let along = |alpha: f32| -> Result<_> {
                        let stops: Vec<D2D1_GRADIENT_STOP> = (0..=8)
                            .map(|k| D2D1_GRADIENT_STOP {
                                position: k as f32 / 8.0,
                                color: palette.at(k as f32 / 8.0, alpha),
                            })
                            .collect();
                        let collection =
                            self.dc.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
                        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                            startPoint: pt(x_min, 0.0),
                            endPoint: pt(x_max, 0.0),
                        };
                        self.dc.CreateLinearGradientBrush(&props, None, &collection)
                    };
                    self.dc.FillGeometry(&band, &along(0.38)?, None);
                    self.dc.DrawGeometry(&edge, &along(0.95)?, width, None);
                }
                Tone::Vertical => {
                    let fill = self.dc.CreateSolidColorBrush(&palette.at(0.2, 0.38), None)?;
                    self.dc.FillGeometry(&band, &fill, None);
                    for i in 0..n {
                        let (a, b) = (&outer[i], &outer[(i + 1) % n]);
                        let t = (points[i].d + points[(i + 1) % n].d) / 2.0 / reach;
                        let brush = self.dc.CreateSolidColorBrush(&palette.at(t, 0.95), None)?;
                        self.dc.DrawLine(pt(a.0, a.1), pt(b.0, b.1), &brush, width, None);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn fill_ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32, color: D2D1_COLOR_F) -> Result<()> {
        unsafe {
            let brush = self.dc.CreateSolidColorBrush(&color, None)?;
            self.dc.FillEllipse(
                &D2D1_ELLIPSE { point: windows_numerics::Vector2 { X: cx, Y: cy }, radiusX: rx, radiusY: ry },
                &brush,
            );
        }
        Ok(())
    }

    pub fn fill_ellipse_gradient(
        &self,
        oval: (f32, f32, f32, f32),
        light: D2D1_COLOR_F,
        shade: D2D1_COLOR_F,
    ) -> Result<()> {
        use windows::Win32::Graphics::Direct2D::Common::D2D1_GRADIENT_STOP;
        use windows::Win32::Graphics::Direct2D::{
            D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2, D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES,
        };
        let (cx, cy, rx, ry) = oval;
        let falloff = if shade.a == 0.0 { 1.0 } else { 1.35 };
        let offset = if shade.a == 0.0 { (0.0, 0.0) } else { (-rx * 0.32, -ry * 0.38) };
        unsafe {
            let stops = self.dc.CreateGradientStopCollection(
                &[
                    D2D1_GRADIENT_STOP { position: 0.0, color: light },
                    D2D1_GRADIENT_STOP { position: 1.0, color: shade },
                ],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            )?;
            let brush = self.dc.CreateRadialGradientBrush(
                &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                    center: windows_numerics::Vector2 { X: cx, Y: cy },
                    gradientOriginOffset: windows_numerics::Vector2 { X: offset.0, Y: offset.1 },
                    radiusX: rx * falloff,
                    radiusY: ry * falloff,
                },
                None,
                &stops,
            )?;
            self.dc.FillEllipse(
                &D2D1_ELLIPSE { point: windows_numerics::Vector2 { X: cx, Y: cy }, radiusX: rx, radiusY: ry },
                &brush,
            );
        }
        Ok(())
    }

    pub fn bezier(&self, points: [(f32, f32); 4], width: f32, color: D2D1_COLOR_F) -> Result<()> {
        use windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_END_OPEN;
        unsafe {
            let factory: ID2D1Factory = self.dc.GetFactory()?;
            let path = factory.CreatePathGeometry()?;
            let sink = path.Open()?;
            let pt = |p: (f32, f32)| windows_numerics::Vector2 { X: p.0, Y: p.1 };
            sink.BeginFigure(pt(points[0]), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: pt(points[1]),
                point2: pt(points[2]),
                point3: pt(points[3]),
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            sink.Close()?;
            self.dc.DrawGeometry(&path, &self.dc.CreateSolidColorBrush(&color, None)?, width, None);
        }
        Ok(())
    }

    pub fn fill_curves(&self, start: (f32, f32), curves: &[[(f32, f32); 3]], color: D2D1_COLOR_F) -> Result<()> {
        unsafe {
            let factory: ID2D1Factory = self.dc.GetFactory()?;
            let path = factory.CreatePathGeometry()?;
            let sink = path.Open()?;
            let pt = |p: (f32, f32)| windows_numerics::Vector2 { X: p.0, Y: p.1 };
            sink.BeginFigure(pt(start), D2D1_FIGURE_BEGIN_FILLED);
            for curve in curves {
                sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: pt(curve[0]),
                    point2: pt(curve[1]),
                    point3: pt(curve[2]),
                });
            }
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            self.dc.FillGeometry(&path, &self.dc.CreateSolidColorBrush(&color, None)?, None);
        }
        Ok(())
    }

    pub fn line(&self, from: (f32, f32), to: (f32, f32), width: f32, color: D2D1_COLOR_F) -> Result<()> {
        unsafe {
            let brush = self.dc.CreateSolidColorBrush(&color, None)?;
            self.dc.DrawLine(
                windows_numerics::Vector2 { X: from.0, Y: from.1 },
                windows_numerics::Vector2 { X: to.0, Y: to.1 },
                &brush,
                width,
                None,
            );
        }
        Ok(())
    }

    pub fn fill_triangle(&self, a: (f32, f32), b: (f32, f32), c: (f32, f32), color: D2D1_COLOR_F) -> Result<()> {
        unsafe {
            let factory: ID2D1Factory = self.dc.GetFactory()?;
            let geometry = factory.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            let pt = |p: (f32, f32)| windows_numerics::Vector2 { X: p.0, Y: p.1 };
            sink.BeginFigure(pt(a), D2D1_FIGURE_BEGIN_FILLED);
            sink.AddLine(pt(b));
            sink.AddLine(pt(c));
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            let brush = self.dc.CreateSolidColorBrush(&color, None)?;
            self.dc.FillGeometry(&geometry, &brush, None);
        }
        Ok(())
    }

    pub fn fill_round_rect(&self, rect: (f32, f32, f32, f32), radius: f32, color: D2D1_COLOR_F) -> Result<()> {
        unsafe {
            let brush = self.dc.CreateSolidColorBrush(&color, None)?;
            self.dc.FillRoundedRectangle(&rounded(rect, radius), &brush);
        }
        Ok(())
    }

    pub fn fill_round_gradient(
        &self,
        rect: (f32, f32, f32, f32),
        radius: f32,
        from: D2D1_COLOR_F,
        to: D2D1_COLOR_F,
    ) -> Result<()> {
        use windows::Win32::Graphics::Direct2D::Common::D2D1_GRADIENT_STOP;
        use windows::Win32::Graphics::Direct2D::{
            D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
        };
        unsafe {
            let stops = self.dc.CreateGradientStopCollection(
                &[D2D1_GRADIENT_STOP { position: 0.0, color: from }, D2D1_GRADIENT_STOP { position: 1.0, color: to }],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            )?;
            let brush = self.dc.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: windows_numerics::Vector2 { X: rect.0, Y: rect.1 },
                    endPoint: windows_numerics::Vector2 { X: rect.2, Y: rect.3 },
                },
                None,
                &stops,
            )?;
            self.dc.FillRoundedRectangle(&rounded(rect, radius), &brush);
        }
        Ok(())
    }

    pub fn fill_capsule_progress(&self, rect: (f32, f32, f32, f32), fraction: f32, base: (u8, u8, u8)) -> Result<()> {
        self.fill_body_progress(rect, rect, (rect.3 - rect.1).min(rect.2 - rect.0) / 2.0, fraction, base)
    }

    fn progress_brush(
        &self,
        range: (f32, f32, f32, f32),
        base: (u8, u8, u8),
        vertical: bool,
    ) -> Result<windows::Win32::Graphics::Direct2D::ID2D1LinearGradientBrush> {
        use windows::Win32::Graphics::Direct2D::Common::D2D1_GRADIENT_STOP;
        use windows::Win32::Graphics::Direct2D::{
            D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
        };
        let stops: [D2D1_GRADIENT_STOP; 5] = std::array::from_fn(|i| {
            let position = i as f32 / 4.0;
            let (r, g, b) = crate::platform::media::gradient(base, position);
            D2D1_GRADIENT_STOP { position, color: rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 0.24) }
        });
        unsafe {
            let collection = self.dc.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
            self.dc.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: windows_numerics::Vector2 { X: range.0, Y: range.1 },
                    endPoint: windows_numerics::Vector2 {
                        X: if vertical { range.0 } else { range.2 },
                        Y: if vertical { range.3 } else { range.1 },
                    },
                },
                None,
                &collection,
            )
        }
    }

    pub fn fill_progress_surface(
        &self,
        range: (f32, f32, f32, f32),
        size: (f32, f32),
        fraction: f32,
        base: (u8, u8, u8),
        vertical: bool,
        reverse: bool,
    ) -> Result<()> {
        if fraction <= 0.0 {
            return Ok(());
        }
        let range = if vertical && reverse { (range.0, range.3, range.2, range.1) } else { range };
        let brush = self.progress_brush(range, base, vertical)?;
        let extent = if fraction >= 1.0 {
            if vertical { if reverse { 0.0 } else { size.1 } } else { size.0 }
        } else if vertical {
            range.1 + (range.3 - range.1) * fraction.min(1.0)
        } else {
            range.0 + (range.2 - range.0) * fraction.min(1.0)
        };
        let rect = if vertical {
            if reverse { (0.0, extent, size.0, size.1) } else { (0.0, 0.0, size.0, extent) }
        } else {
            (0.0, 0.0, extent, size.1)
        };
        unsafe {
            self.dc.FillRectangle(&D2D_RECT_F { left: rect.0, top: rect.1, right: rect.2, bottom: rect.3 }, &brush);
        }
        Ok(())
    }

    pub fn fill_body_progress(
        &self,
        rect: (f32, f32, f32, f32),
        body: (f32, f32, f32, f32),
        radius: f32,
        fraction: f32,
        base: (u8, u8, u8),
    ) -> Result<()> {
        use windows::Win32::Graphics::Direct2D::Common::D2D1_GRADIENT_STOP;
        use windows::Win32::Graphics::Direct2D::{
            D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
        };
        let (left, top, right, bottom) = rect;
        let fraction = fraction.clamp(0.0, 1.0);
        if fraction <= 0.0 || right <= left || bottom <= top {
            return Ok(());
        }
        let stops: [D2D1_GRADIENT_STOP; 5] = std::array::from_fn(|i| {
            let position = i as f32 / 4.0;
            let (r, g, b) = crate::platform::media::gradient(base, position);
            D2D1_GRADIENT_STOP {
                position,
                color: rgba(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, 0.24),
            }
        });
        unsafe {
            let collection = self.dc.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
            let brush = self.dc.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: windows_numerics::Vector2 { X: left, Y: top },
                    endPoint: windows_numerics::Vector2 { X: right, Y: top },
                },
                None,
                &collection,
            )?;
            self.push_clip((left, body.1, left + (right - left) * fraction, body.3));
            self.dc.FillRoundedRectangle(&rounded(body, radius), &brush);
            self.pop_clip();
        }
        Ok(())
    }

    pub fn cached_icon(
        &self,
        rect: (f32, f32, f32, f32),
        icon: &std::sync::Arc<crate::platform::apps::Icon>,
        cache: &mut IconBitmapCache,
        opacity: f32,
    ) -> Result<()> {
        use windows::Win32::Graphics::Direct2D::Common::{
            D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
        };
        use windows::Win32::Graphics::Direct2D::{
            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
        };
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
        if icon.side == 0 || icon.side > 1024 || icon.bgra.len() != (icon.side * icon.side * 4) as usize {
            return Ok(());
        }
        let context = self.dc.cast::<ID2D1DeviceContext>().ok();
        let domain: windows::core::IUnknown =
            if let Some(context) = &context { unsafe { context.GetDevice()? }.cast()? } else { self.dc.cast()? };
        if cache.domain.as_ref().is_none_or(|cached| cached.as_raw() != domain.as_raw()) {
            cache.bitmaps.clear();
            cache.domain = Some(domain);
        }
        let key = std::sync::Arc::as_ptr(icon) as usize;
        if !cache.bitmaps.contains_key(&key) {
            if cache.bitmaps.len() >= 64 {
                cache.bitmaps.clear();
            }
            let properties = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
            };
            let bitmap = unsafe {
                self.dc.CreateBitmap(
                    D2D_SIZE_U { width: icon.side, height: icon.side },
                    Some(icon.bgra.as_ptr().cast()),
                    icon.side * 4,
                    &properties,
                )?
            };
            cache.bitmaps.insert(key, (icon.clone(), bitmap));
        }
        let bitmap = &cache.bitmaps[&key].1;
        let dest =
            D2D_RECT_F { left: rect.0.round(), top: rect.1.round(), right: rect.2.round(), bottom: rect.3.round() };
        unsafe {
            if let Some(context) = context {
                context.DrawBitmap(
                    bitmap,
                    Some(&dest),
                    opacity.clamp(0.0, 1.0),
                    D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                    None,
                    None,
                );
            } else {
                self.dc.DrawBitmap(
                    bitmap,
                    Some(&dest),
                    opacity.clamp(0.0, 1.0),
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
            }
        }
        Ok(())
    }

    pub fn image(&self, rect: (f32, f32, f32, f32), radius: f32, side: u32, bgra: &[u8]) -> Result<()> {
        use windows::Win32::Graphics::Direct2D::Common::{
            D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
        };
        use windows::Win32::Graphics::Direct2D::{D2D1_BITMAP_PROPERTIES, D2D1_BRUSH_PROPERTIES};
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
        if bgra.len() < (side * side * 4) as usize || side == 0 {
            return Ok(());
        }
        unsafe {
            let props = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
            };
            let bitmap = self.dc.CreateBitmap(
                D2D_SIZE_U { width: side, height: side },
                Some(bgra.as_ptr().cast()),
                side * 4,
                &props,
            )?;
            let k = (rect.2 - rect.0) / side as f32;
            let brush_props = D2D1_BRUSH_PROPERTIES {
                opacity: 1.0,
                transform: Matrix3x2 { M11: k, M12: 0.0, M21: 0.0, M22: k, M31: rect.0, M32: rect.1 },
            };
            let brush = self.dc.CreateBitmapBrush(&bitmap, None, Some(&brush_props))?;
            self.dc.FillRoundedRectangle(&rounded(rect, radius), &brush);
        }
        Ok(())
    }

    pub fn cover(
        &self,
        rect: (f32, f32, f32, f32),
        radius: f32,
        cover: &crate::platform::media::Cover,
        mode: crate::data::settings::CoverMode,
    ) -> Result<()> {
        use crate::data::settings::CoverMode;
        match mode {
            CoverMode::Crop => self.image(rect, radius, cover.size, &cover.bgra),
            CoverMode::Stretch => self.image_wh(rect, rect, radius, cover.full_w, cover.full_h, &cover.full),
            CoverMode::Fit => self.image_wh(rect, rect, radius, cover.full_w, cover.full_h, &cover.full),
        }
    }

    pub fn image_wh(
        &self,
        dest: (f32, f32, f32, f32),
        clip: (f32, f32, f32, f32),
        radius: f32,
        w: u32,
        h: u32,
        bgra: &[u8],
    ) -> Result<()> {
        use windows::Win32::Graphics::Direct2D::Common::{
            D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
        };
        use windows::Win32::Graphics::Direct2D::{D2D1_BITMAP_PROPERTIES, D2D1_BRUSH_PROPERTIES};
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
        if w == 0 || h == 0 || bgra.len() < (w * h * 4) as usize {
            return Ok(());
        }
        unsafe {
            let props = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
            };
            let bitmap =
                self.dc.CreateBitmap(D2D_SIZE_U { width: w, height: h }, Some(bgra.as_ptr().cast()), w * 4, &props)?;
            let brush_props = D2D1_BRUSH_PROPERTIES {
                opacity: 1.0,
                transform: Matrix3x2 {
                    M11: (dest.2 - dest.0) / w as f32,
                    M12: 0.0,
                    M21: 0.0,
                    M22: (dest.3 - dest.1) / h as f32,
                    M31: dest.0,
                    M32: dest.1,
                },
            };
            let brush = self.dc.CreateBitmapBrush(&bitmap, None, Some(&brush_props))?;
            self.dc.FillRoundedRectangle(&rounded(clip, radius), &brush);
        }
        Ok(())
    }

    pub fn stroke_round_rect(
        &self,
        rect: (f32, f32, f32, f32),
        radius: f32,
        width: f32,
        color: D2D1_COLOR_F,
    ) -> Result<()> {
        unsafe {
            let brush = self.dc.CreateSolidColorBrush(&color, None)?;
            self.dc.DrawRoundedRectangle(&rounded(rect, radius), &brush, width, None);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn text_in(
        &self,
        family: PCWSTR,
        text: &str,
        rect: (f32, f32, f32, f32),
        size: f32,
        weight: DWRITE_FONT_WEIGHT,
        align: DWRITE_TEXT_ALIGNMENT,
        color: D2D1_COLOR_F,
    ) -> Result<()> {
        unsafe {
            let format = if family == self.icons {
                self.dwrite.CreateTextFormat(
                    family,
                    None,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("en-us"),
                )?
            } else {
                super::fonts::format(self.dwrite, size, weight)?
            };
            format.SetTextAlignment(align)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let sign = self.dwrite.CreateEllipsisTrimmingSign(&format)?;
            format.SetTrimming(
                &DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                },
                &sign,
            )?;
            let brush = self.dc.CreateSolidColorBrush(&color, None)?;
            let wide: Vec<u16> = text.encode_utf16().collect();
            self.dc.DrawText(
                &wide,
                &format,
                &D2D_RECT_F { left: rect.0, top: rect.1, right: rect.2, bottom: rect.3 },
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
        Ok(())
    }
}

fn rounded(r: (f32, f32, f32, f32), radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F { left: r.0, top: r.1, right: r.2, bottom: r.3 },
        radiusX: radius,
        radiusY: radius,
    }
}

fn pick_icon_font(dwrite: &IDWriteFactory) -> PCWSTR {
    let fluent = w!("Segoe Fluent Icons");
    unsafe {
        let mut collection = None;
        if dwrite.GetSystemFontCollection(&mut collection, false).is_ok()
            && let Some(c) = collection
        {
            let (mut index, mut exists) = (0u32, BOOL(0));
            if c.FindFamilyName(fluent, &mut index, &mut exists).is_ok() && exists.as_bool() {
                return fluent;
            }
        }
    }
    w!("Segoe MDL2 Assets")
}

pub fn make_layout(dwrite: &IDWriteFactory, text: &str, width: f32, size: f32) -> Result<IDWriteTextLayout> {
    unsafe {
        let format = crate::ui::fonts::format(dwrite, size, DWRITE_FONT_WEIGHT_NORMAL)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        dwrite.CreateTextLayout(&wide, &format, width.max(1.0), 1_000_000.0)
    }
}

#[windows::core::implement(
    windows::Graphics::IGeometrySource2D,
    windows::Win32::System::WinRT::Graphics::Direct2D::IGeometrySource2DInterop
)]
struct GeometrySource(ID2D1Geometry);

impl windows::Graphics::IGeometrySource2D_Impl for GeometrySource_Impl {}

impl windows::Win32::System::WinRT::Graphics::Direct2D::IGeometrySource2DInterop_Impl for GeometrySource_Impl {
    fn GetGeometry(&self) -> Result<ID2D1Geometry> {
        Ok(self.0.clone())
    }

    fn TryGetGeometryUsingFactory(&self, _factory: windows::core::Ref<ID2D1Factory>) -> Result<ID2D1Geometry> {
        Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_NOTIMPL))
    }
}

pub fn composition_path(geometry: &ID2D1PathGeometry) -> Result<windows::UI::Composition::CompositionPath> {
    let source: windows::Graphics::IGeometrySource2D = GeometrySource(geometry.cast()?).into();
    windows::UI::Composition::CompositionPath::Create(&source)
}
