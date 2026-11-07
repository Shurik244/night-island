#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use windows::Win32::Graphics::DirectWrite::*;
use windows::core::{BOOL, Interface, PCWSTR, Result, w};

use crate::data::settings::InterfaceFont;

#[cfg(not(test))]
static SELECTED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

thread_local! {
    #[cfg(test)]
    static CHOICE: Cell<InterfaceFont> = const { Cell::new(InterfaceFont::Segoe) };
    static FONTS: RefCell<Option<Fonts>> = const { RefCell::new(None) };
}

static CUSTOM: Mutex<Option<Arc<[u8]>>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static IMPORTED: Mutex<Option<std::result::Result<ImportedFont, String>>> = Mutex::new(None);
pub const WM_FONT_READY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 102;
pub const WM_FONT_PICK: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 103;
const MAX_FONT_BYTES: u64 = 16 * 1024 * 1024;

struct CustomFont {
    collection: IDWriteFontCollection,
    name: Vec<u16>,
    data: Arc<[u8]>,
}

impl CustomFont {
    fn new(factory: &IDWriteFactory, loader: &IDWriteInMemoryFontFileLoader, data: Arc<[u8]>) -> Result<Self> {
        unsafe {
            let modern = factory.cast::<IDWriteFactory5>()?;
            let builder = modern.CreateFontSetBuilder()?;
            let file =
                loader.CreateInMemoryFontFileReference(factory, data.as_ptr().cast(), data.len() as u32, None)?;
            builder.AddFontFile(&file)?;
            let collection: IDWriteFontCollection =
                modern.CreateFontCollectionFromFontSet(&builder.CreateFontSet()?)?.cast()?;
            let names = collection.GetFontFamily(0)?.GetFamilyNames()?;
            let mut name = vec![0; names.GetStringLength(0)? as usize + 1];
            names.GetString(0, &mut name)?;
            Ok(Self { collection, name, data })
        }
    }
}

pub struct ImportedFont {
    pub path: String,
    pub data: Arc<[u8]>,
}

pub fn use_custom(data: Arc<[u8]>) {
    *CUSTOM.lock().unwrap_or_else(|e| e.into_inner()) = Some(data);
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

pub fn load_custom(path: &str) {
    if !path.is_empty()
        && let Ok(bytes) = read_font(std::path::Path::new(path))
    {
        use_custom(bytes);
    }
}

pub fn revision() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

pub fn custom_name() -> String {
    with_fonts(|fonts| {
        Ok(fonts
            .custom
            .as_ref()
            .map(|font| String::from_utf16_lossy(&font.name[..font.name.len() - 1]))
            .unwrap_or_default())
    })
    .unwrap_or_default()
}

fn read_font(path: &std::path::Path) -> std::result::Result<Arc<[u8]>, String> {
    use std::io::Read;
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or_default().to_ascii_lowercase();
    if !matches!(extension.as_str(), "ttf" | "otf" | "ttc") {
        return Err("Choose a TTF, OTF or TTC font file".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "Could not read the font file")?;
    let mut bytes = Vec::new();
    file.take(MAX_FONT_BYTES + 1).read_to_end(&mut bytes).map_err(|_| "Could not read the font file")?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_FONT_BYTES {
        return Err("Font files must be between 1 byte and 16 MB".into());
    }
    Ok(bytes.into())
}

fn import_file(path: &std::path::Path) -> std::result::Result<ImportedFont, String> {
    use std::hash::{Hash, Hasher};
    let data = read_font(path)?;
    let fonts = Fonts::new().map_err(|_| "Could not load the font")?;
    CustomFont::new(&fonts.factory, &fonts.loader, data.clone()).map_err(|_| "This file is not a supported font")?;
    let root = std::env::var_os("APPDATA").ok_or("Could not save the font")?;
    let folder = std::path::PathBuf::from(root).join("NightIsland").join("fonts");
    std::fs::create_dir_all(&folder).map_err(|_| "Could not save the font")?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    let saved = folder.join(format!("{:016x}.ttf", hasher.finish()));
    if !saved.exists() {
        let temporary = saved.with_extension("tmp");
        std::fs::write(&temporary, &data)
            .and_then(|_| std::fs::rename(&temporary, &saved))
            .map_err(|_| "Could not save the font")?;
    }
    Ok(ImportedFont { path: saved.to_string_lossy().into_owned(), data })
}

pub fn import_async(path: String, hwnd: windows::Win32::Foundation::HWND) {
    let target = hwnd.0 as usize;
    std::thread::spawn(move || {
        *IMPORTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(import_file(std::path::Path::new(&path)));
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(windows::Win32::Foundation::HWND(target as _)),
                WM_FONT_READY,
                Default::default(),
                Default::default(),
            );
        }
    });
}

pub fn take_import() -> Option<std::result::Result<ImportedFont, String>> {
    IMPORTED.lock().unwrap_or_else(|e| e.into_inner()).take()
}

pub fn download_url(choice: InterfaceFont) -> &'static str {
    match choice {
        InterfaceFont::SanFrancisco => "https://developer.apple.com/fonts/",
        InterfaceFont::Montserrat => "https://fonts.google.com/specimen/Montserrat",
        _ => "https://fonts.google.com/",
    }
}

struct Fonts {
    factory: IDWriteFactory,
    loader: IDWriteInMemoryFontFileLoader,
    montserrat: IDWriteFontCollection,
    sf: Option<PCWSTR>,
    custom: Option<CustomFont>,
    generation: u64,
    widths: HashMap<(InterfaceFont, String, u32, i32), f32>,
    native: Vec<windows::Win32::Foundation::HANDLE>,
    native_montserrat: bool,
    native_custom: bool,
}

impl Fonts {
    fn new() -> Result<Self> {
        unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let modern = factory.cast::<IDWriteFactory5>()?;
            let loader = modern.CreateInMemoryFontFileLoader()?;
            factory.RegisterFontFileLoader(&loader)?;
            let result = (|| {
                let builder = modern.CreateFontSetBuilder()?;
                for data in [
                    include_bytes!("../../assets/fonts/Montserrat-Regular.ttf").as_slice(),
                    include_bytes!("../../assets/fonts/Montserrat-SemiBold.ttf").as_slice(),
                ] {
                    let file = loader.CreateInMemoryFontFileReference(
                        &factory,
                        data.as_ptr().cast(),
                        data.len() as u32,
                        None,
                    )?;
                    builder.AddFontFile(&file)?;
                }
                let montserrat = modern.CreateFontCollectionFromFontSet(&builder.CreateFontSet()?)?.cast()?;
                let mut system = None;
                factory.GetSystemFontCollection(&mut system, false)?;
                let sf = system.and_then(|system| {
                    [w!("SF Pro Text"), w!("SF Pro Display"), w!("SF Pro")].into_iter().find(|name| {
                        let (mut index, mut exists) = (0, BOOL(0));
                        system.FindFamilyName(*name, &mut index, &mut exists).is_ok() && exists.as_bool()
                    })
                });
                Ok((montserrat, sf))
            })();
            match result {
                Ok((montserrat, sf)) => {
                    let data = CUSTOM.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    let custom = data.and_then(|data| CustomFont::new(&factory, &loader, data).ok());
                    Ok(Self {
                        factory,
                        loader,
                        montserrat,
                        sf,
                        custom,
                        generation: GENERATION.load(Ordering::Acquire),
                        widths: HashMap::new(),
                        native: Vec::new(),
                        native_montserrat: false,
                        native_custom: false,
                    })
                }
                Err(error) => {
                    let _ = factory.UnregisterFontFileLoader(&loader);
                    Err(error)
                }
            }
        }
    }

    fn format(&self, choice: InterfaceFont, size: f32, weight: DWRITE_FONT_WEIGHT) -> Result<IDWriteTextFormat> {
        let (name, collection) = match choice {
            InterfaceFont::Custom => self
                .custom
                .as_ref()
                .map_or((w!("Segoe UI"), None), |font| (PCWSTR(font.name.as_ptr()), Some(&font.collection))),
            InterfaceFont::Montserrat => (w!("Montserrat"), Some(&self.montserrat)),
            InterfaceFont::SanFrancisco => (self.sf.unwrap_or(w!("Segoe UI")), None),
            InterfaceFont::Segoe => (w!("Segoe UI"), None),
        };
        unsafe {
            self.factory.CreateTextFormat(
                name,
                collection,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-us"),
            )
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            let _ = self.factory.UnregisterFontFileLoader(&self.loader);
            for handle in &self.native {
                let _ = windows::Win32::Graphics::Gdi::RemoveFontMemResourceEx(*handle);
            }
        }
    }
}

fn with_fonts<T>(action: impl FnOnce(&mut Fonts) -> Result<T>) -> Result<T> {
    FONTS.with_borrow_mut(|fonts| {
        if fonts.as_ref().is_none_or(|fonts| fonts.generation != GENERATION.load(Ordering::Acquire)) {
            *fonts = Some(Fonts::new()?);
        }
        action(fonts.as_mut().unwrap())
    })
}

pub fn set(choice: InterfaceFont) {
    #[cfg(not(test))]
    SELECTED.store(choice as u8, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    CHOICE.set(choice);
}

pub fn selected() -> InterfaceFont {
    #[cfg(not(test))]
    {
        InterfaceFont::ALL
            [usize::from(SELECTED.load(std::sync::atomic::Ordering::Relaxed)).min(InterfaceFont::ALL.len() - 1)]
    }
    #[cfg(test)]
    {
        CHOICE.get()
    }
}

pub fn format(factory: &IDWriteFactory, size: f32, weight: DWRITE_FONT_WEIGHT) -> Result<IDWriteTextFormat> {
    if selected() != InterfaceFont::Segoe
        && let Ok(format) = with_fonts(|fonts| fonts.format(selected(), size, weight))
    {
        return Ok(format);
    }
    unsafe {
        factory.CreateTextFormat(
            w!("Segoe UI"),
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!("en-us"),
        )
    }
}

pub fn width(choice: InterfaceFont, text: &str, size: f32, weight: DWRITE_FONT_WEIGHT) -> f32 {
    with_fonts(|fonts| {
        let key = (choice, text.to_owned(), size.to_bits(), weight.0);
        if let Some(width) = fonts.widths.get(&key) {
            return Ok(*width);
        }
        let format = fonts.format(choice, size, weight)?;
        let wide: Vec<_> = text.encode_utf16().collect();
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe {
            fonts.factory.CreateTextLayout(&wide, &format, 100_000.0, 1000.0)?.GetMetrics(&mut metrics)?;
        }
        if fonts.widths.len() >= 256 {
            fonts.widths.clear();
        }
        fonts.widths.insert(key, metrics.widthIncludingTrailingWhitespace);
        Ok(metrics.widthIncludingTrailingWhitespace)
    })
    .unwrap_or_else(|_| text.chars().count() as f32 * size * 0.65)
}

pub fn sf_available() -> bool {
    with_fonts(|fonts| Ok(fonts.sf.is_some())).unwrap_or(false)
}

pub fn wrapped_height(text: &str, width: f32, size: f32, weight: DWRITE_FONT_WEIGHT) -> f32 {
    with_fonts(|fonts| {
        let format = fonts.format(selected(), size, weight)?;
        let wide: Vec<_> = text.encode_utf16().collect();
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe {
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
            fonts.factory.CreateTextLayout(&wide, &format, width.max(1.0), 100_000.0)?.GetMetrics(&mut metrics)?;
        }
        Ok(metrics.height.ceil())
    })
    .unwrap_or(size * 1.5)
}

pub fn native_family() -> PCWSTR {
    with_fonts(|fonts| {
        if selected() == InterfaceFont::Montserrat && !fonts.native_montserrat {
            for data in [
                include_bytes!("../../assets/fonts/Montserrat-Regular.ttf").as_slice(),
                include_bytes!("../../assets/fonts/Montserrat-SemiBold.ttf").as_slice(),
            ] {
                let mut count = 0;
                let handle = unsafe {
                    windows::Win32::Graphics::Gdi::AddFontMemResourceEx(
                        data.as_ptr().cast(),
                        data.len() as u32,
                        None,
                        &raw mut count,
                    )
                };
                if !handle.is_invalid() {
                    fonts.native.push(handle);
                    fonts.native_montserrat = true;
                }
            }
        }
        if selected() == InterfaceFont::Custom
            && let Some(custom) = &fonts.custom
            && !fonts.native_custom
        {
            let mut count = 0;
            let handle = unsafe {
                windows::Win32::Graphics::Gdi::AddFontMemResourceEx(
                    custom.data.as_ptr().cast(),
                    custom.data.len() as u32,
                    None,
                    &raw mut count,
                )
            };
            if !handle.is_invalid() {
                fonts.native.push(handle);
                fonts.native_custom = true;
            }
        }
        Ok(match selected() {
            InterfaceFont::Custom => fonts.custom.as_ref().map_or(w!("Segoe UI"), |font| PCWSTR(font.name.as_ptr())),
            InterfaceFont::Montserrat if fonts.native_montserrat => w!("Montserrat"),
            InterfaceFont::SanFrancisco => fonts.sf.unwrap_or(w!("Segoe UI")),
            _ => w!("Segoe UI"),
        })
    })
    .unwrap_or(w!("Segoe UI"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_fonts_are_validated_cached_and_reloaded_without_the_source() -> Result<()> {
        let folder = std::env::temp_dir().join(format!("night-island-font-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let source = folder.join("sample.ttf");
        std::fs::write(&source, include_bytes!("../../assets/fonts/Montserrat-Regular.ttf")).unwrap();
        let imported = import_file(&source).unwrap();
        std::fs::remove_file(&source).unwrap();
        assert!(std::path::Path::new(&imported.path).is_file());
        use_custom(imported.data);
        assert_eq!(custom_name(), "Montserrat");
        set(InterfaceFont::Custom);
        assert!(width(InterfaceFont::Custom, "Island 21:51", 13.0, DWRITE_FONT_WEIGHT_NORMAL) > 50.0);
        load_custom(&imported.path);
        assert_eq!(custom_name(), "Montserrat");
        std::fs::write(&source, b"not a font").unwrap();
        assert!(import_file(&source).is_err());
        assert_eq!(custom_name(), "Montserrat");
        std::fs::remove_file(source).unwrap();
        set(InterfaceFont::Segoe);
        Ok(())
    }

    #[test]
    fn bundled_fonts_resolve_without_installing_and_keep_both_alphabets() -> Result<()> {
        let fonts = Fonts::new()?;
        for choice in InterfaceFont::ALL {
            for weight in [DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD] {
                let format = fonts.format(choice, 15.0, weight)?;
                let mut name = vec![0; unsafe { format.GetFontFamilyNameLength() } as usize + 1];
                unsafe {
                    format.GetFontFamilyName(&mut name)?;
                }
                if choice == InterfaceFont::Montserrat {
                    assert_eq!(String::from_utf16_lossy(&name[..name.len() - 1]), "Montserrat");
                }
                let width = width(choice, "Привет, Island! 21:51", 15.0, weight);
                assert!(width > 90.0 && width < 250.0, "{choice:?}: {width}");
            }
        }
        set(InterfaceFont::Montserrat);
        assert_eq!(selected(), InterfaceFont::Montserrat);
        let saved = serde_json::to_string(&crate::data::settings::Settings {
            interface_font: selected(),
            ..Default::default()
        })
        .unwrap();
        let loaded: crate::data::settings::Settings = serde_json::from_str(&saved).unwrap();
        assert_eq!(loaded.interface_font, InterfaceFont::Montserrat);
        assert_eq!(
            serde_json::from_str::<crate::data::settings::Settings>("{}").unwrap().interface_font,
            InterfaceFont::Segoe
        );
        set(InterfaceFont::Segoe);
        Ok(())
    }
}
