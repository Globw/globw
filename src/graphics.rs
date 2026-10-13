//! The graphics window: shows a `Canvas` and reads keyboard and mouse (crate minifb).

pub use minifb::Key;
use minifb::{MouseButton, MouseMode, Window, WindowOptions};

use crate::canvas::Canvas;

/// The keys a key name stands for (SHIFT and CTRL have a left and a right key).
pub fn keys(name: &str) -> Option<Vec<Key>> {
    let upper = name.trim().to_uppercase();
    let one = |k| Some(vec![k]);
    match upper.as_str() {
        "LEFT" => one(Key::Left),
        "RIGHT" => one(Key::Right),
        "UP" => one(Key::Up),
        "DOWN" => one(Key::Down),
        "SPACE" => one(Key::Space),
        "ENTER" => one(Key::Enter),
        "ESCAPE" => one(Key::Escape),
        "SHIFT" => Some(vec![Key::LeftShift, Key::RightShift]),
        "CTRL" => Some(vec![Key::LeftCtrl, Key::RightCtrl]),
        _ => {
            const LETTERS: [Key; 26] = [
                Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H, Key::I, Key::J, Key::K, Key::L, Key::M,
                Key::N, Key::O, Key::P, Key::Q, Key::R, Key::S, Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z,
            ];
            const DIGITS: [Key; 10] = [
                Key::Key0, Key::Key1, Key::Key2, Key::Key3, Key::Key4, Key::Key5, Key::Key6, Key::Key7, Key::Key8, Key::Key9,
            ];
            let mut chars = upper.chars();
            match (chars.next(), chars.next()) {
                (Some(c @ 'A'..='Z'), None) => one(LETTERS[(c as u8 - b'A') as usize]),
                (Some(c @ '0'..='9'), None) => one(DIGITS[(c as u8 - b'0') as usize]),
                _ => None,
            }
        }
    }
}

pub struct Graphics {
    window: Window,
    pub canvas: Canvas,
}

impl Graphics {
    pub fn new(w: usize, h: usize, title: &str) -> Result<Graphics, String> {
        let mut window = Window::new(title, w, h, WindowOptions::default())
            .map_err(|e| format!("cannot open the window: {}", e))?;
        window.set_target_fps(60);
        let mut g = Graphics { window, canvas: Canvas::new(w, h) };
        g.show();
        Ok(g)
    }

    pub fn is_open(&self) -> bool {
        self.window.is_open()
    }

    /// Shows the canvas and processes window events; waits for the next frame.
    pub fn show(&mut self) {
        if self.window.is_open() {
            let _ = self.window.update_with_buffer(&self.canvas.pixels, self.canvas.w, self.canvas.h);
        }
    }

    pub fn key(&self, name: &str) -> Result<bool, String> {
        let ks = keys(name)
            .ok_or_else(|| format!("unknown key \"{}\": use names like \"A\", \"5\", \"LEFT\", \"SPACE\"", name))?;
        Ok(ks.iter().any(|&k| self.window.is_key_down(k)))
    }

    pub fn mouse(&self) -> (i64, i64) {
        self.window.get_mouse_pos(MouseMode::Clamp).map_or((0, 0), |(x, y)| (x as i64, y as i64))
    }

    pub fn click(&self) -> bool {
        self.window.get_mouse_down(MouseButton::Left)
    }

    pub fn keep_open(&mut self) {
        while self.window.is_open() {
            self.show();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names() {
        assert_eq!(keys("LEFT"), Some(vec![Key::Left]));
        assert_eq!(keys("a"), Some(vec![Key::A]));
        assert_eq!(keys("Z"), Some(vec![Key::Z]));
        assert_eq!(keys("5"), Some(vec![Key::Key5]));
        assert_eq!(keys("SPACE"), Some(vec![Key::Space]));
        assert_eq!(keys("SHIFT"), Some(vec![Key::LeftShift, Key::RightShift]));
        assert_eq!(keys("nope"), None);
    }
}
