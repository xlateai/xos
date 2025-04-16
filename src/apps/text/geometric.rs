use fontdue::{Font, Metrics};
use std::collections::HashMap;

/// A single rendered glyph in geometric space.
#[derive(Debug)]
pub struct Character {
    pub ch: char,
    pub normalized_x: f32,
    pub normalized_y: f32,
    pub width: f32,
    pub height: f32,
    pub line_index: usize,
    pub char_index: usize,
    pub metrics: &'static Metrics,
    pub bitmap: &'static [u8],
}

#[derive(Debug)]
pub struct LineInfo {
    pub baseline_y: f32,
    pub start_index: usize,
    pub end_index: usize,
}

pub struct GeometricText {
    pub text: String,
    pub characters: Vec<Character>,
    pub lines: Vec<LineInfo>,
    pub font_size: f32,
    pub aspect_ratio: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub font: Font,
    pub cache: HashMap<char, (Metrics, Vec<u8>)>,
}

impl GeometricText {
    pub fn new(font: Font, font_size: f32, aspect_ratio: f32) -> Self {
        let metrics = font
            .horizontal_line_metrics(font_size)
            .expect("Font missing horizontal metrics");

        Self {
            text: String::new(),
            characters: vec![],
            lines: vec![],
            font_size,
            aspect_ratio,
            ascent: metrics.ascent,
            descent: metrics.descent,
            line_gap: metrics.line_gap,
            font,
            cache: HashMap::new(),
        }
    }

    pub fn set_text(&mut self, text: String) {
        self.text = text;
        self.tick();
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
        self.aspect_ratio = aspect_ratio;
        self.tick();
    }

    fn get_metrics(&mut self, ch: char) -> &'static (Metrics, Vec<u8>) {
        use std::collections::hash_map::Entry;

        match self.cache.entry(ch) {
            Entry::Occupied(e) => unsafe { std::mem::transmute(e.into_mut()) },
            Entry::Vacant(v) => {
                let (metrics, bitmap) = self.font.rasterize(ch, self.font_size);
                let leaked: &'static (Metrics, Vec<u8>) = Box::leak(Box::new((metrics, bitmap)));
                v.insert(leaked.clone());
                leaked
            }
        }
    }

    pub fn tick(&mut self) {
        self.characters.clear();
        self.lines.clear();

        let width_limit = 1.0 * self.aspect_ratio;

        let mut x = 0.0;
        let mut y = 0.0;
        let mut line_start = 0;
        let mut line_index = 0;

        let text = self.text.clone();

        for (i, ch) in text.chars().enumerate() {
            let (metrics, bitmap) = self.get_metrics(ch);
            let advance = metrics.advance_width / (self.font_size * self.aspect_ratio);

            if x + advance > width_limit {
                self.lines.push(LineInfo {
                    baseline_y: y,
                    start_index: line_start,
                    end_index: i,
                });

                x = 0.0;
                y += (self.ascent + self.descent + self.line_gap) / self.font_size;
                line_start = i;
                line_index += 1;
            }

            self.characters.push(Character {
                ch,
                normalized_x: x,
                normalized_y: y,
                width: metrics.width as f32 / self.font_size,
                height: metrics.height as f32 / self.font_size,
                line_index,
                char_index: i,
                metrics,
                bitmap,
            });

            x += advance;
        }

        // Final line
        self.lines.push(LineInfo {
            baseline_y: y,
            start_index: line_start,
            end_index: self.text.len(),
        });
    }
}
