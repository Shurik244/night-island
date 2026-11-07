use super::*;
use crate::ui::gfx::Gpu;
use crate::ui::pet_design::{PetAccessory, PetBody, PetDesign};
use crate::ui::pet_motion::Motion;
use crate::ui::popup::PopupSurface;
use windows::UI::Composition::Compositor;
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
    RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
};

#[test]
#[ignore = "Requires an isolated Windows Composition runtime"]
fn moving_pets_and_custom_ears_fit_the_attached_canvas() -> Result<()> {
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
    let surface = PopupSurface::new((188, 156))?;
    let mut designs: Vec<_> =
        Character::ALL.into_iter().filter(|ch| *ch != Character::None).map(|ch| (ch, PetDesign::default())).collect();
    designs.extend(
        [PetBody::Bear, PetBody::Cat, PetBody::Fox, PetBody::Rabbit, PetBody::Ghost, PetBody::Robot].into_iter().map(
            |body| {
                (
                    Character::Custom,
                    PetDesign {
                        body,
                        width: 1.15,
                        ear_size: 1.25,
                        tail: true,
                        accessory: PetAccessory::Scarf,
                        ..Default::default()
                    },
                )
            },
        ),
    );
    let mut clipped = Vec::new();
    for (character, design) in designs {
        for frame in 0..48 {
            let pose = Pose {
                t: frame as f32 * 0.65,
                look: ((frame as f32).sin(), (frame as f32).cos()),
                reaction: [
                    Reaction::Idle,
                    Reaction::Music,
                    Reaction::Done,
                    Reaction::Attention,
                    Reaction::File,
                    Reaction::Sleep,
                ][frame % 6],
                beat: 1.0,
                blink: false,
                accent: (0.6, 0.7, 1.0),
                motion: Motion {
                    behavior: super::super::pet_behavior::BehaviorPose {
                        walk: if frame / 12 == 1 { 1.0 } else { 0.0 },
                        stride: frame as f32 * 0.6,
                        facing: if frame % 2 == 0 { -1.0 } else { 1.0 },
                        sit: if frame / 12 >= 2 { 1.0 } else { 0.0 },
                        sleep: if frame / 12 == 3 { 1.0 } else { 0.0 },
                    },
                    mouth: if frame % 3 == 0 { 1.0 } else { 0.0 },
                    lean: if frame % 2 == 0 { 0.15 } else { -0.15 },
                    stretch: if frame % 2 == 0 { 0.1 } else { -0.08 },
                    ..Default::default()
                },
            };
            surface.paint(&gpu, |p| {
                let old = p.push_offset(14.0, 14.0);
                let result = draw_with_design(p, character, 160.0, &pose, Some(&design));
                p.set_transform(&old);
                result
            })?;
            let outside = surface
                .pixels()
                .chunks_exact(4)
                .enumerate()
                .filter(|(i, px)| {
                    let (x, y) = (i % 188, i / 188);
                    px[3] > 20 && (!(14..174).contains(&x) || !(14..134).contains(&y))
                })
                .count();
            if outside > 0 {
                clipped.push(format!("{character:?}/{:?} frame {frame}: {outside} pixels", design.body));
                break;
            }
        }
    }
    assert!(clipped.is_empty(), "{}", clipped.join("; "));
    if let Some(dir) = std::env::var_os("NI_COMPANION_PREVIEW") {
        save_behavior_preview(&gpu, &std::path::PathBuf::from(&dir))?;
        let gallery = PopupSurface::new((720, 150))?;
        gallery.paint(&gpu, |p| {
            p.fill_rect((0.0, 0.0, 720.0, 150.0), rgba(0.06, 0.06, 0.07, 1.0))?;
            for (index, body) in PetBody::ALL.into_iter().enumerate() {
                let design =
                    PetDesign { body, tail: matches!(body, PetBody::Cat | PetBody::Fox), ..Default::default() };
                let pose = Pose {
                    t: 1.0,
                    look: (0.1, 0.0),
                    reaction: Reaction::Idle,
                    blink: false,
                    beat: 0.0,
                    accent: (0.6, 0.7, 1.0),
                    motion: Default::default(),
                };
                let old = p.push_offset(index as f32 * 120.0, 12.0);
                let drawn = draw_with_design(p, Character::Custom, 120.0, &pose, Some(&design));
                p.set_transform(&old);
                drawn?;
                p.text(
                    body.name(),
                    (index as f32 * 120.0 + 30.0, 112.0, index as f32 * 120.0 + 115.0, 140.0),
                    13.0,
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL,
                    windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_LEADING,
                    rgba(1.0, 1.0, 1.0, 0.8),
                )?;
            }
            Ok(())
        })?;
        let mut pixels = gallery.pixels();
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let mut encoder = png::Encoder::new(
            std::fs::File::create(std::path::PathBuf::from(&dir).join("custom-bodies.png")).unwrap(),
            720,
            150,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        let folder = std::path::PathBuf::from(dir).join("animation");
        std::fs::create_dir_all(&folder).unwrap();
        let frame_surface = PopupSurface::new((600, 154))?;
        let fox = PetDesign {
            name: "Лисёнок".into(),
            body: PetBody::Fox,
            fur: [223, 133, 66],
            belly: [249, 227, 195],
            accent: [112, 170, 222],
            tail: true,
            accessory: PetAccessory::Scarf,
            ..Default::default()
        };
        let mut motion = crate::ui::pet_motion::PetMotion::default();
        for frame in 0..120 {
            let t = frame as f32 / 30.0;
            let pose_motion = motion.advance(
                ((t * 3.0).sin() * 0.65, (t * 2.0).cos() * 0.35),
                1.0 / 30.0,
                frame < 72,
                (72..92).contains(&frame),
                false,
            );
            let pose = Pose {
                t,
                reaction: Reaction::Idle,
                look: motion.look,
                beat: 0.0,
                blink: false,
                accent: (0.6, 0.7, 1.0),
                motion: Motion {
                    lid: if (36..42).contains(&frame) {
                        crate::ui::pet_motion::eyelid((42 - frame) as f32 / 30.0)
                    } else {
                        0.0
                    },
                    ..pose_motion
                },
            };
            frame_surface.paint(&gpu, |p| {
                p.fill_rect((0.0, 0.0, 600.0, 154.0), rgba(0.055, 0.059, 0.075, 1.0))?;
                for (i, (ch, label)) in [
                    (Character::Spark, "Spark"),
                    (Character::Cat, "Кот"),
                    (Character::Bear, "Медведь"),
                    (Character::Custom, "Свой питомец"),
                ]
                .into_iter()
                .enumerate()
                {
                    let x = i as f32 * 150.0;
                    let old = p.push_offset(x + 15.0, 18.0);
                    let result = draw_with_design(p, ch, 120.0, &pose, Some(&fox));
                    p.set_transform(&old);
                    result?;
                    p.text(
                        label,
                        (x, 116.0, x + 150.0, 142.0),
                        13.0,
                        DWRITE_FONT_WEIGHT_BOLD,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        rgba(0.9, 0.9, 0.94, 1.0),
                    )?;
                }
                Ok(())
            })?;
            let mut rgba = frame_surface.pixels();
            for px in rgba.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
            let mut encoder =
                png::Encoder::new(std::fs::File::create(folder.join(format!("{frame:03}.png"))).unwrap(), 600, 154);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
        }
    }

    Ok(())
}

fn save_behavior_preview(gpu: &Gpu, folder: &std::path::Path) -> Result<()> {
    use crate::ui::pet_behavior::{Activity, Environment, PetBehavior};
    let folder = folder.join("behavior-animation");
    std::fs::create_dir_all(&folder).unwrap();
    let surface = PopupSurface::new((600, 150))?;
    let mut pet = PetBehavior::new(42);
    let mut env = Environment {
        dt: 1.0 / 30.0,
        enabled: true,
        reduced_motion: false,
        engaged: false,
        held: false,
        position: [70.0, 0.0],
        bounds: [8.0, 0.0, 96.0, 0.0],
        scale: 1.0,
    };
    let mut stage = 0;
    let mut resting_frames = 0;
    let mut sleeping_frames = 0;
    let mut image = 0;
    for tick in 0..5000 {
        let frame = pet.advance(&env);
        if let Some(position) = frame.position {
            env.position = position;
        }
        if stage == 0 && frame.activity == Activity::Walking {
            stage = 1;
        }
        if stage == 1 && frame.activity == Activity::Sitting {
            stage = 2;
        }
        if stage == 2 {
            resting_frames += 1;
            if resting_frames > 45 {
                stage = 3;
            }
        }
        if stage == 3 && frame.activity == Activity::Sleeping {
            stage = 4;
        }
        if stage == 4 {
            sleeping_frames += 1;
            if sleeping_frames > 60 {
                break;
            }
        }
        if !matches!(stage, 1 | 2 | 4) {
            continue;
        }
        let pose = Pose {
            t: tick as f32 / 30.0,
            reaction: if stage == 4 { Reaction::Sleep } else { Reaction::Idle },
            look: (frame.pose.facing * frame.pose.walk * 0.6, 0.0),
            blink: false,
            accent: (0.6, 0.7, 1.0),
            beat: 0.0,
            motion: Motion { behavior: frame.pose, ..Default::default() },
        };
        surface.paint(gpu, |p| {
            p.fill_rect((0.0, 0.0, 600.0, 150.0), rgba(0.055, 0.059, 0.075, 1.0))?;
            for (i, character) in [Character::Spark, Character::Cat, Character::Bear].into_iter().enumerate() {
                let x = i as f32 * 200.0;
                let old = p.push_offset(x + env.position[0], 18.0);
                let result = draw_with_design(p, character, 96.0, &pose, None);
                p.set_transform(&old);
                result?;
                p.text(
                    match stage {
                        1 => "Гуляет",
                        2 => "Отдыхает",
                        _ => "Спит",
                    },
                    (x, 110.0, x + 200.0, 138.0),
                    13.0,
                    DWRITE_FONT_WEIGHT_BOLD,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                    rgba(0.9, 0.9, 0.94, 1.0),
                )?;
            }
            Ok(())
        })?;
        let mut rgba = surface.pixels();
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let mut encoder =
            png::Encoder::new(std::fs::File::create(folder.join(format!("{image:03}.png"))).unwrap(), 600, 150);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
        image += 1;
    }
    assert_eq!(sleeping_frames, 61);
    Ok(())
}
