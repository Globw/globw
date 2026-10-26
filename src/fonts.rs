//! Fonts for `text`: Windows fonts by name, font files (.ttf/.otf/.ttc) by
//! path, or the built-in "pixel" bitmap font. Vector fonts are drawn
//! anti-aliased with fontdue. Font files are read once, and every glyph is
//! rasterized once per size, so changing font every frame stays cheap.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fontdue::{Font, FontSettings, Metrics};

use crate::canvas::Canvas;

#[cfg(windows)]
const DEFAULT_FONT: &str = "Segoe UI";
#[cfg(target_os = "macos")]
const DEFAULT_FONT: &str = "Helvetica";
#[cfg(not(any(windows, target_os = "macos")))]
const DEFAULT_FONT: &str = "DejaVu Sans";
const DEFAULT_SIZE: i64 = 20;

#[derive(Clone, Copy)]
enum Current {
    /// the 8×8 bitmap font drawn at `scale` × its size
    Pixel { scale: i64 },
    /// `loaded[font]` at `size` pixels
    Vector { font: usize, size: f32 },
}

/// The font `text` draws with; chosen with `set` (the `font` command).
#[derive(Default)]
pub struct Fonts {
    current: Option<Current>,
    /// installed fonts: display name (lowercase) → file, read once from the registry
    installed: Option<Vec<(String, PathBuf)>>,
    /// fonts already read, and where they are in `loaded` (key: lowercase name or path)
    loaded: Vec<Font>,
    by_name: HashMap<String, usize>,
    /// rasterized glyphs: (font, size in bits, character)
    glyphs: HashMap<(usize, u32, char), (Metrics, Vec<u8>)>,
}

fn is_font_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".ttc")
}

/// Fonts listed in the Windows registry (machine-wide and per-user).
#[cfg(windows)]
fn installed_fonts() -> Vec<(String, PathBuf)> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let font_dir = Path::new(&windir).join("Fonts");
    let mut out = Vec::new();
    for key in ["HKLM", "HKCU"] {
        let path = format!("{}\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts", key);
        let Ok(result) = std::process::Command::new("reg").args(["query", &path]).output() else { continue };
        for line in String::from_utf8_lossy(&result.stdout).lines() {
            let Some((name, file)) = line.trim().split_once("    REG_SZ    ") else { continue };
            let file = file.trim();
            let full = if Path::new(file).is_absolute() { PathBuf::from(file) } else { font_dir.join(file) };
            // "Cambria & Cambria Math (TrueType)" → "cambria", "cambria math"
            let name = name.trim();
            let name = name.rfind(" (").map_or(name, |i| &name[..i]);
            for part in name.split(" & ") {
                out.push((part.trim().to_lowercase(), full.clone()));
            }
        }
    }
    out
}

/// Fonts found in the usual system font directories on Linux and macOS,
/// indexed by the family name read from each file.
#[cfg(not(windows))]
fn installed_fonts() -> Vec<(String, PathBuf)> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".fonts"));
        dirs.push(home.join(".local/share/fonts"));
        dirs.push(home.join("Library/Fonts")); // macOS, per user
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
    }
    let mut out = Vec::new();
    for dir in dirs {
        collect_fonts(&dir, 0, &mut out);
    }
    out
}

/// Walks `dir` for font files and records each face's family name → path.
#[cfg(not(windows))]
fn collect_fonts(dir: &Path, depth: usize, out: &mut Vec<(String, PathBuf)>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_fonts(&path, depth + 1, out);
        } else if path.file_name().and_then(|n| n.to_str()).is_some_and(is_font_file) {
            let Ok(data) = std::fs::read(&path) else { continue };
            let count = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
            for i in 0..count {
                let Ok(face) = ttf_parser::Face::parse(&data, i) else { continue };
                for n in face.names() {
                    if n.name_id == ttf_parser::name_id::FAMILY {
                        if let Some(s) = n.to_string() {
                            out.push((s.to_lowercase(), path.clone()));
                        }
                    }
                }
            }
        }
    }
}

/// Index of the face whose family is `family` inside a font file (0 if none matches).
fn face_index(data: &[u8], family: &str) -> u32 {
    let count = ttf_parser::fonts_in_collection(data).unwrap_or(1);
    (0..count)
        .find(|&i| {
            ttf_parser::Face::parse(data, i).is_ok_and(|face| {
                face.names().into_iter().any(|n| {
                    n.name_id == ttf_parser::name_id::FAMILY
                        && n.to_string().is_some_and(|s| s.eq_ignore_ascii_case(family))
                })
            })
        })
        .unwrap_or(0)
}

fn load(path: &Path, shown: &str, family: Option<&str>) -> Result<Font, String> {
    let data = std::fs::read(path).map_err(|e| format!("cannot read the font file \"{}\": {}", shown, e))?;
    let collection_index = family.map_or(0, |f| face_index(&data, f));
    let settings = FontSettings { collection_index, ..FontSettings::default() };
    Font::from_bytes(data, settings).map_err(|e| format!("the font file \"{}\" is not a valid font: {}", shown, e))
}

impl Fonts {
    /// Number of font files read so far.
    pub fn loaded_files(&self) -> usize {
        self.loaded.len()
    }

    /// Chooses the font for the next `text` calls. `size` is the height in pixels.
    pub fn set(&mut self, name: &str, size: i64) -> Result<(), String> {
        if !(1..=500).contains(&size) {
            return Err(format!("the font size must be between 1 and 500, here it is {}", size));
        }
        if name.trim().eq_ignore_ascii_case("pixel") {
            self.current = Some(Current::Pixel { scale: (size / 8).max(1) });
            return Ok(());
        }
        let key = name.trim().to_lowercase();
        let font = match self.by_name.get(&key) {
            Some(&id) => id,
            None => {
                let font = if is_font_file(name) {
                    load(Path::new(name), name, None)?
                } else {
                    let installed = self.installed.get_or_insert_with(installed_fonts);
                    let Some((_, path)) = installed.iter().find(|(n, _)| *n == key) else {
                        return Err(format!(
                            "font \"{}\" not found: use a font installed on your system (like \"{}\"), a .ttf/.otf file, or \"pixel\"",
                            name, DEFAULT_FONT
                        ));
                    };
                    let path = path.clone();
                    load(&path, name, Some(name.trim()))?
                };
                self.loaded.push(font);
                self.by_name.insert(key, self.loaded.len() - 1);
                self.loaded.len() - 1
            }
        };
        self.current = Some(Current::Vector { font, size: size as f32 });
        Ok(())
    }

    fn current(&mut self) -> Current {
        if self.current.is_none() && self.set(DEFAULT_FONT, DEFAULT_SIZE).is_err() {
            self.current = Some(Current::Pixel { scale: 2 });
        }
        self.current.expect("a font is always set here")
    }

    /// The rasterized glyph of `ch`, computed once per font and size.
    fn glyph(&mut self, font: usize, size: f32, ch: char) -> &(Metrics, Vec<u8>) {
        let loaded = &self.loaded;
        self.glyphs.entry((font, size.to_bits(), ch)).or_insert_with(|| loaded[font].rasterize(ch, size))
    }

    /// Width in pixels of `text` in the current font.
    pub fn width(&mut self, text: &str) -> Result<i64, String> {
        Ok(match self.current() {
            Current::Pixel { scale } => text.chars().count() as i64 * 8 * scale,
            Current::Vector { font, size } => {
                let w: f32 = text.chars().map(|ch| self.glyph(font, size, ch).0.advance_width).sum();
                w.round() as i64
            }
        })
    }

    /// Draws `text` with its top-left corner at (x, y).
    pub fn draw(&mut self, canvas: &mut Canvas, x: f64, y: f64, text: &str, color: u32) -> Result<(), String> {
        match self.current() {
            Current::Pixel { scale } => canvas.text_scaled(x, y, text, color, scale),
            Current::Vector { font, size } => {
                let ascent = self.loaded[font].horizontal_line_metrics(size).map_or(size * 0.8, |m| m.ascent);
                let baseline = y as f32 + ascent;
                let mut pen = x as f32;
                for ch in text.chars() {
                    let (m, bitmap) = self.glyph(font, size, ch);
                    let left = (pen + m.xmin as f32).round() as i64;
                    let top = (baseline - (m.ymin as f32 + m.height as f32)).round() as i64;
                    for row in 0..m.height {
                        for col in 0..m.width {
                            let a = bitmap[row * m.width + col];
                            if a > 0 {
                                canvas.blend(left + col as i64, top + row as i64, color, a);
                            }
                        }
                    }
                    pen += m.advance_width;
                }
            }
        }
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::canvas::Canvas;

    const WHITE: u32 = 0xFFFFFF;

    fn lit(c: &Canvas) -> usize {
        c.pixels.iter().filter(|&&p| p != 0).count()
    }

    /// Anti-aliased text has edge pixels between black and the text color.
    fn smooth(c: &Canvas) -> bool {
        c.pixels.iter().any(|&p| p != 0 && p != WHITE)
    }

    #[test]
    fn pixel_font_is_still_available() {
        let mut f = Fonts::default();
        f.set("pixel", 16).unwrap();
        let mut c = Canvas::new(40, 20);
        f.draw(&mut c, 0.0, 0.0, "A", WHITE).unwrap();
        assert!(lit(&c) > 0 && !smooth(&c));
        assert_eq!(f.width("AB").unwrap(), 32);
        f.set("PIXEL", 24).unwrap();
        assert_eq!(f.width("AB").unwrap(), 48);
    }

    #[test]
    fn system_font_is_smooth() {
        let mut f = Fonts::default();
        if f.set("Arial", 24).is_err() {
            return; // Arial is not installed on this OS (e.g. Linux CI)
        }
        let mut c = Canvas::new(200, 40);
        f.draw(&mut c, 0.0, 0.0, "Hello", WHITE).unwrap();
        assert!(lit(&c) > 20 && smooth(&c));
        assert!(f.width("Hello").unwrap() > f.width("Hi").unwrap());
        assert!(f.set("arial", 12).is_ok(), "names ignore case");
    }

    #[test]
    fn default_font_is_not_pixelated() {
        let mut f = Fonts::default();
        if f.set(DEFAULT_FONT, 24).is_err() {
            return; // no smooth system font available here
        }
        let mut c = Canvas::new(200, 40);
        f.draw(&mut c, 0.0, 0.0, "Hello", WHITE).unwrap();
        assert!(smooth(&c));
    }

    #[test]
    fn text_starts_at_the_top_left_corner() {
        let mut f = Fonts::default();
        if f.set("Arial", 30).is_err() {
            return; // Arial is not installed on this OS (e.g. Linux CI)
        }
        let mut c = Canvas::new(200, 100);
        f.draw(&mut c, 50.0, 40.0, "H", WHITE).unwrap();
        let ys: Vec<usize> = (0..100).filter(|&y| (0..200).any(|x| c.get(x, y) != 0)).collect();
        let xs: Vec<usize> = (0..200).filter(|&x| (0..100).any(|y| c.get(x, y) != 0)).collect();
        assert!(ys[0] >= 40 && ys[0] < 50, "top {:?}", ys.first());
        assert!(*ys.last().unwrap() < 70);
        assert!(xs[0] >= 50 && xs[0] < 56, "left {:?}", xs.first());
    }

    #[test]
    fn font_file_by_path() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let path = format!("{}\\Fonts\\arial.ttf", windir);
        if !std::path::Path::new(&path).exists() {
            return; // no Windows arial.ttf here (e.g. Linux/macOS CI)
        }
        let mut f = Fonts::default();
        f.set(&path, 20).unwrap();
        let mut c = Canvas::new(100, 30);
        f.draw(&mut c, 0.0, 0.0, "Ok", WHITE).unwrap();
        assert!(smooth(&c));
    }

    #[test]
    fn font_errors() {
        let mut f = Fonts::default();
        let e = f.set("No Such Font", 20).unwrap_err();
        assert!(e.starts_with("font \"No Such Font\" not found"), "{}", e);
        let e = f.set("missing.ttf", 20).unwrap_err();
        assert!(e.starts_with("cannot read the font file \"missing.ttf\""), "{}", e);
        assert_eq!(f.set("Arial", 0).unwrap_err(), "the font size must be between 1 and 500, here it is 0");
    }

    #[test]
    fn fonts_are_loaded_once() {
        let mut f = Fonts::default();
        if f.set("Arial", 12).is_err() {
            return; // Arial is not installed on this OS (e.g. Linux CI)
        }
        for size in [12, 24, 12, 24] {
            f.set("Arial", size).unwrap();
            f.width("Hello").unwrap();
        }
        assert_eq!(f.loaded_files(), 1);
    }
}
