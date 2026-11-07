use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
use windows::core::{Error, Result};

use super::gfx::{Gpu, Painter};

pub const POPUP_MARGIN: f32 = 10.0;

pub fn hit_panel(point: (f32, f32), size: (f32, f32), scale: f32) -> bool {
    let margin = POPUP_MARGIN * scale;
    if size.0 <= 2.0 * margin || size.1 <= 2.0 * margin {
        return false;
    }
    let radius = (28.0 * scale).min((size.0 - 2.0 * margin) / 2.0).min((size.1 - 2.0 * margin) / 2.0);
    let cx = point.0.clamp((margin + radius).min(size.0 - margin - radius), size.0 - margin - radius);
    let cy = point.1.clamp((margin + radius).min(size.1 - margin - radius), size.1 - margin - radius);
    (point.0 - cx).powi(2) + (point.1 - cy).powi(2) <= radius * radius
}

pub struct PopupSurface {
    dc: HDC,
    bitmap: HBITMAP,
    size: (i32, i32),
    target: std::cell::RefCell<Option<windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget>>,
}

impl PopupSurface {
    pub fn new(size: (i32, i32)) -> Result<Self> {
        unsafe {
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            if dc.0.is_null() {
                return Err(Error::from_thread());
            }
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
            let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    let _ = DeleteDC(dc);
                    return Err(error);
                }
            };
            SelectObject(dc, bitmap.into());
            Ok(Self { dc, bitmap, size, target: Default::default() })
        }
    }

    pub fn paint(&self, gpu: &Gpu, draw: impl FnOnce(&Painter) -> Result<()>) -> Result<()> {
        self.paint_size(gpu, self.size, draw)
    }

    pub fn fits(&self, size: (i32, i32)) -> bool {
        size.0 > 0 && size.1 > 0 && size.0 <= self.size.0 && size.1 <= self.size.1
    }

    pub fn paint_size(&self, gpu: &Gpu, size: (i32, i32), draw: impl FnOnce(&Painter) -> Result<()>) -> Result<()> {
        if !self.fits(size) {
            return Err(Error::empty());
        }
        let mut result = Ok(());
        let mut target = self.target.borrow_mut();
        if target.is_none() {
            *target = Some(gpu.create_dc_target()?);
        }
        if let Err(error) = gpu.paint_dc_target(target.as_ref().unwrap(), self.dc, size, |p| result = draw(p)) {
            target.take();
            return Err(error);
        }
        result
    }

    pub fn present(&self, hwnd: HWND, at: Option<POINT>, alpha: u8) -> Result<()> {
        self.present_size(hwnd, at, self.size, alpha)
    }

    pub fn present_size(&self, hwnd: HWND, at: Option<POINT>, size: (i32, i32), alpha: u8) -> Result<()> {
        if !self.fits(size) {
            return Err(Error::empty());
        }
        unsafe {
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: alpha,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(
                hwnd,
                None,
                at.as_ref().map(|p| p as *const _),
                Some(&SIZE { cx: size.0, cy: size.1 }),
                Some(self.dc),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }
    }

    #[cfg(test)]
    pub fn storage_id(&self) -> isize {
        self.bitmap.0 as isize
    }

    #[cfg(test)]
    pub fn pixels(&self) -> Vec<u8> {
        let mut bitmap = BITMAP::default();
        unsafe {
            let _ = GdiFlush();
            assert_ne!(GetObjectW(self.bitmap.into(), size_of::<BITMAP>() as i32, Some((&raw mut bitmap).cast())), 0);
            std::slice::from_raw_parts(bitmap.bmBits.cast(), (self.size.0 * self.size.1 * 4) as usize).to_vec()
        }
    }
}

impl Drop for PopupSurface {
    fn drop(&mut self) {
        self.target.get_mut().take();
        unsafe {
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(self.bitmap.into());
        }
    }
}

pub struct PopupBackdrop {
    hwnd: HWND,
    owner: HWND,
    _target: windows::UI::Composition::Desktop::DesktopWindowTarget,
    _root: windows::UI::Composition::ContainerVisual,
    sprite: windows::UI::Composition::SpriteVisual,
    geometry: windows::UI::Composition::CompositionRoundedRectangleGeometry,
}

impl PopupBackdrop {
    #[cfg(test)]
    pub fn window(&self) -> HWND {
        self.hwnd
    }

    pub fn new(owner: HWND, compositor: &windows::UI::Composition::Compositor) -> Result<Self> {
        use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::core::{Interface, w};
        static CLASS: std::sync::Once = std::sync::Once::new();
        unsafe extern "system" fn proc(
            hwnd: HWND,
            message: u32,
            w: windows::Win32::Foundation::WPARAM,
            l: windows::Win32::Foundation::LPARAM,
        ) -> windows::Win32::Foundation::LRESULT {
            match message {
                WM_NCHITTEST => windows::Win32::Foundation::LRESULT(HTTRANSPARENT as isize),
                WM_MOUSEACTIVATE => windows::Win32::Foundation::LRESULT(MA_NOACTIVATE as isize),
                WM_ERASEBKGND => windows::Win32::Foundation::LRESULT(1),
                _ => unsafe { DefWindowProcW(hwnd, message, w, l) },
            }
        }
        let instance = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None)? };
        CLASS.call_once(|| unsafe {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                lpszClassName: w!("NightIsland.PopupBackdrop"),
                ..Default::default()
            });
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                w!("NightIsland.PopupBackdrop"),
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
            )?
        };
        let create = || -> Result<Self> {
            let target =
                unsafe { compositor.cast::<ICompositorDesktopInterop>()?.CreateDesktopWindowTarget(hwnd, true)? };
            let root = compositor.CreateContainerVisual()?;
            root.SetRelativeSizeAdjustment(windows_numerics::Vector2 { X: 1.0, Y: 1.0 })?;
            let (sprite, geometry) =
                crate::ui::desktop_dock::create_backdrop(hwnd, compositor, &root).ok_or_else(Error::empty)?;
            target.SetRoot(&root)?;
            Ok(Self { hwnd, owner, _target: target, _root: root, sprite, geometry })
        };
        let result = create();
        if result.is_err() {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
        result
    }

    pub fn show(&self, origin: (i32, i32), size: (i32, i32), scale: f32, visible: bool) {
        use windows::Win32::UI::WindowsAndMessaging::*;
        let visible = visible && unsafe { IsWindowVisible(self.owner).as_bool() };
        let margin = POPUP_MARGIN * scale;
        let panel = windows_numerics::Vector2 { X: size.0 as f32 - 2.0 * margin, Y: size.1 as f32 - 2.0 * margin };
        let _ = self.sprite.SetOffset(windows_numerics::Vector3 { X: margin, Y: margin, Z: 0.0 });
        let _ = self.sprite.SetSize(panel);
        let _ = self.geometry.SetSize(panel);
        let _ = self.geometry.SetCornerRadius(windows_numerics::Vector2 { X: 28.0 * scale, Y: 28.0 * scale });
        unsafe {
            let region = CreateRoundRectRgn(
                (margin - 2.0).floor() as i32,
                (margin - 2.0).floor() as i32,
                (size.0 as f32 - margin + 3.0).ceil() as i32,
                (size.1 as f32 - margin + 3.0).ceil() as i32,
                (56.0 * scale + 4.0).ceil() as i32,
                (56.0 * scale + 4.0).ceil() as i32,
            );
            if SetWindowRgn(self.hwnd, Some(region), true) == 0 {
                let _ = DeleteObject(region.into());
            }
            let _ = SetWindowPos(
                self.hwnd,
                Some(self.owner),
                origin.0,
                origin.1,
                size.0,
                size.1,
                SWP_NOACTIVATE | if visible { SWP_SHOWWINDOW } else { SWP_HIDEWINDOW },
            );
        }
    }

    pub fn refresh(&self) {
        let _ = self
            .sprite
            .Compositor()
            .and_then(|c| c.CreateHostBackdropBrush())
            .and_then(|brush| self.sprite.SetBrush(&brush));
    }

    pub fn set_opacity(&self, opacity: f32) {
        let _ = self.sprite.SetOpacity(opacity.clamp(0.0, 1.0));
    }

    pub fn hide(&self) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                self.hwnd,
                windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
            );
        }
    }

    pub fn set_capture_hidden(&self, hidden: bool) {
        use windows::Win32::UI::WindowsAndMessaging::*;
        unsafe {
            let _ = SetWindowDisplayAffinity(self.hwnd, if hidden { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
        }
    }
}

impl Drop for PopupBackdrop {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.hwnd);
        }
    }
}
