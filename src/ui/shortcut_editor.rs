use crate::data::desktop_dock::Pin;
use crate::ui::{
    gfx::{Gpu, rgba},
    i18n::tr,
};
use std::cell::Cell;
use std::sync::Once;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, EM_SETLIMITTEXT, ODS_FOCUS, ODS_SELECTED};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, Result, w};

struct State {
    gpu: Gpu,
    fields: [HWND; 3],
    brush: HBRUSH,
    result: Option<Pin>,
    done: Cell<bool>,
    picker: fn(HWND) -> Option<String>,
    scale: f32,
}

fn text(hwnd: HWND) -> String {
    let mut buffer = [0u16; 4096];
    let len = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..len])
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        let state = &mut *ptr;
        match msg {
            WM_CLOSE => {
                state.done.set(true);
                LRESULT(0)
            }
            WM_COMMAND => {
                match wp.0 & 0xffff {
                    1 => {
                        let name = text(state.fields[0]);
                        let target = text(state.fields[1]);
                        if !target.trim().is_empty() {
                            state.result = Some(Pin {
                                name,
                                target: target.trim().into(),
                                icon: text(state.fields[2]).trim().into(),
                            });
                            state.done.set(true);
                        } else {
                            let _ = SetFocus(Some(state.fields[1]));
                        }
                    }
                    2 => state.done.set(true),
                    10 | 11 => {
                        if let Some(path) = (state.picker)(hwnd) {
                            let index = if wp.0 & 0xffff == 10 { 1 } else { 2 };
                            let _ = SetWindowTextW(state.fields[index], &HSTRING::from(path));
                        }
                    }
                    12 => {
                        let _ = SetWindowTextW(state.fields[2], w!(""));
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_CTLCOLOREDIT => {
                let dc = HDC(wp.0 as _);
                SetTextColor(dc, COLORREF(0xeeeeee));
                SetBkColor(dc, COLORREF(0x202020));
                LRESULT(state.brush.0 as isize)
            }
            WM_DRAWITEM => {
                let item = &*(lp.0 as *const DRAWITEMSTRUCT);
                let mut label = [0u16; 128];
                let len = GetWindowTextW(item.hwndItem, &mut label) as usize;
                let text = String::from_utf16_lossy(&label[..len]);
                let rect = item.rcItem;
                let _ = state.gpu.paint_dc(item.hDC, (rect.right - rect.left, rect.bottom - rect.top), |p| {
                    let width = (rect.right - rect.left) as f32;
                    let height = (rect.bottom - rect.top) as f32;
                    let _ = p.fill_round_rect(
                        (0.0, 0.0, width, height),
                        9.0 * state.scale,
                        if item.CtlID == 1 { rgba(0.35, 0.38, 0.55, 1.0) } else { rgba(0.14, 0.14, 0.16, 1.0) },
                    );
                    if item.itemState.0 & ODS_SELECTED.0 != 0 {
                        let _ = p.fill_round_rect(
                            (1.0, 1.0, width - 1.0, height - 1.0),
                            8.0 * state.scale,
                            rgba(1.0, 1.0, 1.0, 0.08),
                        );
                    }
                    if item.itemState.0 & ODS_FOCUS.0 != 0 {
                        let _ = p.stroke_round_rect(
                            (1.0, 1.0, width - 1.0, height - 1.0),
                            8.0 * state.scale,
                            state.scale,
                            rgba(0.75, 0.78, 1.0, 0.9),
                        );
                    }
                    let _ = p.text(
                        &text,
                        (4.0, 0.0, width - 4.0, height),
                        12.0 * state.scale,
                        windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL,
                        windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_CENTER,
                        rgba(0.95, 0.95, 0.97, 1.0),
                    );
                });
                LRESULT(1)
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut paint);
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let s = state.scale;
                let _ = state.gpu.paint_dc(dc, (rect.right, rect.bottom), |p| {
                    let _ = p.fill_round_rect(
                        (0.0, 0.0, rect.right as f32, rect.bottom as f32),
                        0.0,
                        rgba(0.04, 0.04, 0.045, 1.0),
                    );
                    for (label, y) in [
                        ("Name", 18.0),
                        ("Application or file", 86.0),
                        ("Custom icon (PNG, ICO or application)", 154.0),
                        ("Leave the icon empty to use the original", 222.0),
                    ] {
                        let _ = p.text(
                            &tr(label),
                            (22.0 * s, y * s, 492.0 * s, (y + 24.0) * s),
                            12.0 * s,
                            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL,
                            windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_LEADING,
                            rgba(0.75, 0.75, 0.8, 1.0),
                        );
                    }
                });
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

pub fn show(owner: HWND, gpu: &Gpu, pin: &Pin, picker: fn(HWND) -> Option<String>) -> Result<Option<Pin>> {
    unsafe {
        static CLASS: Once = Once::new();
        let instance = GetModuleHandleW(None)?;
        CLASS.call_once(|| {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("NightIsland.ShortcutEditor"),
                ..Default::default()
            });
        });
        let scale = windows::Win32::UI::HiDpi::GetDpiForWindow(owner).max(96) as f32 / 96.0;
        let mut area = RECT::default();
        let _ = GetWindowRect(owner, &mut area);
        let monitor = crate::platform::desktop_dock::monitors().into_iter().find(|m| m.primary).unwrap();
        let width = (520.0 * scale) as i32;
        let height = (340.0 * scale) as i32;
        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            w!("NightIsland.ShortcutEditor"),
            &HSTRING::from(tr("Edit dock shortcut").as_ref()),
            WS_CAPTION | WS_SYSMENU,
            monitor.work.left + (monitor.work.right - monitor.work.left - width) / 2,
            monitor.work.top + (monitor.work.bottom - monitor.work.top - height) / 2,
            width,
            height,
            Some(owner),
            None,
            Some(instance.into()),
            None,
        )?;
        let dark = 1i32;
        let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(20), (&raw const dark).cast(), 4);
        let brush = CreateSolidBrush(COLORREF(0x202020));
        let mut state = Box::new(State {
            gpu: gpu.popup(),
            fields: [HWND::default(); 3],
            brush,
            result: None,
            done: Cell::new(false),
            picker,
            scale,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&raw mut *state) as isize);
        let font = CreateFontW(
            -(16.0 * scale) as i32,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            crate::ui::fonts::native_family(),
        );
        let build = (|| -> Result<()> {
            for (i, value) in [&pin.name, &pin.target, &pin.icon].into_iter().enumerate() {
                let field = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("EDIT"),
                    &HSTRING::from(value),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(0x80),
                    (22.0 * scale) as i32,
                    ((44.0 + i as f32 * 68.0) * scale) as i32,
                    ((if i == 0 { 466.0 } else { 370.0 }) * scale) as i32,
                    (30.0 * scale) as i32,
                    Some(hwnd),
                    Some(HMENU((30 + i) as _)),
                    Some(instance.into()),
                    None,
                )?;
                state.fields[i] = field;
                SendMessageW(field, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
                SendMessageW(field, EM_SETLIMITTEXT, Some(WPARAM(if i == 0 { 80 } else { 2048 })), None);
            }
            for (id, label, x, y, w) in [
                (10, "Browse…", 402.0, 112.0, 86.0),
                (11, "Browse…", 402.0, 180.0, 86.0),
                (12, "Reset icon", 22.0, 258.0, 120.0),
                (2, "Cancel", 272.0, 258.0, 100.0),
                (1, "Save", 382.0, 258.0, 106.0),
            ] {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("BUTTON"),
                    &HSTRING::from(tr(label).as_ref()),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(0xb),
                    (x * scale) as i32,
                    (y * scale) as i32,
                    (w * scale) as i32,
                    (32.0 * scale) as i32,
                    Some(hwnd),
                    Some(HMENU(id as _)),
                    Some(instance.into()),
                    None,
                )?;
            }
            Ok(())
        })();
        let owner_enabled = IsWindowEnabled(owner).as_bool();
        if build.is_ok() {
            let _ = EnableWindow(owner, false);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetFocus(Some(state.fields[0]));
            let mut message = MSG::default();
            while !state.done.get() {
                let status = GetMessageW(&mut message, None, 0, 0);
                if status.0 <= 0 {
                    if status.0 == 0 {
                        PostQuitMessage(message.wParam.0 as i32);
                    }
                    break;
                }
                if !IsDialogMessageW(hwnd, &message).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            let _ = EnableWindow(owner, owner_enabled);
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        let _ = DestroyWindow(hwnd);
        let _ = DeleteObject(font.into());
        let _ = DeleteObject(brush.into());
        build?;
        Ok(state.result.take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::UI::Composition::Compositor;
    use windows::Win32::System::WinRT::*;

    fn capture(hwnd: HWND) {
        let Some(folder) = std::env::var_os("NI_SHORTCUT_PREVIEW") else {
            return;
        };
        unsafe {
            let mut r = RECT::default();
            GetClientRect(hwnd, &mut r).unwrap();
            let screen = GetDC(Some(hwnd));
            let dc = CreateCompatibleDC(Some(screen));
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: r.right,
                    biHeight: -r.bottom,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
            SelectObject(dc, bitmap.into());
            BitBlt(dc, 0, 0, r.right, r.bottom, Some(screen), 0, 0, SRCCOPY).unwrap();
            let mut pixels = std::slice::from_raw_parts(bits.cast::<u8>(), (r.right * r.bottom * 4) as usize).to_vec();
            for p in pixels.chunks_exact_mut(4) {
                p.swap(0, 2);
                p[3] = 255;
            }
            let _ = DeleteDC(dc);
            let _ = DeleteObject(bitmap.into());
            ReleaseDC(Some(hwnd), screen);
            std::fs::create_dir_all(&folder).unwrap();
            let file = std::fs::File::create(std::path::PathBuf::from(folder).join("shortcut-editor.png")).unwrap();
            let mut encoder = png::Encoder::new(file, r.right as u32, r.bottom as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
    }

    #[test]
    #[ignore = "Opens an isolated shortcut editor and verifies save/cancel without launching files"]
    fn native_shortcut_editor_save_cancel() -> Result<()> {
        unsafe {
            RoInitialize(RO_INIT_SINGLETHREADED)?;
        }
        let queue = unsafe {
            CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_NONE,
            })?
        };
        let compositor = Compositor::new()?;
        let gpu = Gpu::new(&compositor)?;
        let owner = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Editor fixture"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            )?
        };
        for save in [true, false] {
            let worker = std::thread::spawn(move || unsafe {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                let hwnd = loop {
                    if let Ok(hwnd) = FindWindowW(w!("NightIsland.ShortcutEditor"), None)
                        && IsWindowVisible(hwnd).as_bool()
                    {
                        break hwnd;
                    }
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                };
                std::thread::sleep(std::time::Duration::from_millis(200));
                capture(hwnd);
                for (id, value) in [(30, "Custom browser"), (31, "C:/Browser.exe"), (32, "C:/icon.png")] {
                    let field = GetDlgItem(Some(hwnd), id).unwrap();
                    let _ = SetWindowTextW(field, &HSTRING::from(value));
                }
                let _ = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(if save { 1 } else { 2 }), LPARAM(0));
            });
            let result = show(owner, &gpu, &Pin::from_path("C:/Original.exe"), |_| None)?;
            worker.join().unwrap();
            if save {
                let pin = result.unwrap();
                assert_eq!(
                    (pin.name.as_str(), pin.target.as_str(), pin.icon.as_str()),
                    ("Custom browser", "C:/Browser.exe", "C:/icon.png")
                );
            } else {
                assert!(result.is_none());
            }
            assert!(unsafe { IsWindowEnabled(owner).as_bool() });
        }
        unsafe {
            DestroyWindow(owner)?;
        }
        drop(gpu);
        drop(compositor);
        drop(queue);
        unsafe {
            RoUninitialize();
        }
        Ok(())
    }
}
