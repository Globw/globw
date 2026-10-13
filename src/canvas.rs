//! A pixel buffer with the drawing primitives of the graphics window.
//! It knows nothing about windows, so it can be tested anywhere.

use font8x8::{UnicodeFonts, BASIC_FONTS, LATIN_FONTS};

const COLORS: [(&str, u32); 12] = [
    ("black", 0x000000),
    ("white", 0xFFFFFF),
    ("red", 0xFF0000),
    ("green", 0x00C000),
    ("blue", 0x0000FF),
    ("yellow", 0xFFFF00),
    ("orange", 0xFF8800),
    ("purple", 0x8800CC),
    ("pink", 0xFF88CC),
    ("cyan", 0x00FFFF),
    ("gray", 0x808080),
    ("brown", 0x8B4513),
];

/// A color name or `#rrggbb` → 0xRRGGBB.
pub fn parse_color(text: &str) -> Result<u32, String> {
    let lower = text.trim().to_lowercase();
    if let Some((_, c)) = COLORS.iter().find(|(name, _)| *name == lower) {
        return Ok(*c);
    }
    if let Some(hex) = lower.strip_prefix('#') {
        if hex.len() == 6 {
            if let Ok(c) = u32::from_str_radix(hex, 16) {
                return Ok(c);
            }
        }
    }
    Err(format!("unknown color \"{}\": use a name like \"red\" or a code like \"#ff8800\"", text))
}

/// Size of a character on screen: the 8×8 font drawn at double size.
pub const CHAR_SIZE: i64 = 16;

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub pixels: Vec<u32>,
    /// distance of the 3D surface drawn at each pixel; infinity where there is none
    pub depth: Vec<f32>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas { w, h, pixels: vec![0; w * h], depth: vec![f32::INFINITY; w * h] }
    }

    pub fn get(&self, x: usize, y: usize) -> u32 {
        self.pixels[y * self.w + x]
    }

    fn set(&mut self, x: i64, y: i64, c: u32) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.pixels[y as usize * self.w + x as usize] = c;
        }
    }

    pub fn clear(&mut self, c: u32) {
        self.pixels.iter_mut().for_each(|p| *p = c);
        self.depth.iter_mut().for_each(|d| *d = f32::INFINITY);
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, c: u32) {
        let x0 = (x.round() as i64).max(0);
        let y0 = (y.round() as i64).max(0);
        let x1 = ((x + w).round() as i64).min(self.w as i64);
        let y1 = ((y + h).round() as i64).min(self.h as i64);
        for py in y0..y1 {
            let row = py as usize * self.w;
            for px in x0..x1 {
                self.pixels[row + px as usize] = c;
            }
        }
    }

    pub fn circle(&mut self, cx: f64, cy: f64, r: f64, c: u32) {
        let (y0, y1) = ((cy - r).floor() as i64, (cy + r).ceil() as i64);
        let (x0, x1) = ((cx - r).floor() as i64, (cx + r).ceil() as i64);
        for py in y0.max(0)..=y1.min(self.h as i64 - 1) {
            for px in x0.max(0)..=x1.min(self.w as i64 - 1) {
                let (dx, dy) = (px as f64 - cx, py as f64 - cy);
                if dx * dx + dy * dy <= r * r {
                    self.set(px, py, c);
                }
            }
        }
    }

    /// Bresenham's line between the rounded endpoints.
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, c: u32) {
        let (mut x, mut y) = (x1.round() as i64, y1.round() as i64);
        let (xe, ye) = (x2.round() as i64, y2.round() as i64);
        let (dx, dy) = ((xe - x).abs(), -(ye - y).abs());
        let (sx, sy) = (if x < xe { 1 } else { -1 }, if y < ye { 1 } else { -1 });
        let mut err = dx + dy;
        loop {
            self.set(x, y, c);
            if x == xe && y == ye {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Text in the 8×8 font at double size, one 16×16 cell per character.
    pub fn text(&mut self, x: f64, y: f64, text: &str, c: u32) {
        let (x0, y0) = (x.round() as i64, y.round() as i64);
        for (k, ch) in text.chars().enumerate() {
            let glyph = BASIC_FONTS
                .get(ch)
                .or_else(|| LATIN_FONTS.get(ch))
                .or_else(|| BASIC_FONTS.get('?'))
                .unwrap_or([0; 8]);
            let cell = x0 + k as i64 * CHAR_SIZE;
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        let (px, py) = (cell + col * 2, y0 + row as i64 * 2);
                        for (ox, oy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            self.set(px + ox, py + oy, c);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: u32 = 0xFF0000;

    #[test]
    fn colors() {
        assert_eq!(parse_color("red"), Ok(RED));
        assert_eq!(parse_color("Red"), Ok(RED));
        assert_eq!(parse_color("#ff8800"), Ok(0xFF8800));
        assert_eq!(parse_color("#FF8800"), Ok(0xFF8800));
        assert_eq!(parse_color("white"), Ok(0xFFFFFF));
        assert_eq!(
            parse_color("nope"),
            Err("unknown color \"nope\": use a name like \"red\" or a code like \"#ff8800\"".to_string())
        );
        assert!(parse_color("#12345").is_err());
    }

    #[test]
    fn clear_fills_everything() {
        let mut c = Canvas::new(4, 3);
        c.clear(RED);
        assert!(c.pixels.iter().all(|&p| p == RED));
    }

    #[test]
    fn rect_is_clipped_at_the_edges() {
        let mut c = Canvas::new(10, 10);
        c.rect(-5.0, -5.0, 8.0, 8.0, RED);
        assert_eq!(c.get(0, 0), RED);
        assert_eq!(c.get(2, 2), RED);
        assert_eq!(c.get(3, 3), 0);
        c.rect(8.0, 8.0, 100.0, 100.0, RED);
        assert_eq!(c.get(9, 9), RED);
    }

    #[test]
    fn circle_center_and_edge() {
        let mut c = Canvas::new(21, 21);
        c.circle(10.0, 10.0, 5.0, RED);
        assert_eq!(c.get(10, 10), RED);
        assert_eq!(c.get(15, 10), RED);
        assert_eq!(c.get(16, 10), 0);
        assert_eq!(c.get(14, 14), 0);
    }

    #[test]
    fn line_endpoints() {
        let mut c = Canvas::new(10, 10);
        c.line(0.0, 0.0, 9.0, 9.0, RED);
        assert_eq!(c.get(0, 0), RED);
        assert_eq!(c.get(5, 5), RED);
        assert_eq!(c.get(9, 9), RED);
        assert_eq!(c.get(0, 9), 0);
        c.line(-20.0, 3.0, 30.0, 3.0, RED);
        assert_eq!(c.get(0, 3), RED);
        assert_eq!(c.get(9, 3), RED);
    }

    #[test]
    fn text_draws_inside_its_cells() {
        let mut c = Canvas::new(40, 20);
        c.text(0.0, 0.0, "A", RED);
        let lit = |c: &Canvas, x0: usize, x1: usize| (x0..x1).any(|x| (0..16).any(|y| c.get(x, y) == RED));
        assert!(lit(&c, 0, 16));
        assert!(!lit(&c, 16, 40));
        c.text(16.0, 0.0, "à", RED);
        assert!(lit(&c, 16, 32));
    }
}
