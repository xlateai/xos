use fontdue::{Font, FontSettings};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFamily {
    JetBrainsMono = 0,
    NotoSansMedium = 1,
    NotoSansJp = 2,
    DotGothic16 = 3,
}

impl FontFamily {
    pub const ALL: [FontFamily; 4] = [
        FontFamily::JetBrainsMono,
        FontFamily::NotoSansMedium,
        FontFamily::NotoSansJp,
        FontFamily::DotGothic16,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FontFamily::JetBrainsMono => "JetBrains Mono",
            FontFamily::NotoSansMedium => "Noto Sans Medium",
            FontFamily::NotoSansJp => "Noto Sans JP",
            FontFamily::DotGothic16 => "DotGothic16",
        }
    }
}

impl From<u8> for FontFamily {
    fn from(value: u8) -> Self {
        match value {
            1 => FontFamily::NotoSansMedium,
            2 => FontFamily::NotoSansJp,
            3 => FontFamily::DotGothic16,
            _ => FontFamily::JetBrainsMono,
        }
    }
}

static DEFAULT_FONT_FAMILY: AtomicU8 = AtomicU8::new(FontFamily::DotGothic16 as u8);
static DEFAULT_FONT_VERSION: AtomicU64 = AtomicU64::new(1);


pub fn jetbrains_mono() -> Font {
    let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
    return Font::from_bytes(font_bytes, FontSettings::default()).unwrap();
}

pub fn noto_sans_jp() -> Font {
    let font_bytes = include_bytes!("../../assets/NotoSansJP-Regular.ttf") as &[u8];
    Font::from_bytes(font_bytes, FontSettings::default()).unwrap()
}

pub fn noto_sans_medium() -> Font {
    let font_bytes = include_bytes!("../../assets/NotoSans-Medium.ttf") as &[u8];
    Font::from_bytes(font_bytes, FontSettings::default()).unwrap()
}

pub fn dot_gothic_16() -> Font {
    let font_bytes = include_bytes!("../../assets/DotGothic16-Regular.ttf") as &[u8];
    Font::from_bytes(font_bytes, FontSettings::default()).unwrap()
}

pub fn default_font_family() -> FontFamily {
    FontFamily::from(DEFAULT_FONT_FAMILY.load(Ordering::Relaxed))
}

pub fn default_font_name() -> &'static str {
    default_font_family().label()
}

pub fn set_default_font_family(family: FontFamily) {
    let prev = default_font_family();
    if prev != family {
        DEFAULT_FONT_FAMILY.store(family as u8, Ordering::Relaxed);
        DEFAULT_FONT_VERSION.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn default_font_version() -> u64 {
    DEFAULT_FONT_VERSION.load(Ordering::Relaxed)
}

pub fn default_font() -> Font {
    match default_font_family() {
        FontFamily::JetBrainsMono => jetbrains_mono(),
        FontFamily::NotoSansMedium => noto_sans_medium(),
        FontFamily::NotoSansJp => noto_sans_jp(),
        FontFamily::DotGothic16 => dot_gothic_16(),
    }
}
