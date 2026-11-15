//! The graphics window: shows a `Canvas` and reads keyboard and mouse (crate minifb).
//! Without the `native` feature (the wasm playground) there is no window, so
//! `Graphics` is a stub whose `new` reports that graphics only run on the desktop.

use crate::canvas::Canvas;
use crate::render3d::Scene;

/// The editing keys the GUI widgets react to (independent of the window crate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKey {
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Enter,
    Tab,
    Escape,
    /// a letter key, for shortcuts like Ctrl+C
    Letter(char),
}

/// What the user did since the previous frame: typed text, pressed keys (with
/// auto-repeat), the scroll wheel and the mouse buttons.
#[derive(Debug, Clone, Default)]
pub struct FrameInput {
    pub chars: Vec<char>,
    pub keys: Vec<EditKey>,
    /// wheel movement: positive scrolls up
    pub scroll: f32,
    pub left: bool,
    pub right: bool,
    /// the left / right button went down during this frame
    pub left_pressed: bool,
    pub right_pressed: bool,
    pub ctrl: bool,
    pub shift: bool,
}

/// The mouse pointer's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    Arrow,
    Text,
    Hand,
}

#[cfg(not(feature = "native"))]
pub struct Graphics {
    pub canvas: Canvas,
    pub scene: Scene,
    pub input: FrameInput,
    pub blocked: Vec<(f64, f64, f64, f64)>,
}

#[cfg(not(feature = "native"))]
impl Graphics {
    pub fn new(_w: usize, _h: usize, _title: &str) -> Result<Graphics, String> {
        Err("graphics only run on the desktop, not in the web playground".to_string())
    }
    pub fn is_open(&self) -> bool { false }
    pub fn show(&mut self) {}
    pub fn key(&self, _name: &str) -> Result<bool, String> { Ok(false) }
    pub fn mouse(&self) -> (i64, i64) { (0, 0) }
    pub fn raw_mouse(&self) -> (i64, i64) { (0, 0) }
    pub fn click(&self) -> bool { false }
    pub fn keep_open(&mut self) {}
    pub fn set_pointer(&mut self, _p: Pointer) {}
    pub fn is_headless(&self) -> bool { true }
    pub fn unthrottled(&mut self) {}
    pub fn headless(w: usize, h: usize) -> Graphics {
        Graphics { canvas: Canvas::new(w, h), scene: Scene::default(), input: FrameInput::default(), blocked: Vec::new() }
    }
}

#[cfg(feature = "native")]
pub use minifb::Key;
#[cfg(feature = "native")]
use minifb::{CursorStyle, InputCallback, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
#[cfg(feature = "native")]
use std::{cell::RefCell, rc::Rc};

/// Collects the characters typed into the window (text input, with accents
/// and capitals as the keyboard layout gives them).
#[cfg(feature = "native")]
struct Typed(Rc<RefCell<Vec<char>>>);

#[cfg(feature = "native")]
impl InputCallback for Typed {
    fn add_char(&mut self, c: u32) {
        if let Some(ch) = char::from_u32(c) {
            if !ch.is_control() {
                self.0.borrow_mut().push(ch);
            }
        }
    }
}

#[cfg(feature = "native")]
fn edit_key(k: Key) -> Option<EditKey> {
    Some(match k {
        Key::Backspace => EditKey::Backspace,
        Key::Delete => EditKey::Delete,
        Key::Left => EditKey::Left,
        Key::Right => EditKey::Right,
        Key::Up => EditKey::Up,
        Key::Down => EditKey::Down,
        Key::Home => EditKey::Home,
        Key::End => EditKey::End,
        Key::PageUp => EditKey::PageUp,
        Key::PageDown => EditKey::PageDown,
        Key::Enter | Key::NumPadEnter => EditKey::Enter,
        Key::Tab => EditKey::Tab,
        Key::Escape => EditKey::Escape,
        _ => {
            const LETTERS: [Key; 26] = [
                Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H, Key::I, Key::J, Key::K, Key::L, Key::M,
                Key::N, Key::O, Key::P, Key::Q, Key::R, Key::S, Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z,
            ];
            let i = LETTERS.iter().position(|&l| l == k)?;
            EditKey::Letter((b'a' + i as u8) as char)
        }
    })
}

/// The keys a key name stands for (SHIFT and CTRL have a left and a right key).
#[cfg(feature = "native")]
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

#[cfg(feature = "native")]
pub struct Graphics {
    /// the real window; `None` in headless mode (tests: input is simulated)
    window: Option<Window>,
    /// headless mode: the simulated pointer and the input of the coming frames
    sim_mouse: (i64, i64),
    sim_frames: std::collections::VecDeque<FrameInput>,
    pub canvas: Canvas,
    /// 3D camera and rotation; a new window starts from the default
    pub scene: Scene,
    /// the input of the last frame, for the GUI widgets
    pub input: FrameInput,
    typed: Rc<RefCell<Vec<char>>>,
    pointer: Pointer,
    /// rectangles of the open menus: the widgets do not see the pointer there
    pub blocked: Vec<(f64, f64, f64, f64)>,
}

#[cfg(feature = "native")]
impl Graphics {
    pub fn new(w: usize, h: usize, title: &str) -> Result<Graphics, String> {
        let mut window = Window::new(title, w, h, WindowOptions::default())
            .map_err(|e| format!("cannot open the window: {}", e))?;
        window.set_target_fps(60);
        let typed = Rc::new(RefCell::new(Vec::new()));
        window.set_input_callback(Box::new(Typed(typed.clone())));
        let mut g = Graphics {
            window: Some(window),
            sim_mouse: (0, 0),
            sim_frames: std::collections::VecDeque::new(),
            canvas: Canvas::new(w, h),
            scene: Scene::default(),
            input: FrameInput::default(),
            typed,
            pointer: Pointer::Arrow,
            blocked: Vec::new(),
        };
        g.show();
        Ok(g)
    }

    /// A canvas with no window, whose input is simulated (for tests).
    pub fn headless(w: usize, h: usize) -> Graphics {
        Graphics {
            window: None,
            sim_mouse: (0, 0),
            sim_frames: std::collections::VecDeque::new(),
            canvas: Canvas::new(w, h),
            scene: Scene::default(),
            input: FrameInput::default(),
            typed: Rc::new(RefCell::new(Vec::new())),
            pointer: Pointer::Arrow,
            blocked: Vec::new(),
        }
    }

    pub fn is_headless(&self) -> bool {
        self.window.is_none()
    }

    /// No frame-rate wait for this window (a secondary window: the main one
    /// already paces the program).
    pub fn unthrottled(&mut self) {
        if let Some(w) = self.window.as_mut() {
            w.set_target_fps(0);
        }
    }

    /// Headless mode: the input of the next frame, with the pointer at (x, y).
    pub fn simulate(&mut self, x: i64, y: i64, input: FrameInput) {
        self.sim_frames.push_back(input);
        self.sim_mouse = (x, y);
    }

    pub fn is_open(&self) -> bool {
        self.window.as_ref().map_or(true, Window::is_open)
    }

    /// Shows the canvas and processes window events; waits for the next frame.
    pub fn show(&mut self) {
        if let Some(w) = self.window.as_mut() {
            if w.is_open() {
                let _ = w.update_with_buffer(&self.canvas.pixels, self.canvas.w, self.canvas.h);
            }
        }
        self.collect_input();
    }

    /// Gathers what happened during the last frame, for the GUI widgets.
    fn collect_input(&mut self) {
        let Some(w) = self.window.as_ref() else {
            let mut next = self.sim_frames.pop_front().unwrap_or_default();
            next.left_pressed = next.left && !self.input.left;
            next.right_pressed = next.right && !self.input.right;
            self.input = next;
            return;
        };
        let (left, right) = (w.get_mouse_down(MouseButton::Left), w.get_mouse_down(MouseButton::Right));
        let down = |k: Key| w.is_key_down(k);
        self.input = FrameInput {
            chars: std::mem::take(&mut *self.typed.borrow_mut()),
            keys: w.get_keys_pressed(KeyRepeat::Yes).into_iter().filter_map(edit_key).collect(),
            scroll: w.get_scroll_wheel().map_or(0.0, |(_, y)| y),
            left_pressed: left && !self.input.left,
            right_pressed: right && !self.input.right,
            left,
            right,
            ctrl: down(Key::LeftCtrl) || down(Key::RightCtrl),
            shift: down(Key::LeftShift) || down(Key::RightShift),
        };
        // with Ctrl held the letters are shortcuts, not text
        if self.input.ctrl {
            self.input.chars.clear();
        }
    }

    /// Changes the mouse pointer's look (only when it differs, to avoid flicker).
    pub fn set_pointer(&mut self, p: Pointer) {
        if p != self.pointer {
            self.pointer = p;
            let Some(w) = self.window.as_mut() else { return };
            w.set_cursor_style(match p {
                Pointer::Arrow => CursorStyle::Arrow,
                Pointer::Text => CursorStyle::Ibeam,
                Pointer::Hand => CursorStyle::Arrow,
            });
        }
    }

    pub fn key(&self, name: &str) -> Result<bool, String> {
        let ks = keys(name)
            .ok_or_else(|| format!("unknown key \"{}\": use names like \"A\", \"5\", \"LEFT\", \"SPACE\"", name))?;
        Ok(self.window.as_ref().is_some_and(|w| ks.iter().any(|&k| w.is_key_down(k))))
    }

    /// The pointer, as the widgets see it: far away while it is over an open
    /// menu, so nothing under the menu reacts.
    pub fn mouse(&self) -> (i64, i64) {
        let (x, y) = self.raw_mouse();
        if self.blocked.iter().any(|&(bx, by, bw, bh)| (x as f64) >= bx && (x as f64) < bx + bw && (y as f64) >= by && (y as f64) < by + bh) {
            return (-100_000, -100_000);
        }
        (x, y)
    }

    /// The real pointer position (for the menus themselves).
    pub fn raw_mouse(&self) -> (i64, i64) {
        match &self.window {
            Some(w) => w.get_mouse_pos(MouseMode::Clamp).map_or((0, 0), |(x, y)| (x as i64, y as i64)),
            None => self.sim_mouse,
        }
    }

    pub fn click(&self) -> bool {
        match &self.window {
            Some(w) => w.get_mouse_down(MouseButton::Left),
            None => self.input.left,
        }
    }

    pub fn keep_open(&mut self) {
        while self.window.as_ref().is_some_and(Window::is_open) {
            self.show();
        }
    }
}

#[cfg(all(test, feature = "native"))]
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
