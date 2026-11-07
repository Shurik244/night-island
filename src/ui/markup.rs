use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
    D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory,
    ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWriteCreateFactory, IDWriteFactory,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BitBlt, COLORONCOLOR, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, HALFTONE, HDC,
    ReleaseDC, SRCCOPY, SelectObject, SetStretchBltMode, StretchBlt,
};
use windows::core::{Result, w};
use windows_numerics::Vector2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Arrow,
    Rect,
    Marker,
    Text,
    Blur,
}

pub const TOOLS: [Tool; 5] = [Tool::Arrow, Tool::Rect, Tool::Marker, Tool::Text, Tool::Blur];

pub const COLORS: [(f32, f32, f32); 5] =
    [(1.0, 0.23, 0.19), (1.0, 0.8, 0.0), (0.2, 0.78, 0.35), (0.04, 0.52, 1.0), (1.0, 1.0, 1.0)];

const BLUR_BLOCK: i32 = 11;
const MAX_TEXT_CHARS: usize = 200;

#[derive(Clone)]
pub struct Annot {
    pub tool: Tool,
    pub color: usize,
    pub a: POINT,
    pub b: POINT,
    pub text: String,
}

impl Annot {
    pub fn new(tool: Tool, color: usize, at: POINT) -> Self {
        Self { tool, color, a: at, b: at, text: String::new() }
    }

    pub fn push_char(&mut self, c: char) {
        if self.text.chars().count() < MAX_TEXT_CHARS {
            self.text.push(c);
        }
    }

    pub fn is_visible(&self) -> bool {
        match self.tool {
            Tool::Text => !self.text.is_empty(),
            _ => (self.a.x - self.b.x).abs() > 2 || (self.a.y - self.b.y).abs() > 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Tool(Tool),
    Color(usize),
    Undo,
    Confirm,
    Cancel,
}

pub struct Toolbar {
    pub rect: RECT,
    pub items: Vec<(Item, RECT)>,
}

impl Toolbar {
    pub fn hit(&self, x: i32, y: i32) -> Option<Item> {
        let inside = |r: &RECT| x >= r.left && x < r.right && y >= r.top && y < r.bottom;
        self.items.iter().find(|(_, r)| inside(r)).map(|(i, _)| *i)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.rect.left && x < self.rect.right && y >= self.rect.top && y < self.rect.bottom
    }
}

const BAR_H: i32 = 44;

pub fn toolbar(sel: RECT, screen_w: i32, screen_h: i32) -> Toolbar {
    let mut spec: Vec<(Item, i32)> = TOOLS.iter().map(|t| (Item::Tool(*t), 36)).collect();
    spec.extend((0..COLORS.len()).map(|i| (Item::Color(i), 26)));
    spec.extend([(Item::Undo, 36), (Item::Confirm, 36), (Item::Cancel, 36)]);
    let gaps = 2 * 10;
    let width: i32 = spec.iter().map(|(_, w)| w).sum::<i32>() + gaps + 20;

    let cx = (sel.left + sel.right) / 2;
    let left = (cx - width / 2).clamp(8, (screen_w - width - 8).max(8));
    let top = if sel.bottom + 10 + BAR_H <= screen_h {
        sel.bottom + 10
    } else if sel.top - 10 - BAR_H >= 0 {
        sel.top - 10 - BAR_H
    } else {
        (sel.bottom - BAR_H - 10).max(8)
    };
    let rect = RECT { left, top, right: left + width, bottom: top + BAR_H };

    let mut items = Vec::new();
    let mut x = left + 10;
    for (n, (item, w)) in spec.into_iter().enumerate() {
        if n == TOOLS.len() || n == TOOLS.len() + COLORS.len() {
            x += 10;
        }
        let h = if matches!(item, Item::Color(_)) { 26 } else { 34 };
        let y = top + (BAR_H - h) / 2;
        items.push((item, RECT { left: x, top: y, right: x + w, bottom: y + h }));
        x += w;
    }
    Toolbar { rect, items }
}

pub struct Renderer {
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
}

fn color(rgb: (f32, f32, f32), a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: rgb.0, g: rgb.1, b: rgb.2, a }
}

fn pt(p: POINT, shift: POINT) -> Vector2 {
    Vector2 { X: (p.x + shift.x) as f32, Y: (p.y + shift.y) as f32 }
}

fn rect_f(r: RECT) -> D2D_RECT_F {
    D2D_RECT_F { left: r.left as f32, top: r.top as f32, right: r.right as f32, bottom: r.bottom as f32 }
}

fn pixelate(dc: HDC, dst: RECT, grab_dc: HDC, src: RECT) {
    let (w, h) = (src.right - src.left, src.bottom - src.top);
    if w <= 0 || h <= 0 {
        return;
    }
    let (sw, sh) = ((w / BLUR_BLOCK).max(1), (h / BLUR_BLOCK).max(1));
    unsafe {
        let screen = GetDC(None);
        let small = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, sw, sh);
        let old = SelectObject(small, bitmap.into());
        SetStretchBltMode(small, HALFTONE);
        let _ = StretchBlt(small, 0, 0, sw, sh, Some(grab_dc), src.left, src.top, w, h, SRCCOPY);
        SetStretchBltMode(dc, COLORONCOLOR);
        let _ = StretchBlt(
            dc,
            dst.left,
            dst.top,
            dst.right - dst.left,
            dst.bottom - dst.top,
            Some(small),
            0,
            0,
            sw,
            sh,
            SRCCOPY,
        );
        SelectObject(small, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(small);
        ReleaseDC(None, screen);
    }
}

fn norm(a: POINT, b: POINT) -> RECT {
    RECT { left: a.x.min(b.x), top: a.y.min(b.y), right: a.x.max(b.x), bottom: a.y.max(b.y) }
}

impl Renderer {
    pub fn new() -> Option<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            Some(Self { d2d, dwrite })
        }
    }

    fn target(&self) -> Result<ID2D1DCRenderTarget> {
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
        unsafe { self.d2d.CreateDCRenderTarget(&props) }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        dc: HDC,
        bind: RECT,
        shift: POINT,
        grab_dc: HDC,
        annots: &[Annot],
        current: Option<&Annot>,
        typing: bool,
        clip: RECT,
        bar: Option<(&Toolbar, Tool, usize)>,
    ) {
        let all: Vec<&Annot> = annots.iter().chain(current).collect();

        for a in all.iter().filter(|a| a.tool == Tool::Blur) {
            let src = norm(a.a, a.b);
            let dst = RECT {
                left: src.left + shift.x,
                top: src.top + shift.y,
                right: src.right + shift.x,
                bottom: src.bottom + shift.y,
            };
            let clipped = RECT {
                left: dst.left.max(clip.left),
                top: dst.top.max(clip.top),
                right: dst.right.min(clip.right),
                bottom: dst.bottom.min(clip.bottom),
            };
            let src_clipped = RECT {
                left: clipped.left - shift.x,
                top: clipped.top - shift.y,
                right: clipped.right - shift.x,
                bottom: clipped.bottom - shift.y,
            };
            pixelate(dc, clipped, grab_dc, src_clipped);
        }

        let Ok(rt) = self.target() else { return };
        unsafe {
            if rt.BindDC(dc, &bind).is_err() {
                return;
            }
            rt.BeginDraw();
            rt.PushAxisAlignedClip(&rect_f(clip), D2D1_ANTIALIAS_MODE_ALIASED);
            for a in &all {
                let editing = typing && current.is_some_and(|c| std::ptr::eq(c, *a));
                let _ = self.draw_annot(&rt, a, shift, editing);
            }
            rt.PopAxisAlignedClip();
            if let Some((bar, tool, color_idx)) = bar {
                let _ = self.draw_toolbar(&rt, bar, tool, color_idx);
            }
            let _ = rt.EndDraw(None, None);
        }
    }

    fn brush(&self, rt: &ID2D1DCRenderTarget, c: D2D1_COLOR_F) -> Result<ID2D1SolidColorBrush> {
        unsafe { rt.CreateSolidColorBrush(&c, None) }
    }

    fn draw_annot(&self, rt: &ID2D1DCRenderTarget, a: &Annot, shift: POINT, editing: bool) -> Result<()> {
        let rgb = COLORS[a.color % COLORS.len()];
        let (p0, p1) = (pt(a.a, shift), pt(a.b, shift));
        unsafe {
            match a.tool {
                Tool::Blur => {}
                Tool::Rect => {
                    let r = norm(a.a, a.b);
                    let r = RECT {
                        left: r.left + shift.x,
                        top: r.top + shift.y,
                        right: r.right + shift.x,
                        bottom: r.bottom + shift.y,
                    };
                    rt.DrawRoundedRectangle(
                        &D2D1_ROUNDED_RECT { rect: rect_f(r), radiusX: 3.0, radiusY: 3.0 },
                        &self.brush(rt, color(rgb, 1.0))?,
                        3.5,
                        None,
                    );
                }
                Tool::Marker => {
                    rt.DrawLine(p0, p1, &self.brush(rt, color(rgb, 0.4))?, 20.0, None);
                }
                Tool::Arrow => {
                    let brush = self.brush(rt, color(rgb, 1.0))?;
                    let (dx, dy) = (p1.X - p0.X, p1.Y - p0.Y);
                    let len = (dx * dx + dy * dy).sqrt();
                    if len < 1.0 {
                        return Ok(());
                    }
                    let (ux, uy) = (dx / len, dy / len);
                    let head = 18.0f32.min(len);
                    let base = Vector2 { X: p1.X - ux * head, Y: p1.Y - uy * head };
                    rt.DrawLine(p0, Vector2 { X: base.X + ux * 2.0, Y: base.Y + uy * 2.0 }, &brush, 4.0, None);
                    let (nx, ny) = (-uy * head * 0.45, ux * head * 0.45);
                    let geometry = self.d2d.CreatePathGeometry()?;
                    let sink = geometry.Open()?;
                    sink.BeginFigure(p1, D2D1_FIGURE_BEGIN_FILLED);
                    sink.AddLine(Vector2 { X: base.X + nx, Y: base.Y + ny });
                    sink.AddLine(Vector2 { X: base.X - nx, Y: base.Y - ny });
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    sink.Close()?;
                    rt.FillGeometry(&geometry, &brush, None);
                }
                Tool::Text => {
                    let format = crate::ui::fonts::format(&self.dwrite, 24.0, DWRITE_FONT_WEIGHT_BOLD)?;
                    let shown = if editing { format!("{}|", a.text) } else { a.text.clone() };
                    let wide: Vec<u16> = shown.encode_utf16().collect();
                    let rect = D2D_RECT_F { left: p0.X, top: p0.Y - 4.0, right: p0.X + 4000.0, bottom: p0.Y + 40.0 };
                    let shadow = D2D_RECT_F {
                        left: rect.left + 1.5,
                        top: rect.top + 1.5,
                        right: rect.right,
                        bottom: rect.bottom,
                    };
                    rt.DrawText(
                        &wide,
                        &format,
                        &shadow,
                        &self.brush(rt, color((0.0, 0.0, 0.0), 0.55))?,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    rt.DrawText(
                        &wide,
                        &format,
                        &rect,
                        &self.brush(rt, color(rgb, 1.0))?,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            }
        }
        Ok(())
    }

    fn glyph(
        &self,
        rt: &ID2D1DCRenderTarget,
        text: &str,
        r: RECT,
        size: f32,
        weight: DWRITE_FONT_WEIGHT,
        c: D2D1_COLOR_F,
    ) -> Result<()> {
        unsafe {
            let family = if text.is_ascii() { w!("Segoe UI") } else { w!("Segoe UI Symbol") };
            let format = self.dwrite.CreateTextFormat(
                family,
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-us"),
            )?;
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            let wide: Vec<u16> = text.encode_utf16().collect();
            rt.DrawText(
                &wide,
                &format,
                &rect_f(r),
                &self.brush(rt, c)?,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
        Ok(())
    }

    fn draw_toolbar(&self, rt: &ID2D1DCRenderTarget, bar: &Toolbar, active: Tool, active_color: usize) -> Result<()> {
        let white = |a: f32| color((1.0, 1.0, 1.0), a);
        unsafe {
            let plate = D2D1_ROUNDED_RECT { rect: rect_f(bar.rect), radiusX: 14.0, radiusY: 14.0 };
            rt.FillRoundedRectangle(&plate, &self.brush(rt, color((0.07, 0.07, 0.08), 0.94))?);
            rt.DrawRoundedRectangle(&plate, &self.brush(rt, white(0.14))?, 1.0, None);
        }
        for (item, r) in &bar.items {
            let (cx, cy) = ((r.left + r.right) as f32 / 2.0, (r.top + r.bottom) as f32 / 2.0);
            let center = Vector2 { X: cx, Y: cy };
            unsafe {
                match item {
                    Item::Tool(t) => {
                        if *t == active {
                            rt.FillRoundedRectangle(
                                &D2D1_ROUNDED_RECT { rect: rect_f(*r), radiusX: 9.0, radiusY: 9.0 },
                                &self.brush(rt, white(0.2))?,
                            );
                        }
                        let ink = self.brush(rt, white(0.95))?;
                        let v = |dx: f32, dy: f32| Vector2 { X: cx + dx, Y: cy + dy };
                        match t {
                            Tool::Arrow => {
                                rt.DrawLine(v(-8.0, 8.0), v(6.0, -6.0), &ink, 2.4, None);
                                rt.DrawLine(v(6.0, -6.0), v(-1.0, -6.0), &ink, 2.4, None);
                                rt.DrawLine(v(6.0, -6.0), v(6.0, 1.0), &ink, 2.4, None);
                            }
                            Tool::Rect => rt.DrawRoundedRectangle(
                                &D2D1_ROUNDED_RECT {
                                    rect: D2D_RECT_F {
                                        left: cx - 8.0,
                                        top: cy - 6.0,
                                        right: cx + 8.0,
                                        bottom: cy + 6.0,
                                    },
                                    radiusX: 2.0,
                                    radiusY: 2.0,
                                },
                                &ink,
                                2.2,
                                None,
                            ),
                            Tool::Marker => rt.DrawLine(
                                v(-8.0, 5.0),
                                v(8.0, -5.0),
                                &self.brush(rt, color((1.0, 0.85, 0.2), 0.8))?,
                                9.0,
                                None,
                            ),
                            Tool::Text => {
                                self.glyph(rt, "T", *r, 18.0, DWRITE_FONT_WEIGHT_BOLD, white(0.95))?;
                            }
                            Tool::Blur => {
                                for (i, j) in [(0, 0), (1, 1), (2, 0), (0, 2), (2, 2)] {
                                    let (x, y) = (cx - 9.0 + i as f32 * 6.5, cy - 9.0 + j as f32 * 6.5);
                                    rt.FillRectangle(
                                        &D2D_RECT_F { left: x, top: y, right: x + 5.0, bottom: y + 5.0 },
                                        &ink,
                                    );
                                }
                            }
                        }
                    }
                    Item::Color(i) => {
                        rt.FillEllipse(
                            &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                                point: center,
                                radiusX: 9.0,
                                radiusY: 9.0,
                            },
                            &self.brush(rt, color(COLORS[*i], 1.0))?,
                        );
                        if *i == active_color {
                            rt.DrawEllipse(
                                &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                                    point: center,
                                    radiusX: 12.0,
                                    radiusY: 12.0,
                                },
                                &self.brush(rt, white(0.95))?,
                                2.0,
                                None,
                            );
                        }
                    }
                    Item::Undo => self.glyph(rt, "\u{21B6}", *r, 20.0, DWRITE_FONT_WEIGHT_NORMAL, white(0.9))?,
                    Item::Confirm => {
                        self.glyph(rt, "\u{2713}", *r, 20.0, DWRITE_FONT_WEIGHT_BOLD, color((0.4, 0.9, 0.55), 1.0))?
                    }
                    Item::Cancel => {
                        self.glyph(rt, "\u{2715}", *r, 16.0, DWRITE_FONT_WEIGHT_NORMAL, color((1.0, 0.45, 0.45), 1.0))?
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn burn(renderer: &Renderer, grab_dc: HDC, sel: RECT, annots: &[Annot]) -> Option<(Vec<u8>, i32, i32)> {
    use windows::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS};
    let (w, h) = (sel.right - sel.left, sel.bottom - sel.top);
    if w <= 0 || h <= 0 {
        return None;
    }
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0);
        ReleaseDC(None, screen);
        let result = bitmap.ok().filter(|_| !bits.is_null()).map(|bitmap| {
            let old = SelectObject(dc, bitmap.into());
            let _ = BitBlt(dc, 0, 0, w, h, Some(grab_dc), sel.left, sel.top, SRCCOPY);
            let local = RECT { left: 0, top: 0, right: w, bottom: h };
            renderer.render(dc, local, POINT { x: -sel.left, y: -sel.top }, grab_dc, annots, None, false, local, None);
            let mut out = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize).to_vec();
            for px in out.chunks_exact_mut(4) {
                px[3] = 255;
            }
            SelectObject(dc, old);
            let _ = DeleteObject(bitmap.into());
            (out, w, h)
        });
        let _ = DeleteDC(dc);
        result
    }
}
