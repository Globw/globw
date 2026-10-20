//! A pixel buffer with the drawing primitives of the graphics window.
//! It knows nothing about windows, so it can be tested anywhere.

use font8x8::{UnicodeFonts, BASIC_FONTS, LATIN_FONTS};

const COLORS: [(&str, u32); 22] = [
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
    ("lime", 0x40FF40),
    ("navy", 0x000080),
    ("teal", 0x008080),
    ("magenta", 0xFF00FF),
    ("gold", 0xFFD700),
    ("silver", 0xC0C0C0),
    ("maroon", 0x800000),
    ("olive", 0x808000),
    ("darkgray", 0x404040),
    ("lightgray", 0xD3D3D3),
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

    /// Frosted "liquid glass" over a rounded rectangle: blurs the pixels already
    /// drawn behind it, lays a translucent `tint` on top, then a top sheen and a
    /// lit top-left / shaded bottom-right rim. `blur` is the blur radius in pixels,
    /// `radius` the corner radius. Call it after drawing the content behind it.
    pub fn glass(&mut self, x: f64, y: f64, w: f64, h: f64, tint: u32, blur: i64, radius: f64) {
        let x0 = (x.round() as i64).max(0);
        let y0 = (y.round() as i64).max(0);
        let x1 = ((x + w).round() as i64).min(self.w as i64);
        let y1 = ((y + h).round() as i64).min(self.h as i64);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let rad = radius.min(w / 2.0).min(h / 2.0).max(0.0);
        let (rw, rh) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let inside = |px: i64, py: i64| -> bool {
            let (fx, fy) = (px as f64 + 0.5, py as f64 + 0.5);
            if fx < x || fx > x + w || fy < y || fy > y + h {
                return false;
            }
            let cx = (fx).clamp(x + rad, x + w - rad);
            let cy = (fy).clamp(y + rad, y + h - rad);
            let (dx, dy) = (fx - cx, fy - cy);
            dx * dx + dy * dy <= rad * rad + 0.5
        };
        // copy the region behind the panel
        let mut buf = vec![0u32; rw * rh];
        for j in 0..rh {
            for i in 0..rw {
                buf[j * rw + i] = self.pixels[(y0 as usize + j) * self.w + (x0 as usize + i)];
            }
        }
        // separable box blur (frost)
        if blur > 0 {
            let r = blur as usize;
            let avg = |src: &[u32], idxs: &mut dyn Iterator<Item = usize>| -> u32 {
                let (mut sr, mut sg, mut sb, mut n) = (0u32, 0u32, 0u32, 0u32);
                for k in idxs {
                    let p = src[k];
                    sr += (p >> 16) & 0xFF;
                    sg += (p >> 8) & 0xFF;
                    sb += p & 0xFF;
                    n += 1;
                }
                n = n.max(1);
                ((sr / n) << 16) | ((sg / n) << 8) | (sb / n)
            };
            let mut tmp = buf.clone();
            for j in 0..rh {
                for i in 0..rw {
                    let (lo, hi) = (i.saturating_sub(r), (i + r).min(rw - 1));
                    tmp[j * rw + i] = avg(&buf, &mut (lo..=hi).map(|k| j * rw + k));
                }
            }
            for j in 0..rh {
                for i in 0..rw {
                    let (lo, hi) = (j.saturating_sub(r), (j + r).min(rh - 1));
                    buf[j * rw + i] = avg(&tmp, &mut (lo..=hi).map(|k| k * rw + i));
                }
            }
        }
        let (tr, tg, tb) = ((tint >> 16) & 0xFF, (tint >> 8) & 0xFF, tint & 0xFF);
        // composite: blurred background + tint + top sheen, inside the rounded mask
        for j in 0..rh {
            let py = y0 + j as i64;
            for i in 0..rw {
                let px = x0 + i as i64;
                if !inside(px, py) {
                    continue;
                }
                let p = buf[j * rw + i];
                let (mut pr, mut pg, mut pb) = ((p >> 16) & 0xFF, (p >> 8) & 0xFF, p & 0xFF);
                let ta = 60u32; // translucent tint ~0.24
                pr = (tr * ta + pr * (255 - ta)) / 255;
                pg = (tg * ta + pg * (255 - ta)) / 255;
                pb = (tb * ta + pb * (255 - ta)) / 255;
                let fy = j as f64 / rh as f64;
                let ga = ((1.0 - fy * 2.0).max(0.0) * 70.0) as u32; // top sheen
                pr = (255 * ga + pr * (255 - ga)) / 255;
                pg = (255 * ga + pg * (255 - ga)) / 255;
                pb = (255 * ga + pb * (255 - ga)) / 255;
                self.pixels[py as usize * self.w + px as usize] = (pr << 16) | (pg << 8) | pb;
            }
        }
        // rim light: bright top/left edge, soft dark bottom/right edge
        for j in 0..rh {
            let py = y0 + j as i64;
            for i in 0..rw {
                let px = x0 + i as i64;
                if !inside(px, py) {
                    continue;
                }
                if !inside(px, py - 1) || !inside(px - 1, py) {
                    self.blend(px, py, 0xFFFFFF, 150);
                } else if !inside(px, py + 1) || !inside(px + 1, py) {
                    self.blend(px, py, 0x000000, 60);
                }
            }
        }
    }

    /// Mixes `c` into the pixel with opacity `alpha` (0..255), for anti-aliased text.
    pub fn blend(&mut self, x: i64, y: i64, c: u32, alpha: u8) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let i = y as usize * self.w + x as usize;
        let (old, a) = (self.pixels[i], alpha as u32);
        let mix = |shift: u32| ((((c >> shift) & 0xFF) * a + ((old >> shift) & 0xFF) * (255 - a)) / 255) << shift;
        self.pixels[i] = mix(16) | mix(8) | mix(0);
    }

    /// Text in the 8×8 font at double size, one 16×16 cell per character.
    pub fn text(&mut self, x: f64, y: f64, text: &str, c: u32) {
        self.text_scaled(x, y, text, c, 2);
    }

    /// Text in the 8×8 font, each font pixel drawn as a `scale`×`scale` square.
    pub fn text_scaled(&mut self, x: f64, y: f64, text: &str, c: u32, scale: i64) {
        let (x0, y0) = (x.round() as i64, y.round() as i64);
        for (k, ch) in text.chars().enumerate() {
            let glyph = BASIC_FONTS
                .get(ch)
                .or_else(|| LATIN_FONTS.get(ch))
                .or_else(|| BASIC_FONTS.get('?'))
                .unwrap_or([0; 8]);
            let cell = x0 + k as i64 * 8 * scale;
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        let (px, py) = (cell + col * scale, y0 + row as i64 * scale);
                        for oy in 0..scale {
                            for ox in 0..scale {
                                self.set(px + ox, py + oy, c);
                            }
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
        for name in ["lime", "navy", "teal", "magenta", "gold", "silver", "maroon", "olive", "darkgray", "lightgray"] {
            assert!(parse_color(name).is_ok(), "{}", name);
        }
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
