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
            descent: metrics.descent.abs(),
            line_gap: metrics.line_gap,
            font,
        }
    }

    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        self.font_size = font_size;
    
        if let Some(metrics) = self.font.horizontal_line_metrics(font_size) {
            self.ascent = metrics.ascent;
            self.descent = metrics.descent.abs();
            self.line_gap = metrics.line_gap;
        }
    }

    pub fn tick(&mut self, window_width: f32, _window_height: f32) {
        self.characters.clear();
        self.lines.clear();
    
        let mut x = 0.0;
        let mut baseline_y = self.ascent;
        let mut line_start = 0;
        let mut line_index = 0;
    
        let text = self.text.clone();
        let mut last_index = 0;
    
        for (i, ch) in text.chars().enumerate() {
            if ch == '\n' {
                // End current line on newline character
                self.lines.push(LineInfo {
                    baseline_y,
                    start_index: line_start,
                    end_index: i,
                });
    
                x = 0.0;
                baseline_y += self.ascent + self.descent + self.line_gap;
                line_start = i + 1;
                line_index += 1;
                continue;
            }
    
            let (metrics, bitmap) = self.font.rasterize(ch, self.font_size);
            let advance = metrics.advance_width;
    
            if x + advance > window_width {
                self.lines.push(LineInfo {
                    baseline_y,
                    start_index: line_start,
                    end_index: i,
                });
    
                x = 0.0;
                baseline_y += self.ascent + self.descent + self.line_gap;
                line_start = i;
                line_index += 1;
            }
    
            let y = baseline_y - metrics.height as f32 - metrics.ymin as f32;
    
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
            last_index = i;
        }
    
        self.lines.push(LineInfo {
            baseline_y,
            start_index: line_start,
            end_index: last_index + 1,
        });
    }
    
}
