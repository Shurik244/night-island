use std::cell::{Cell, RefCell};
use std::sync::Once;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_METRICS,
};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetMonitorInfoW, HBITMAP, HDC,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, ReleaseDC, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HTTRANSPARENT, HWND_TOPMOST, KillTimer, MA_NOACTIVATE, RegisterClassW, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SetTimer, SetWindowPos, ShowWindow, ULW_ALPHA, UpdateLayeredWindow,
    WM_MOUSEACTIVATE, WM_NCHITTEST, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{Result, w};

use crate::ui::gfx::{Gpu, rgba};
use crate::ui::theme;

const FADE_TIMER: usize = 1;
const FADE_STEP_MS: u32 = 15;
const FADE_STEPS: u32 = 8;

struct Shown {
    mem: HDC,
    dib: HBITMAP,
    size: (i32, i32),
    step: u32,
}

thread_local! {
    static TIP: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
    static SHOWN: RefCell<Option<Shown>> = const { RefCell::new(None) };
}

fn free(shown: Shown) {
    unsafe {
        let _ = DeleteDC(shown.mem);
        let _ = DeleteObject(shown.dib.into());
    }
}

fn present(hwnd: HWND, shown: &Shown) {
    unsafe {
        let t = (shown.step as f32 / FADE_STEPS as f32).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t) * (1.0 - t);
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: (eased * 255.0) as u8,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let screen = GetDC(None);
        let _ = UpdateLayeredWindow(
            hwnd,
            Some(screen),
            None,
            Some(&SIZE { cx: shown.size.0, cy: shown.size.1 }),
            Some(shown.mem),
            Some(&POINT { x: 0, y: 0 }),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        ReleaseDC(None, screen);
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_TIMER if wparam.0 == FADE_TIMER => {
            SHOWN.with_borrow_mut(|s| {
                if let Some(shown) = s {
                    shown.step += 1;
                    present(hwnd, shown);
                    if shown.step >= FADE_STEPS {
                        unsafe {
                            let _ = KillTimer(Some(hwnd), FADE_TIMER);
                        }
                    }
                }
            });
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn window() -> HWND {
    static CLASS: Once = Once::new();
    let tip = TIP.get();
    if !tip.0.is_null() {
        return tip;
    }
    unsafe {
        let Ok(instance) = GetModuleHandleW(None) else { return tip };
        CLASS.call_once(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                lpszClassName: w!("NightIsland.Tip"),
                ..Default::default()
            };
            RegisterClassW(&class);
        });
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            w!("NightIsland.Tip"),
            w!(""),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();
        TIP.set(hwnd);
        hwnd
    }
}

fn measure(gpu: &Gpu, text: &str, size: f32) -> Result<(f32, f32)> {
    unsafe {
        let format = crate::ui::fonts::format(gpu.dwrite(), size, DWRITE_FONT_WEIGHT_NORMAL)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = gpu.dwrite().CreateTextLayout(&wide, &format, 420.0 * size, 200.0 * size)?;
        let mut m = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut m)?;
        Ok((m.widthIncludingTrailingWhitespace.ceil(), m.height.ceil()))
    }
}

pub fn show(gpu: &Gpu, text: &str, at: (i32, i32), scale: f32) {
    let hwnd = window();
    if hwnd.0.is_null() {
        return;
    }
    let _ = paint(gpu, hwnd, text, at, scale);
}

fn paint(gpu: &Gpu, hwnd: HWND, text: &str, at: (i32, i32), scale: f32) -> Result<()> {
    let font = 12.5 * scale;
    let (tw, th) = measure(gpu, text, font)?;
    let (pad_x, pad_y) = (10.0 * scale, 6.0 * scale);
    let margin = (10.0 * scale).ceil();
    let body = (tw + 2.0 * pad_x, th + 2.0 * pad_y);
    let size = ((body.0 + 2.0 * margin).ceil() as i32, (body.1 + 2.0 * margin).ceil() as i32);
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.0,
                biHeight: -size.1,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0);
        ReleaseDC(None, screen);
        let dib = match dib {
            Ok(d) => d,
            Err(e) => {
                let _ = DeleteDC(mem);
                return Err(e);
            }
        };
        SelectObject(mem, dib.into());
        let painted = gpu.paint_dc(mem, size, |p| {
            let (r, g, b) = theme::panel();
            let lift = |c: f32| c + (1.0 - c) * 0.10;
            let rect = (margin, margin, margin + body.0, margin + body.1);
            let radius = 8.0 * scale;
            for i in 0..6 {
                let grow = i as f32 * 1.4 * scale;
                let a = 0.05 * (1.0 - i as f32 / 6.0);
                let _ = p.fill_round_rect(
                    (rect.0 - grow, rect.1 - grow + 2.0 * scale, rect.2 + grow, rect.3 + grow + 2.0 * scale),
                    radius + grow,
                    rgba(0.0, 0.0, 0.0, a),
                );
            }
            let _ = p.fill_round_rect(rect, radius, rgba(lift(r), lift(g), lift(b), 0.97));
            let _ = p.stroke_round_rect(rect, radius, 1.0, rgba(1.0, 1.0, 1.0, 0.12));
            let _ = p.text(
                text,
                rect,
                font,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                rgba(1.0, 1.0, 1.0, 0.92),
            );
        });
        if let Err(e) = painted {
            let _ = DeleteDC(mem);
            let _ = DeleteObject(dib.into());
            return Err(e);
        }
        let mut x = at.0 - size.0 / 2;
        let mut y = at.1 - margin as i32;
        let monitor = MonitorFromPoint(POINT { x: at.0, y: at.1 }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(monitor, &mut mi).as_bool() {
            let work = mi.rcWork;
            x = x.clamp(work.left, (work.right - size.0).max(work.left));
            y = y.clamp(work.top, (work.bottom - size.1).max(work.top));
        }
        let shown = Shown { mem, dib, size, step: 0 };
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
        present(hwnd, &shown);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(Some(hwnd), FADE_TIMER, FADE_STEP_MS, None);
        if let Some(old) = SHOWN.replace(Some(shown)) {
            free(old);
        }
    }
    Ok(())
}

pub fn hide() {
    let hwnd = TIP.get();
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = KillTimer(Some(hwnd), FADE_TIMER);
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    if let Some(old) = SHOWN.replace(None) {
        free(old);
    }
}
