use fontdue::{Font, FontSettings};


pub fn jetbrains_mono() -> Font {
    let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
    return Font::from_bytes(font_bytes, FontSettings::default()).unwrap();
}

pub fn noto_sans_jp() -> Font {
    let font_bytes = include_bytes!("../../assets/NotoSansJP-Regular.ttf") as &[u8];
    Font::from_bytes(font_bytes, FontSettings::default()).unwrap()
}
