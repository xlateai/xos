use fontdue::{Font, Metrics};

/// A single rendered glyph in pixel space.
#[derive(Debug)]
pub struct Character {
    pub ch: char,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub line_index: usize,
    pub char_index: usize,
    pub metrics: Metrics,
    pub bitmap: Vec<u8>,
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
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub font: Font,
}

impl GeometricText {
    pub fn new(font: Font, font_size: f32) -> Self {
        let metrics = font
            .horizontal_line_metrics(font_size)
            .expect("Font missing horizontal metrics");

        Self {
            text: String::new(),
            characters: vec![],
            lines: vec![],
            font_size,
            ascent: metrics.ascent,
            descent: metrics.descent,
            line_gap: metrics.line_gap,
            font,
        }
    }

    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }

    pub fn tick(&mut self, window_width: f32, _window_height: f32) {
        self.characters.clear();
        self.lines.clear();

        let mut x = 0.0;
        let mut y = 0.0;
        let mut line_start = 0;
        let mut line_index = 0;

        let text = self.text.clone();

        for (i, ch) in text.chars().enumerate() {
            let (metrics, bitmap) = self.font.rasterize(ch, self.font_size);
            let advance = metrics.advance_width;

            if x + advance > window_width {
                self.lines.push(LineInfo {
                    baseline_y: y,
                    start_index: line_start,
                    end_index: i,
                });

                x = 0.0;
                y += self.ascent + self.descent + self.line_gap;
                line_start = i;
                line_index += 1;
            }

            self.characters.push(Character {
                ch,
                x,
                y,
                width: metrics.width as f32,
                height: metrics.height as f32,
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
