use std::sync::Once;
use std::time::Instant;

use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{Compositor, ContainerVisual, InsetClip, LayerVisual, SpriteVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Interface, Result, h, w};
use windows_numerics::{Vector2, Vector3};

use super::State;
use crate::ui::gfx::{Gpu, Layer};

pub(super) struct Reveal {
    hwnd: HWND,
    compositor: Compositor,
    target: DesktopWindowTarget,
    root: ContainerVisual,
    content: LayerVisual,
    plate: Layer,
    bridge: SpriteVisual,
    clip: InsetClip,
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCHITTEST { LRESULT(HTTRANSPARENT as isize) } else { unsafe { DefWindowProcW(hwnd, msg, wp, lp) } }
}

impl Reveal {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let compositor = gpu.compositor()?;
        let root = compositor.CreateContainerVisual()?;
        let fill = Vector2 { X: 1.0, Y: 1.0 };
        root.SetRelativeSizeAdjustment(fill)?;
        let content = compositor.CreateLayerVisual()?;
        let clip = compositor.CreateInsetClip()?;
        content.SetClip(&clip)?;
        content.SetRelativeSizeAdjustment(fill)?;
        root.Children()?.InsertAtTop(&content)?;
        let plate = gpu.layer(&compositor, (1, 1))?;
        let bridge = compositor.CreateSpriteVisual()?;
        let shape = compositor.CreateRoundedRectangleGeometry()?;
        shape.SetSize(Vector2 { X: 1.0, Y: 1.0 })?;
        shape.SetCornerRadius(Vector2 { X: 0.5, Y: 0.5 })?;
        bridge.SetClip(&compositor.CreateGeometricClipWithGeometry(&shape)?)?;
        bridge.SetSize(Vector2 { X: 1.0, Y: 1.0 })?;
        content.Children()?.InsertAtTop(&bridge)?;
        content.Children()?.InsertAtTop(&plate.sprite)?;
        static CLASS: Once = Once::new();
        let instance = unsafe { GetModuleHandleW(None)? };
        CLASS.call_once(|| unsafe {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                lpszClassName: w!("NightIsland.PanelReveal"),
                ..Default::default()
            });
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
                w!("NightIsland.PanelReveal"),
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
        let target = unsafe {
            compositor
                .cast::<ICompositorDesktopInterop>()
                .and_then(|interop| interop.CreateDesktopWindowTarget(hwnd, true))
                .and_then(|target| {
                    target.SetRoot(&root)?;
                    Ok(target)
                })
        };
        match target {
            Ok(target) => Ok(Self { hwnd, compositor, target, root, content, plate, bridge, clip }),
            Err(error) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                Err(error)
            }
        }
    }

    pub fn animate(&mut self, state: &State, edge: f32, left: bool) -> Result<()> {
        self.plate.resize(state.size)?;
        let mut drawn = Ok(());
        self.plate.paint(&state.gpu, |p| drawn = state.draw(p))?;
        drawn?;
        let (r, g, b) = crate::ui::theme::panel_u8();
        self.bridge.SetBrush(&self.compositor.CreateColorBrushWithColor(windows::UI::Color {
            A: 255,
            R: r,
            G: g,
            B: b,
        })?)?;
        self.bridge.SetIsVisible(state.settings.liquid)?;
        self.opacity(state.ghost_alpha);
        let now = Instant::now();
        let travel = state.travel.remaining(now);
        let duration = state.remaining(now);
        let frames: Vec<_> = (0..=60)
            .map(|step| {
                let t = step as f32 / 60.0;
                let at = now + duration.mul_f32(t);
                let (x, y) = state.position_at(at);
                (
                    t,
                    x,
                    y,
                    state.travel.value(at),
                    state.boundary_at(at).map_or(edge, |b| b.0),
                    state.island_motion.as_ref().map_or(0.0, |m| m.progress.value(at)),
                )
            })
            .collect();
        let (w, height) = (state.size.0 as f32, state.size.1 as f32);
        let top = frames.iter().map(|f| f.2).fold(f32::INFINITY, f32::min).floor();
        let bottom = frames.iter().map(|f| f.2 + height).fold(f32::NEG_INFINITY, f32::max).ceil();
        let (start, end) = if left {
            (
                frames.iter().map(|f| f.1.min(f.4)).fold(edge, f32::min).floor(),
                frames.iter().map(|f| f.4).fold(edge, f32::max).ceil(),
            )
        } else {
            (
                frames.iter().map(|f| f.4).fold(edge, f32::min).floor(),
                frames.iter().map(|f| (f.1 + w).max(f.4)).fold(edge, f32::max).ceil(),
            )
        };
        let offset = self.compositor.CreateVector3KeyFrameAnimation()?;
        let scale = self.compositor.CreateVector3KeyFrameAnimation()?;
        self.plate.sprite.SetCenterPoint(Vector3 {
            X: if left { w - 27.0 * state.scale } else { 27.0 * state.scale },
            Y: height * 0.5,
            Z: 0.0,
        })?;
        let fade = self.compositor.CreateScalarKeyFrameAnimation()?;
        let neck_offset = self.compositor.CreateVector3KeyFrameAnimation()?;
        let neck_scale = self.compositor.CreateVector3KeyFrameAnimation()?;
        let clip_left = self.compositor.CreateScalarKeyFrameAnimation()?;
        let clip_right = self.compositor.CreateScalarKeyFrameAnimation()?;
        let linear = self.compositor.CreateCubicBezierEasingFunction(
            Vector2 { X: 1.0 / 3.0, Y: 1.0 / 3.0 },
            Vector2 { X: 2.0 / 3.0, Y: 2.0 / 3.0 },
        )?;
        for (t, x, y, progress, edge, parent_progress) in frames {
            let reach = 27.0 * state.scale;
            let inner = if left { (x + w - reach).min(edge) } else { (x + reach).max(edge) };
            let width = (inner - edge).abs();
            let reveal_neck = if travel.is_zero() {
                0.0
            } else {
                neck_height(progress, width, state.settings.island_gap as f32, state.scale)
            };
            let parent_neck = 20.0 * state.scale * (std::f32::consts::PI * parent_progress).sin().max(0.0);
            let neck = reveal_neck.max(parent_neck);
            clip_left.InsertKeyFrameWithEasingFunction(t, if left { 0.0 } else { edge - start }, &linear)?;
            clip_right.InsertKeyFrameWithEasingFunction(t, if left { end - edge } else { 0.0 }, &linear)?;
            fade.InsertKeyFrameWithEasingFunction(t, reveal_opacity(progress), &linear)?;
            let stretch = crate::ui::reveal::smooth_range(progress, 0.0, 1.0);
            scale.InsertKeyFrameWithEasingFunction(
                t,
                Vector3 { X: 0.86 + 0.14 * stretch, Y: 0.86 + 0.14 * stretch, Z: 1.0 },
                &linear,
            )?;
            offset.InsertKeyFrameWithEasingFunction(t, Vector3 { X: x - start, Y: y - top, Z: 0.0 }, &linear)?;
            neck_offset.InsertKeyFrameWithEasingFunction(
                t,
                Vector3 { X: inner.min(edge) - start, Y: y - top + (height - neck) / 2.0, Z: 0.0 },
                &linear,
            )?;
            neck_scale.InsertKeyFrameWithEasingFunction(t, Vector3 { X: width, Y: neck, Z: 1.0 }, &linear)?;
        }
        let duration = duration.max(std::time::Duration::from_millis(1));
        for animation in [&offset, &neck_offset, &neck_scale, &scale] {
            animation.SetDuration(duration.into())?;
        }
        fade.SetDuration(duration.into())?;
        clip_left.SetDuration(duration.into())?;
        clip_right.SetDuration(duration.into())?;
        self.clip.StartAnimation(h!("LeftInset"), &clip_left)?;
        self.clip.StartAnimation(h!("RightInset"), &clip_right)?;
        self.content.StartAnimation(h!("Opacity"), &fade)?;
        self.plate.sprite.StartAnimation(h!("Offset"), &offset)?;
        self.plate.sprite.StartAnimation(h!("Scale"), &scale)?;
        self.bridge.StartAnimation(h!("Offset"), &neck_offset)?;
        self.bridge.StartAnimation(h!("Scale"), &neck_scale)?;
        unsafe {
            let mut affinity = 0;
            let _ = GetWindowDisplayAffinity(state.hwnd, &mut affinity);
            self.capture_hidden(affinity == WDA_EXCLUDEFROMCAPTURE.0);
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                start as i32,
                top as i32,
                ((end - start) as i32).max(1),
                ((bottom - top) as i32).max(1),
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn seek(&self, progress: f32) -> Result<()> {
        for controller in [
            self.plate.sprite.TryGetAnimationController(h!("Offset"))?,
            self.plate.sprite.TryGetAnimationController(h!("Scale"))?,
            self.bridge.TryGetAnimationController(h!("Offset"))?,
            self.bridge.TryGetAnimationController(h!("Scale"))?,
            self.content.TryGetAnimationController(h!("Opacity"))?,
            self.clip.TryGetAnimationController(h!("LeftInset"))?,
            self.clip.TryGetAnimationController(h!("RightInset"))?,
        ] {
            controller.Pause()?;
            controller.SetProgress(progress)?;
        }
        Ok(())
    }

    pub fn opacity(&self, alpha: u8) {
        let _ = self.root.SetOpacity(f32::from(alpha) / 255.0);
    }

    pub fn capture_hidden(&self, hidden: bool) {
        unsafe {
            let _ = SetWindowDisplayAffinity(self.hwnd, if hidden { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
        }
    }
}

fn reveal_opacity(progress: f32) -> f32 {
    let t = (progress / 0.45).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn neck_height(progress: f32, width: f32, gap: f32, scale: f32) -> f32 {
    let taper = ((1.0 - progress) / 0.16).clamp(0.0, 1.0);
    let proximity = ((gap + 33.0 - width / scale) / 6.0).clamp(0.0, 1.0);
    25.0 * scale * taper * proximity
}

impl Drop for Reveal {
    fn drop(&mut self) {
        let _ = self.root.SetIsVisible(false);
        let _ = self.target.SetRoot(None::<&windows::UI::Composition::Visual>);
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liquid_bridge_does_not_stretch_across_the_old_island_edge() {
        for scale in [1.0, 1.5, 2.0] {
            assert!(neck_height(0.9, 47.0 * scale, 20.0, scale) > 0.0);
            assert_eq!(neck_height(0.9, 90.0 * scale, 20.0, scale), 0.0);
            assert_eq!(neck_height(1.0, 47.0 * scale, 20.0, scale), 0.0);
        }
    }
}
