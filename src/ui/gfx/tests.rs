use super::*;

use windows::Win32::Foundation::E_INVALIDARG;
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
    RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
};
use windows::core::Error;

#[test]
#[ignore = "Requires an interactive Windows desktop and an isolated Composition runtime"]
fn weather_updates_a_region_and_recovers_after_a_failed_draw() -> Result<()> {
    struct Runtime;
    impl Drop for Runtime {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    unsafe { RoInitialize(RO_INIT_SINGLETHREADED)? };
    let _runtime = Runtime;
    let _queue = unsafe {
        CreateDispatcherQueueController(DispatcherQueueOptions {
            dwSize: size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_NONE,
        })?
    };
    let compositor = Compositor::new()?;
    let gpu = Gpu::new(&compositor)?;
    let layer = gpu.layer(&compositor, (160, 48))?;
    layer.paint(&gpu, |p| {
        p.fill_rect((0.0, 0.0, 160.0, 48.0), WHITE).unwrap();
    })?;
    let rect = (18.25, 10.75, 39.25, 31.75);
    layer.paint_region(&gpu, rect, |p| p.weather_svg("rain", rect, true))?;
    let failed = layer.paint_region(&gpu, rect, |_| Err(Error::from_hresult(E_INVALIDARG)));
    assert!(failed.is_err());
    layer.paint_region(&gpu, rect, |p| p.weather_svg("snow", rect, true))?;
    layer.paint_region(&gpu, (-20.0, -20.0, -10.0, -10.0), |_| panic!("an empty region must not draw"))?;
    layer.paint_region(&gpu, (150.5, 40.5, 180.0, 60.0), |p| p.fill_rect((150.0, 40.0, 180.0, 60.0), WHITE))?;
    Ok(())
}

#[test]
fn capsule_progress_fills_the_width_without_painting_outside_its_corners() -> Result<()> {
    use windows::Win32::Graphics::Direct2D::Common::{D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT};
    use windows::Win32::Graphics::Direct2D::{
        D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_CPU_READ, D2D1_BITMAP_OPTIONS_TARGET,
        D2D1_BITMAP_PROPERTIES1, D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_MAP_OPTIONS_READ,
    };
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

    unsafe {
        let mut d3d: Option<ID3D11Device> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_WARP,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut d3d),
            None,
            None,
        )?;
        let dxgi: IDXGIDevice = d3d.ok_or_else(Error::empty)?.cast()?;
        let dc = D2D1CreateDevice(&dxgi, None)?.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
            ..Default::default()
        };
        let target = dc.CreateBitmap(D2D_SIZE_U { width: 400, height: 48 }, None, 0, &props)?;
        let staging = dc.CreateBitmap(
            D2D_SIZE_U { width: 400, height: 48 },
            None,
            0,
            &D2D1_BITMAP_PROPERTIES1 {
                bitmapOptions: D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                ..props
            },
        )?;
        dc.SetTarget(&target);
        let rt: ID2D1RenderTarget = dc.cast()?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let weather = std::cell::RefCell::default();
        let vector_icons = RefCell::default();
        let painter = Painter {
            dc: &rt,
            dwrite: &dwrite,
            icons: w!("Segoe UI Symbol"),
            vector_icons: &vector_icons,
            weather: &weather,
        };
        let mut baseline = Vec::new();
        for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
            dc.BeginDraw();
            dc.Clear(Some(&rgba(0.05, 0.06, 0.08, 1.0)));
            painter.fill_capsule_progress((0.0, 0.0, 400.0, 48.0), fraction, (107, 151, 215))?;
            dc.EndDraw(None, None)?;
            staging.CopyFromBitmap(None, &target, None)?;
            let mapped = staging.Map(D2D1_MAP_OPTIONS_READ)?;
            let row = std::slice::from_raw_parts(mapped.bits.add(24 * mapped.pitch as usize), 400 * 4);
            let corner = std::slice::from_raw_parts(mapped.bits, 4);
            if baseline.is_empty() {
                baseline = row.to_vec();
            }
            let changed = row.chunks_exact(4).zip(baseline.chunks_exact(4)).filter(|(a, b)| a != b).count();
            assert!((changed as f32 - 400.0 * fraction).abs() <= 2.0, "{fraction}: {changed}");
            assert_eq!(corner, &baseline[..4]);
            staging.Unmap()?;

            if let Ok(dir) = std::env::var("NI_PROGRESS_PREVIEW_DIR") {
                dc.BeginDraw();
                let old = painter.push_offset(8.0, 9.0);
                crate::ui::characters::draw(
                    &painter,
                    crate::ui::characters::Character::Spark,
                    40.0,
                    &crate::ui::characters::Pose {
                        motion: Default::default(),
                        t: 0.0,
                        reaction: crate::ui::characters::Reaction::Idle,
                        look: (0.0, 0.0),
                        blink: false,
                        accent: (0.42, 0.59, 0.84),
                        beat: 0.0,
                    },
                )?;
                painter.set_transform(&old);
                painter.text(
                    "Night Island",
                    (60.0, 0.0, 270.0, 48.0),
                    14.0,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                    WHITE,
                )?;
                dc.EndDraw(None, None)?;
                staging.CopyFromBitmap(None, &target, None)?;
                let mapped = staging.Map(D2D1_MAP_OPTIONS_READ)?;
                let mut rgba = Vec::with_capacity(400 * 48 * 4);
                for y in 0..48 {
                    for pixel in
                        std::slice::from_raw_parts(mapped.bits.add(y * mapped.pitch as usize), 400 * 4).chunks_exact(4)
                    {
                        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                    }
                }
                staging.Unmap()?;
                std::fs::create_dir_all(&dir).unwrap();
                let file = std::fs::File::create(
                    std::path::Path::new(&dir).join(format!("fill-{:03}.png", (fraction * 100.0) as u32)),
                )
                .unwrap();
                let mut encoder = png::Encoder::new(file, 400, 48);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
            }
        }
        for attached_top in [false, true] {
            let body = if attached_top { (0.0, -16.0, 400.0, 48.0) } else { (0.0, 0.0, 400.0, 64.0) };
            for fraction in [0.0, 0.25, 0.5, 1.0] {
                dc.BeginDraw();
                dc.Clear(Some(&rgba(0.0, 0.0, 0.0, 1.0)));
                painter.fill_body_progress((0.0, 0.0, 400.0, 48.0), body, 16.0, fraction, (107, 151, 215))?;
                dc.EndDraw(None, None)?;
                staging.CopyFromBitmap(None, &target, None)?;
                let mapped = staging.Map(D2D1_MAP_OPTIONS_READ)?;
                let y = if attached_top { 0 } else { 47 };
                let row = std::slice::from_raw_parts(mapped.bits.add(y * mapped.pitch as usize), 400 * 4);
                let colored = row.chunks_exact(4).filter(|pixel| pixel[0] > 0 || pixel[1] > 0 || pixel[2] > 0).count();
                assert!(
                    (colored as f32 - 400.0 * fraction).abs() <= 2.0,
                    "Docked edge lost fill: {attached_top}, {fraction}, {colored}"
                );
                staging.Unmap()?;
            }
        }
        Ok(())
    }
}
