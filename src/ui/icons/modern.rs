mod art;

use std::collections::HashMap;
use std::str::SplitAsciiWhitespace;

use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_BEZIER_SEGMENT, D2D1_COLOR_F, D2D1_FIGURE_BEGIN, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW,
    D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID, D2D1_LINE_JOIN_ROUND, D2D1_QUADRATIC_BEZIER_SEGMENT,
    D2D1_STROKE_STYLE_PROPERTIES, ID2D1Factory, ID2D1GeometrySink, ID2D1PathGeometry, ID2D1RenderTarget,
    ID2D1StrokeStyle,
};
use windows::core::{Error, Interface, Result};
use windows_numerics::{Matrix3x2, Vector2};

type Rect = (f32, f32, f32, f32);

#[cfg(test)]
pub(super) fn has_glyph(ch: char) -> bool {
    art::paths(ch).is_some()
}

#[derive(Default)]
pub(crate) struct Cache {
    factories: HashMap<usize, FactoryCache>,
}

struct FactoryCache {
    style: ID2D1StrokeStyle,
    glyphs: HashMap<char, Geometry>,
}

struct Geometry {
    outline: Option<ID2D1PathGeometry>,
    solid: Option<ID2D1PathGeometry>,
}

impl Cache {
    pub(crate) fn draw(
        &mut self,
        dc: &ID2D1RenderTarget,
        ch: char,
        rect: Rect,
        size: f32,
        color: D2D1_COLOR_F,
    ) -> Result<bool> {
        let Some((outline, solid)) = art::paths(ch) else { return Ok(false) };
        let factory: ID2D1Factory = unsafe { dc.GetFactory()? };
        let factory_id = factory.as_raw() as usize;
        let cached = match self.factories.entry(factory_id) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => entry.insert(FactoryCache {
                style: unsafe {
                    factory.CreateStrokeStyle(
                        &D2D1_STROKE_STYLE_PROPERTIES {
                            startCap: D2D1_CAP_STYLE_ROUND,
                            endCap: D2D1_CAP_STYLE_ROUND,
                            dashCap: D2D1_CAP_STYLE_ROUND,
                            lineJoin: D2D1_LINE_JOIN_ROUND,
                            miterLimit: 1.0,
                            dashStyle: D2D1_DASH_STYLE_SOLID,
                            dashOffset: 0.0,
                        },
                        None,
                    )?
                },
                glyphs: HashMap::new(),
            }),
        };
        let geometry = match cached.glyphs.entry(ch) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => entry.insert(Geometry {
                outline: path(&factory, outline, D2D1_FIGURE_BEGIN_HOLLOW)?,
                solid: path(&factory, solid, D2D1_FIGURE_BEGIN_FILLED)?,
            }),
        };

        let scale = size * 1.12 / 24.0;
        let x = (rect.0 + rect.2) / 2.0 - 12.0 * scale;
        let y = (rect.1 + rect.3) / 2.0 - 12.0 * scale;
        let local = Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: x, M32: y };
        unsafe {
            let brush = dc.CreateSolidColorBrush(&color, None)?;
            let mut original = Matrix3x2::default();
            dc.GetTransform(&mut original);
            dc.SetTransform(&(local * original));
            if let Some(solid) = &geometry.solid {
                dc.FillGeometry(solid, &brush, None);
            }
            if let Some(outline) = &geometry.outline {
                dc.DrawGeometry(outline, &brush, 2.0, &cached.style);
            }
            dc.SetTransform(&original);
        }
        Ok(true)
    }
}

fn number(tokens: &mut SplitAsciiWhitespace<'_>) -> Result<f32> {
    tokens.next().and_then(|token| token.parse().ok()).ok_or_else(Error::empty)
}

fn point(tokens: &mut SplitAsciiWhitespace<'_>) -> Result<Vector2> {
    Ok(Vector2 { X: number(tokens)?, Y: number(tokens)? })
}

fn rounded_rect(sink: &ID2D1GeometrySink, origin: Vector2, size: Vector2, r: f32, mode: D2D1_FIGURE_BEGIN) {
    let Vector2 { X: left, Y: top } = origin;
    let (right, bottom) = (left + size.X, top + size.Y);
    unsafe {
        sink.BeginFigure(Vector2 { X: left + r, Y: top }, mode);
        for (line, corner, end) in [
            ((right - r, top), (right, top), (right, top + r)),
            ((right, bottom - r), (right, bottom), (right - r, bottom)),
            ((left + r, bottom), (left, bottom), (left, bottom - r)),
            ((left, top + r), (left, top), (left + r, top)),
        ] {
            sink.AddLine(Vector2 { X: line.0, Y: line.1 });
            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                point1: Vector2 { X: corner.0, Y: corner.1 },
                point2: Vector2 { X: end.0, Y: end.1 },
            });
        }
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
    }
}

fn circle(sink: &ID2D1GeometrySink, center: Vector2, radius: f32, mode: D2D1_FIGURE_BEGIN) {
    let at = |x: f32, y: f32| Vector2 { X: center.X + x * radius, Y: center.Y + y * radius };
    let k = 0.552_284_8;
    unsafe {
        sink.BeginFigure(at(1.0, 0.0), mode);
        for (a, b, c) in [
            ((1.0, k), (k, 1.0), (0.0, 1.0)),
            ((-k, 1.0), (-1.0, k), (-1.0, 0.0)),
            ((-1.0, -k), (-k, -1.0), (0.0, -1.0)),
            ((k, -1.0), (1.0, -k), (1.0, 0.0)),
        ] {
            sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: at(a.0, a.1), point2: at(b.0, b.1), point3: at(c.0, c.1) });
        }
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
    }
}

fn path(factory: &ID2D1Factory, data: &str, mode: D2D1_FIGURE_BEGIN) -> Result<Option<ID2D1PathGeometry>> {
    if data.is_empty() {
        return Ok(None);
    }
    unsafe {
        let path = factory.CreatePathGeometry()?;
        let sink = path.Open()?;
        let mut tokens = data.split_ascii_whitespace();
        let mut open = false;
        while let Some(command) = tokens.next() {
            if matches!(command, "M" | "R" | "O") && open {
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                open = false;
            }
            match command {
                "M" => {
                    sink.BeginFigure(point(&mut tokens)?, mode);
                    open = true;
                }
                "L" if open => sink.AddLine(point(&mut tokens)?),
                "Q" if open => sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                    point1: point(&mut tokens)?,
                    point2: point(&mut tokens)?,
                }),
                "C" if open => sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: point(&mut tokens)?,
                    point2: point(&mut tokens)?,
                    point3: point(&mut tokens)?,
                }),
                "Z" if open => {
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    open = false;
                }
                "R" => rounded_rect(&sink, point(&mut tokens)?, point(&mut tokens)?, number(&mut tokens)?, mode),
                "O" => circle(&sink, point(&mut tokens)?, number(&mut tokens)?, mode),
                _ => return Err(Error::empty()),
            }
        }
        if open {
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
        }
        sink.Close()?;
        Ok(Some(path))
    }
}
