//! The interactive parts of the `gtq` GUI: keyboard focus, text editing,
//! the clipboard and the colour theme.
//!
//! `gtq` is immediate-mode: the program draws every widget each frame and gets
//! the new value back. What must survive between frames — which widget has the
//! focus, the caret and selection of the field being edited, the undo history —
//! lives in [`GuiState`]. A widget is known by an id made from its kind and its
//! rectangle, so the same call in the next frame finds its state again.
//!
//! The editing itself ([`Editor`]) is plain logic with no window, so it is
//! unit-tested on its own.

use crate::graphics::{EditKey, FrameInput};

/// The text being edited in the focused field.
#[derive(Debug, Clone, PartialEq)]
pub struct Editor {
    pub text: Vec<char>,
    /// the caret, as a character index (0..=len)
    pub caret: usize,
    /// the other end of the selection; equal to `caret` when nothing is selected
    pub anchor: usize,
    /// whether Enter inserts a line break (a text area) or not (a field)
    pub multiline: bool,
}

/// What a key press asked for besides editing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EditResult {
    /// text to put on the clipboard (copy / cut)
    pub copy: Option<String>,
    /// the user asked to paste
    pub paste: bool,
    /// Enter in a single-line field
    pub submitted: bool,
    /// the text changed (for the undo history)
    pub changed: bool,
    pub undo: bool,
    pub redo: bool,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Editor {
    pub fn new(text: &str, multiline: bool) -> Editor {
        let text: Vec<char> = text.chars().collect();
        let n = text.len();
        Editor { text, caret: n, anchor: n, multiline }
    }

    pub fn string(&self) -> String {
        self.text.iter().collect()
    }

    /// The selected range (start, end), possibly empty.
    pub fn selection(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    pub fn selected(&self) -> String {
        let (a, b) = self.selection();
        self.text[a..b].iter().collect()
    }

    fn delete_selection(&mut self) -> bool {
        let (a, b) = self.selection();
        if a == b {
            return false;
        }
        self.text.drain(a..b);
        self.caret = a;
        self.anchor = a;
        true
    }

    /// Types text at the caret, replacing the selection.
    pub fn insert(&mut self, s: &str) {
        self.delete_selection();
        let chars: Vec<char> = s
            .chars()
            .filter(|&c| if self.multiline { c == '\n' || !c.is_control() } else { !c.is_control() })
            .collect();
        let n = chars.len();
        self.text.splice(self.caret..self.caret, chars);
        self.caret += n;
        self.anchor = self.caret;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
    }

    /// The start of the word before `i` (Ctrl+Left, Ctrl+Backspace).
    fn word_left(&self, mut i: usize) -> usize {
        while i > 0 && !is_word(self.text[i - 1]) {
            i -= 1;
        }
        while i > 0 && is_word(self.text[i - 1]) {
            i -= 1;
        }
        i
    }

    /// The end of the word after `i` (Ctrl+Right, Ctrl+Delete).
    fn word_right(&self, mut i: usize) -> usize {
        let n = self.text.len();
        while i < n && !is_word(self.text[i]) {
            i += 1;
        }
        while i < n && is_word(self.text[i]) {
            i += 1;
        }
        i
    }

    /// The start and end of the line holding `i`.
    fn line_bounds(&self, i: usize) -> (usize, usize) {
        let start = self.text[..i].iter().rposition(|&c| c == '\n').map_or(0, |p| p + 1);
        let end = self.text[i..].iter().position(|&c| c == '\n').map_or(self.text.len(), |p| i + p);
        (start, end)
    }

    /// The same column one line up (`up`) or down, for a text area.
    fn vertical(&self, i: usize, up: bool) -> usize {
        let (start, end) = self.line_bounds(i);
        let col = i - start;
        if up {
            if start == 0 {
                return 0;
            }
            let (ps, pe) = self.line_bounds(start - 1);
            (ps + col).min(pe)
        } else {
            if end == self.text.len() {
                return end;
            }
            let (ns, ne) = self.line_bounds(end + 1);
            (ns + col).min(ne)
        }
    }

    fn move_to(&mut self, i: usize, extend: bool) {
        self.caret = i.min(self.text.len());
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Applies one key press.
    pub fn key(&mut self, k: EditKey, ctrl: bool, shift: bool) -> EditResult {
        let mut r = EditResult::default();
        let (a, b) = self.selection();
        match k {
            EditKey::Backspace => {
                if !self.delete_selection() && self.caret > 0 {
                    let from = if ctrl { self.word_left(self.caret) } else { self.caret - 1 };
                    self.text.drain(from..self.caret);
                    self.caret = from;
                    self.anchor = from;
                }
                r.changed = true;
            }
            EditKey::Delete => {
                if !self.delete_selection() && self.caret < self.text.len() {
                    let to = if ctrl { self.word_right(self.caret) } else { self.caret + 1 };
                    self.text.drain(self.caret..to);
                }
                r.changed = true;
            }
            EditKey::Left => {
                let to = if ctrl {
                    self.word_left(self.caret)
                } else if a != b && !shift {
                    a
                } else {
                    self.caret.saturating_sub(1)
                };
                self.move_to(to, shift);
            }
            EditKey::Right => {
                let to = if ctrl {
                    self.word_right(self.caret)
                } else if a != b && !shift {
                    b
                } else {
                    self.caret + 1
                };
                self.move_to(to, shift);
            }
            EditKey::Up if self.multiline => self.move_to(self.vertical(self.caret, true), shift),
            EditKey::Down if self.multiline => self.move_to(self.vertical(self.caret, false), shift),
            EditKey::Home => {
                let to = if ctrl || !self.multiline { 0 } else { self.line_bounds(self.caret).0 };
                self.move_to(to, shift);
            }
            EditKey::End => {
                let to = if ctrl || !self.multiline { self.text.len() } else { self.line_bounds(self.caret).1 };
                self.move_to(to, shift);
            }
            EditKey::Enter if self.multiline => {
                self.insert("\n");
                r.changed = true;
            }
            EditKey::Enter => r.submitted = true,
            EditKey::Letter('a') if ctrl => self.select_all(),
            EditKey::Letter('c') if ctrl => {
                if a != b {
                    r.copy = Some(self.selected());
                }
            }
            EditKey::Letter('x') if ctrl => {
                if a != b {
                    r.copy = Some(self.selected());
                    self.delete_selection();
                    r.changed = true;
                }
            }
            EditKey::Letter('v') if ctrl => r.paste = true,
            EditKey::Letter('z') if ctrl && shift => r.redo = true,
            EditKey::Letter('z') if ctrl => r.undo = true,
            EditKey::Letter('y') if ctrl => r.redo = true,
            _ => {}
        }
        r
    }
}

/// Colours of the widgets, for the dark (default) and light themes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub window: u32,
    pub field: u32,
    pub field_hover: u32,
    pub border: u32,
    pub focus: u32,
    pub text: u32,
    pub muted: u32,
    pub selection: u32,
    pub accent: u32,
    pub accent_text: u32,
    pub panel: u32,
    pub row_alt: u32,
    pub row_hover: u32,
}

impl Theme {
    pub fn dark() -> Theme {
        Theme {
            dark: true,
            window: 0x111827,
            field: 0x1f2937,
            field_hover: 0x243042,
            border: 0x475569,
            focus: 0x60a5fa,
            text: 0xe2e8f0,
            muted: 0x94a3b8,
            selection: 0x1d4ed8,
            accent: 0x3b82f6,
            accent_text: 0xffffff,
            panel: 0x1e293b,
            row_alt: 0x1a2332,
            row_hover: 0x2b3a52,
        }
    }

    pub fn light() -> Theme {
        Theme {
            dark: false,
            window: 0xf3f4f6,
            field: 0xffffff,
            field_hover: 0xf9fafb,
            border: 0x9ca3af,
            focus: 0x2563eb,
            text: 0x111827,
            muted: 0x6b7280,
            selection: 0xbfdbfe,
            accent: 0x2563eb,
            accent_text: 0xffffff,
            panel: 0xffffff,
            row_alt: 0xf3f4f6,
            row_hover: 0xdbeafe,
        }
    }
}

thread_local! {
    /// whether the light theme is on, for the colours of the classic widgets
    static LIGHT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn set_light(on: bool) {
    LIGHT.with(|l| l.set(on));
}

/// A colour of the built-in widget palette (designed dark) as the current theme
/// shows it: unchanged in the dark theme, its light counterpart in the light one.
pub fn ui_color(hex: &str) -> Result<u32, String> {
    let c = crate::canvas::parse_color(hex)?;
    if !LIGHT.with(std::cell::Cell::get) {
        return Ok(c);
    }
    Ok(match c {
        0xe2e8f0 | 0xf8fafc => 0x111827, // text
        0xdddddd => 0x374151,
        0x94a3b8 | 0x888888 => 0x6b7280, // muted text
        0x64748b => 0x94a3b8,
        0x1e293b | 0x1f2937 => 0xffffff, // panels, fields
        0x172033 => 0xf8fafc,
        0x0b1220 => 0xf9fafb,
        0x334155 => 0xf3f4f6,            // raised / hovered panels
        0x374151 => 0xe5e7eb,            // buttons, tracks
        0x4a5568 => 0xd1d5db,
        0x2d3748 => 0xc7cbd1,
        0x475569 => 0x9ca3af,            // borders
        0x60a5fa | 0x3b82f6 => 0x2563eb, // accent
        0x93c5fd | 0x2563eb => 0x1d4ed8,
        0xbfdbfe => 0x1e40af,
        0x22c55e => 0x16a34a,
        0xfbbf24 => 0xd97706,
        0xef4444 => 0xdc2626,
        other => other,
    })
}

/// Undo history entries: the text and the caret.
type Snapshot = (Vec<char>, usize);

/// What the GUI remembers between frames.
pub struct GuiState {
    /// the id of the widget with the keyboard focus
    pub focus: Option<u64>,
    /// the editor of the focused text widget
    pub editor: Option<Editor>,
    /// horizontal (fields) or vertical (areas) scroll of the focused text, in pixels
    pub scroll: f64,
    /// the mouse is selecting text in the focused widget
    pub dragging: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// focusable widgets drawn this frame, in order, for Tab
    order: Vec<u64>,
    /// a widget took this frame's click
    claimed: bool,
    /// the next focused widget should select all its text (focus by Tab)
    pub select_on_focus: bool,
    /// Enter was pressed in this single-line field (its id), this frame
    pub submitted: Option<u64>,
    /// the mouse is over a text widget this frame
    pub over_text: bool,
    /// the scroll (x, y) of each scrolling widget, by id
    pub scrolls: std::collections::HashMap<u64, (f64, f64)>,
    /// the sort (column, descending) of each table, by id
    pub sorts: std::collections::HashMap<u64, (usize, bool)>,
    /// the last click on a list row (widget, row, when), for double-clicks
    pub last_click: Option<(u64, usize, std::time::Instant)>,
    /// a list or table row was double-clicked or got Enter, this frame
    pub activated: Option<u64>,
    /// the open menu, if any
    pub menu: Option<OpenMenu>,
    /// drop-downs to draw at the end of this frame
    pub popups: Vec<Popup>,
    /// the automatic column layout: x, the y of the next row, width, gap
    pub column: Option<(f64, f64, f64, f64)>,
    pub theme: Theme,
    /// when the caret last moved, for blinking
    pub blink: std::time::Instant,
}

impl Default for GuiState {
    fn default() -> GuiState {
        GuiState {
            focus: None,
            editor: None,
            scroll: 0.0,
            dragging: false,
            undo: Vec::new(),
            redo: Vec::new(),
            order: Vec::new(),
            claimed: false,
            select_on_focus: false,
            submitted: None,
            over_text: false,
            scrolls: std::collections::HashMap::new(),
            sorts: std::collections::HashMap::new(),
            last_click: None,
            activated: None,
            menu: None,
            popups: Vec::new(),
            column: None,
            theme: Theme::dark(),
            blink: std::time::Instant::now(),
        }
    }
}

/// An id for a widget: its kind and its rectangle.
pub fn widget_id(kind: u64, x: f64, y: f64, w: f64, h: f64) -> u64 {
    let mut v = kind.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    for p in [x, y, w, h] {
        v = (v ^ (p.round() as i64 as u64)).wrapping_mul(0x0100_0000_01B3);
    }
    v
}

impl GuiState {
    /// Records a focusable widget (for Tab) and whether it is focused.
    pub fn register(&mut self, id: u64) -> bool {
        self.order.push(id);
        self.focus == Some(id)
    }

    /// Gives the focus to a widget (by click or Tab) with its current text.
    pub fn take_focus(&mut self, id: u64, text: &str, multiline: bool) {
        self.claimed = true;
        if self.focus != Some(id) {
            self.focus = Some(id);
            self.editor = Some(Editor::new(text, multiline));
            self.scroll = 0.0;
            self.undo.clear();
            self.redo.clear();
        }
        self.blink = std::time::Instant::now();
    }

    pub fn claim(&mut self) {
        self.claimed = true;
    }

    /// Saves the text before an edit, for Ctrl+Z.
    pub fn remember(&mut self, e: &Editor) {
        if self.undo.last().is_some_and(|(t, _)| *t == e.text) {
            return;
        }
        self.undo.push((e.text.clone(), e.caret));
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, e: &mut Editor) {
        if let Some((t, c)) = self.undo.pop() {
            self.redo.push((e.text.clone(), e.caret));
            e.text = t;
            e.caret = c.min(e.text.len());
            e.anchor = e.caret;
        }
    }

    pub fn redo(&mut self, e: &mut Editor) {
        if let Some((t, c)) = self.redo.pop() {
            self.undo.push((e.text.clone(), e.caret));
            e.text = t;
            e.caret = c.min(e.text.len());
            e.anchor = e.caret;
        }
    }

    /// The end of a frame: a click on nothing drops the focus, and Tab moves it
    /// to the next (Shift: previous) focusable widget.
    pub fn end_frame(&mut self, input: &FrameInput) {
        if input.left_pressed && !self.claimed {
            self.focus = None;
            self.editor = None;
        }
        if input.keys.contains(&EditKey::Tab) && !self.order.is_empty() {
            let n = self.order.len();
            let next = match self.focus.and_then(|f| self.order.iter().position(|&o| o == f)) {
                Some(i) if input.shift => (i + n - 1) % n,
                Some(i) => (i + 1) % n,
                None if input.shift => n - 1,
                None => 0,
            };
            self.focus = Some(self.order[next]);
            self.editor = None; // made by the widget, which knows its text
            self.select_on_focus = true;
        }
        self.order.clear();
        self.claimed = false;
        self.submitted = None;
        self.activated = None;
        self.over_text = false;
    }
}

/// The system clipboard (desktop only).
pub fn clipboard_get() -> Option<String> {
    #[cfg(feature = "native")]
    {
        arboard::Clipboard::new().ok()?.get_text().ok()
    }
    #[cfg(not(feature = "native"))]
    {
        None
    }
}

pub fn clipboard_set(text: &str) {
    #[cfg(feature = "native")]
    {
        if let Ok(mut c) = arboard::Clipboard::new() {
            let _ = c.set_text(text.to_string());
        }
    }
    #[cfg(not(feature = "native"))]
    {
        let _ = text;
    }
}

/// Widget kinds, for their ids.
const KIND_FIELD: u64 = 1;

fn hex(c: u32) -> String {
    format!("#{:06x}", c & 0xFF_FFFF)
}

/// x of every caret position in `s`, from the left of the text.
fn caret_stops(fonts: &mut crate::fonts::Fonts, s: &str) -> Result<Vec<f64>, String> {
    let mut out = vec![0.0];
    let mut acc = 0.0;
    for c in s.chars() {
        acc += fonts.width(&c.to_string())? as f64;
        out.push(acc);
    }
    Ok(out)
}

/// The caret position nearest to `at` pixels from the left of the text.
fn nearest_stop(stops: &[f64], at: f64) -> usize {
    let mut best = 0;
    for (i, &s) in stops.iter().enumerate() {
        if (s - at).abs() < (stops[best] - at).abs() {
            best = i;
        }
    }
    best
}

impl crate::builtins::Env {
    /// A theme colour by name, as "#rrggbb", for the program's own drawing.
    pub(crate) fn theme_color(&self, name: &str) -> Result<String, String> {
        let t = &self.gui.theme;
        Ok(hex(match name.trim().to_lowercase().as_str() {
            "window" | "background" => t.window,
            "field" => t.field,
            "border" => t.border,
            "focus" => t.focus,
            "text" => t.text,
            "muted" => t.muted,
            "selection" => t.selection,
            "accent" => t.accent,
            "panel" => t.panel,
            other => {
                return Err(format!(
                    "gtq.theme_color: unknown colour \"{}\" (window, field, border, focus, text, muted, selection, accent, panel)",
                    other
                ))
            }
        }))
    }

    /// Applies this frame's typing and editing keys to the focused editor.
    fn edit(&mut self, e: &mut Editor, input: &FrameInput, id: u64, secret: bool) {
        if !input.chars.is_empty() {
            self.gui.remember(e);
            let typed: String = input.chars.iter().collect();
            e.insert(&typed);
            self.gui.blink = std::time::Instant::now();
        }
        for &k in input.keys.iter().filter(|&&k| k != EditKey::Tab) {
            let before = e.clone();
            let r = e.key(k, input.ctrl, input.shift);
            if let Some(c) = &r.copy {
                if !secret {
                    clipboard_set(c);
                }
            }
            if r.paste {
                if let Some(c) = clipboard_get() {
                    e.insert(&c);
                }
            }
            if r.undo {
                self.gui.undo(e);
            }
            if r.redo {
                self.gui.redo(e);
            }
            if r.submitted {
                self.gui.submitted = Some(id);
            }
            if e.text != before.text && !r.undo && !r.redo {
                self.gui.remember(&before);
            }
            self.gui.blink = std::time::Instant::now();
        }
    }

    /// A single-line text field (`password` hides the characters): click or Tab
    /// to focus, then type; arrows, Home/End, Shift to select, Ctrl+A/C/X/V/Z/Y.
    /// Returns the text as it is after this frame.
    pub(crate) fn text_field(&mut self, x: f64, y: f64, w: f64, h: f64, value: &str, password: bool) -> Result<String, String> {
        let id = widget_id(KIND_FIELD, x, y, w, h);
        let focused = self.gui.register(id);
        let g = self.graphics.as_mut().ok_or_else(|| crate::builtins::NO_WINDOW.to_string())?;
        let input = g.input.clone();
        let (mx, my) = g.mouse();
        let (mx, my) = (mx as f64, my as f64);
        let hover = mx >= x && mx < x + w && my >= y && my < y + h;
        if hover {
            self.gui.over_text = true;
        }
        let pad = 7.0;
        let fh = self.fonts.height();
        let text_x = x + pad;
        let text_y = y + ((h - fh) / 2.0).max(0.0);
        let shown = |e: &str| if password { "\u{2022}".repeat(e.chars().count()) } else { e.to_string() };

        // the focus arrived by Tab: make the editor, with all the text selected
        if focused && self.gui.editor.is_none() {
            let mut e = Editor::new(value, false);
            if std::mem::take(&mut self.gui.select_on_focus) {
                e.select_all();
            }
            self.gui.editor = Some(e);
            self.gui.scroll = 0.0;
        }
        // the program changed the value itself (for example cleared it)
        if focused {
            if let Some(e) = self.gui.editor.as_mut() {
                if e.string() != value {
                    *e = Editor::new(value, false);
                }
            }
        }

        // mouse: click to place the caret (Shift extends), drag to select
        if input.left_pressed && hover {
            let was = self.gui.focus == Some(id);
            self.gui.take_focus(id, value, false);
            let scroll = self.gui.scroll;
            let e = self.gui.editor.as_mut().expect("an editor after take_focus");
            let st = caret_stops(&mut self.fonts, &shown(&e.string()))?;
            let at = nearest_stop(&st, mx - text_x + scroll);
            e.caret = at;
            if !(input.shift && was) {
                e.anchor = at;
            }
            self.gui.dragging = true;
        } else if self.gui.focus == Some(id) && self.gui.dragging {
            if input.left {
                let scroll = self.gui.scroll;
                let e = self.gui.editor.as_mut().expect("an editor while focused");
                let st = caret_stops(&mut self.fonts, &shown(&e.string()))?;
                e.caret = nearest_stop(&st, mx - text_x + scroll);
            } else {
                self.gui.dragging = false;
            }
        }

        // keyboard: only the focused field reads it
        let focused = self.gui.focus == Some(id);
        if focused {
            let mut e = self.gui.editor.take().expect("an editor while focused");
            self.edit(&mut e, &input, id, password);
            self.gui.editor = Some(e);
        }

        // draw
        let t = self.gui.theme;
        let current = match (&self.gui.editor, focused) {
            (Some(e), true) => e.clone(),
            _ => Editor::new(value, false),
        };
        let text = shown(&current.string());
        let st = caret_stops(&mut self.fonts, &text)?;
        let last = st.len() - 1;
        if focused {
            // keep the caret inside the field
            let cx = st[current.caret.min(last)];
            let inner = (w - 2.0 * pad).max(1.0);
            if cx - self.gui.scroll > inner {
                self.gui.scroll = cx - inner;
            }
            if cx < self.gui.scroll {
                self.gui.scroll = cx;
            }
            self.gui.scroll = self.gui.scroll.min((st[last] - inner).max(0.0)).max(0.0);
        }
        let scroll = if focused { self.gui.scroll } else { 0.0 };
        let blink_on = (self.gui.blink.elapsed().as_millis() / 530) % 2 == 0;
        let g = self.graphics.as_mut().ok_or_else(|| crate::builtins::NO_WINDOW.to_string())?;
        let c = &mut g.canvas;
        c.rect(x, y, w, h, if hover && !focused { t.field_hover } else { t.field });
        let edge = if focused { t.focus } else { t.border };
        let thick = if focused { 2.0 } else { 1.0 };
        c.rect(x, y, w, thick, edge);
        c.rect(x, y + h - thick, w, thick, edge);
        c.rect(x, y, thick, h, edge);
        c.rect(x + w - thick, y, thick, h, edge);
        let saved = c.clip();
        c.set_clip(x + 2.0, y + 2.0, w - 4.0, h - 4.0);
        let (sa, sb) = current.selection();
        if focused && sa != sb {
            let (xa, xb) = (st[sa.min(last)], st[sb.min(last)]);
            c.rect(text_x + xa - scroll, text_y, xb - xa, fh, t.selection);
        }
        self.fonts.draw(c, text_x - scroll, text_y, &text, t.text)?;
        if focused && blink_on {
            let cx = text_x + st[current.caret.min(last)] - scroll;
            c.rect(cx, text_y, 1.5, fh, t.text);
        }
        c.restore_clip(saved);
        Ok(if focused { current.string() } else { value.to_string() })
    }
}

const KIND_AREA: u64 = 2;

/// The (start, end) character index of each line of `text`.
fn line_spans(text: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, &c) in text.iter().enumerate() {
        if c == '\n' {
            out.push((start, i));
            start = i + 1;
        }
    }
    out.push((start, text.len()));
    out
}

/// The line holding character index `i`.
fn line_of(spans: &[(usize, usize)], i: usize) -> usize {
    spans.iter().position(|&(a, b)| i >= a && i <= b).unwrap_or(spans.len() - 1)
}

/// How many pixels one notch of the mouse wheel scrolls, in lines.
const WHEEL_LINES: f64 = 3.0;

impl crate::builtins::Env {
    /// A multi-line text area: typing, Enter for new lines, arrows (also up and
    /// down), Page Up/Down, selection, clipboard, undo, and a vertical scroll
    /// that follows the caret and the mouse wheel.
    pub(crate) fn text_area(&mut self, x: f64, y: f64, w: f64, h: f64, value: &str) -> Result<String, String> {
        let id = widget_id(KIND_AREA, x, y, w, h);
        let focused = self.gui.register(id);
        let g = self.graphics.as_mut().ok_or_else(|| crate::builtins::NO_WINDOW.to_string())?;
        let input = g.input.clone();
        let (mx, my) = g.mouse();
        let (mx, my) = (mx as f64, my as f64);
        let hover = mx >= x && mx < x + w && my >= y && my < y + h;
        if hover {
            self.gui.over_text = true;
        }
        let pad = 8.0;
        let fh = self.fonts.height();
        let lh = (fh * 1.3).round().max(1.0);
        let (inner_w, inner_h) = ((w - 2.0 * pad).max(1.0), (h - 2.0 * pad).max(1.0));
        let (mut sx, mut sy) = self.gui.scrolls.get(&id).copied().unwrap_or((0.0, 0.0));

        if focused && self.gui.editor.is_none() {
            let mut e = Editor::new(value, true);
            if std::mem::take(&mut self.gui.select_on_focus) {
                e.select_all();
            }
            self.gui.editor = Some(e);
        }
        if focused {
            if let Some(e) = self.gui.editor.as_mut() {
                if e.string() != value {
                    *e = Editor::new(value, true);
                }
            }
        }
        if hover && input.scroll != 0.0 {
            sy -= input.scroll as f64 * lh * WHEEL_LINES;
        }

        // the character index under the pointer
        let index_at = |fonts: &mut crate::fonts::Fonts, text: &[char], sx: f64, sy: f64| -> Result<usize, String> {
            let spans = line_spans(text);
            let line = (((my - y - pad + sy) / lh).floor().max(0.0) as usize).min(spans.len() - 1);
            let (a, b) = spans[line];
            let s: String = text[a..b].iter().collect();
            let st = caret_stops(fonts, &s)?;
            Ok(a + nearest_stop(&st, mx - x - pad + sx))
        };
        if input.left_pressed && hover {
            let was = self.gui.focus == Some(id);
            self.gui.take_focus(id, value, true);
            let mut e = self.gui.editor.take().expect("an editor after take_focus");
            let at = index_at(&mut self.fonts, &e.text, sx, sy)?;
            e.caret = at;
            if !(input.shift && was) {
                e.anchor = at;
            }
            self.gui.editor = Some(e);
            self.gui.dragging = true;
        } else if self.gui.focus == Some(id) && self.gui.dragging {
            if input.left {
                let mut e = self.gui.editor.take().expect("an editor while focused");
                e.caret = index_at(&mut self.fonts, &e.text, sx, sy)?;
                self.gui.editor = Some(e);
            } else {
                self.gui.dragging = false;
            }
        }

        let focused = self.gui.focus == Some(id);
        if focused {
            let mut e = self.gui.editor.take().expect("an editor while focused");
            // Page Up / Page Down: a screenful of lines
            let page = ((inner_h / lh).floor() as usize).max(1);
            for &k in &input.keys {
                let step = match k {
                    EditKey::PageUp => Some(EditKey::Up),
                    EditKey::PageDown => Some(EditKey::Down),
                    _ => None,
                };
                if let Some(step) = step {
                    for _ in 0..page {
                        e.key(step, false, input.shift);
                    }
                }
            }
            self.edit(&mut e, &input, id, false);
            self.gui.editor = Some(e);
        }

        let current = match (&self.gui.editor, focused) {
            (Some(e), true) => e.clone(),
            _ => Editor::new(value, true),
        };
        let spans = line_spans(&current.text);
        let total_h = spans.len() as f64 * lh;
        let caret_line = line_of(&spans, current.caret);
        if focused && (!input.keys.is_empty() || !input.chars.is_empty() || self.gui.dragging) {
            // keep the caret in view
            let cy = caret_line as f64 * lh;
            if cy + lh - sy > inner_h {
                sy = cy + lh - inner_h;
            }
            if cy < sy {
                sy = cy;
            }
            let (a, _) = spans[caret_line];
            let s: String = current.text[a..current.caret].iter().collect();
            let cx = self.fonts.width(&s)? as f64;
            if cx - sx > inner_w - 2.0 {
                sx = cx - inner_w + 2.0;
            }
            if cx < sx {
                sx = cx;
            }
        }
        sy = sy.min((total_h - inner_h).max(0.0)).max(0.0);
        sx = sx.max(0.0);
        self.gui.scrolls.insert(id, (sx, sy));

        // draw
        let t = self.gui.theme;
        let blink_on = (self.gui.blink.elapsed().as_millis() / 530) % 2 == 0;
        let (sa, sb) = current.selection();
        let space = self.fonts.width(" ")? as f64;
        let first = (sy / lh).floor() as usize;
        let last = (((sy + inner_h) / lh).ceil() as usize).min(spans.len());
        let mut lines = Vec::new();
        for (i, &(a, b)) in spans.iter().enumerate().take(last).skip(first) {
            let s: String = current.text[a..b].iter().collect();
            let st = caret_stops(&mut self.fonts, &s)?;
            lines.push((i, a, b, s, st));
        }
        let g = self.graphics.as_mut().ok_or_else(|| crate::builtins::NO_WINDOW.to_string())?;
        let c = &mut g.canvas;
        c.rect(x, y, w, h, t.field);
        let edge = if focused { t.focus } else { t.border };
        let thick = if focused { 2.0 } else { 1.0 };
        c.rect(x, y, w, thick, edge);
        c.rect(x, y + h - thick, w, thick, edge);
        c.rect(x, y, thick, h, edge);
        c.rect(x + w - thick, y, thick, h, edge);
        let saved = c.clip();
        c.set_clip(x + pad / 2.0, y + 2.0, w - pad, h - 4.0);
        for (i, a, b, s, st) in &lines {
            let ly = y + pad + *i as f64 * lh - sy;
            let ty = ly + (lh - fh) / 2.0;
            if focused && sa != sb && sa <= *b && sb >= *a {
                let from = st[sa.max(*a) - a];
                let mut to = st[sb.min(*b) - a];
                if sb > *b {
                    to += space; // the line break is selected too
                }
                c.rect(x + pad + from - sx, ly, to - from, lh, t.selection);
            }
            self.fonts.draw(c, x + pad - sx, ty, s, t.text)?;
            if focused && blink_on && *i == caret_line {
                let cx = x + pad + st[current.caret - a] - sx;
                c.rect(cx, ty, 1.5, fh, t.text);
            }
        }
        // a scrollbar when the text is taller than the area
        if total_h > inner_h {
            let track = h - 8.0;
            let thumb = (track * inner_h / total_h).max(20.0);
            let top = y + 4.0 + (track - thumb) * (sy / (total_h - inner_h));
            c.rect(x + w - 7.0, top, 4.0, thumb, t.border);
        }
        c.restore_clip(saved);
        Ok(if focused { current.string() } else { value.to_string() })
    }
}

const KIND_LIST: u64 = 3;
const KIND_TABLE: u64 = 4;

/// Compares two cells for sorting: as numbers when both are numbers, else as
/// text ignoring case.
fn cell_order(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    }
}

/// The display order of the rows (original indices), sorted by a column.
pub fn sorted_order(rows: &[Vec<String>], sort: Option<(usize, bool)>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    if let Some((col, desc)) = sort {
        let cell = |r: usize| rows[r].get(col).map(String::as_str).unwrap_or("");
        order.sort_by(|&a, &b| {
            let o = cell_order(cell(a), cell(b));
            if desc {
                o.reverse()
            } else {
                o
            }
        });
    }
    order
}

/// Strings of a Globw list of strings.
pub(crate) fn strings_of(v: &crate::vm::Value) -> Vec<String> {
    match v {
        crate::vm::Value::Seq(q) => q.borrow().iter().map(crate::vm::display).collect(),
        _ => Vec::new(),
    }
}

impl crate::builtins::Env {
    /// A scrolling list of rows (a list box, or a table when `headers` is given):
    /// click or arrows to select, double-click or Enter to activate, the wheel
    /// to scroll, a header click to sort. `selected` and the result are 1-based
    /// indices into the original `rows` (0: none).
    pub(crate) fn grid_view(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        headers: Option<Vec<String>>,
        rows: Vec<Vec<String>>,
        selected: i64,
    ) -> Result<i64, String> {
        let kind = if headers.is_some() { KIND_TABLE } else { KIND_LIST };
        let id = widget_id(kind, x, y, w, h);
        let focused = self.gui.register(id);
        if focused {
            self.gui.select_on_focus = false;
            self.gui.editor = None;
        }
        let g = self.graphics.as_mut().ok_or_else(|| crate::builtins::NO_WINDOW.to_string())?;
        let input = g.input.clone();
        let (mx, my) = g.mouse();
        let (mx, my) = (mx as f64, my as f64);
        let hover = mx >= x && mx < x + w && my >= y && my < y + h;
        let fh = self.fonts.height();
        let rh = (fh * 1.6).round().max(1.0);
        let head_h = if headers.is_some() { rh } else { 0.0 };
        let (body_y, body_h) = (y + head_h, (h - head_h).max(1.0));
        let pad = 8.0;
        let ncols = headers.as_ref().map_or(1, Vec::len).max(rows.iter().map(Vec::len).max().unwrap_or(1)).max(1);

        // column widths: content-sized, the last one takes the rest
        let mut widths = vec![0.0f64; ncols];
        if let Some(hs) = &headers {
            for (c, t) in hs.iter().enumerate() {
                widths[c] = widths[c].max(self.fonts.width(t)? as f64 + 2.0 * pad + 12.0);
            }
        }
        for r in rows.iter().take(300) {
            for (c, t) in r.iter().enumerate().take(ncols) {
                widths[c] = widths[c].max(self.fonts.width(t)? as f64 + 2.0 * pad);
            }
        }
        let inner_w = w - 8.0;
        let total: f64 = widths.iter().sum();
        if total < inner_w {
            widths[ncols - 1] += inner_w - total;
        } else if total > 0.0 {
            let k = inner_w / total;
            widths.iter_mut().for_each(|c| *c *= k);
        }
        let mut col_x = vec![x];
        for c in &widths {
            col_x.push(col_x.last().copied().unwrap_or(x) + c);
        }

        let sort = self.gui.sorts.get(&id).copied();
        let order = sorted_order(&rows, sort);
        let n = rows.len();
        let mut pos = usize::try_from(selected - 1).ok().filter(|&s| s < n).and_then(|s| order.iter().position(|&o| o == s));
        let (_, mut sy) = self.gui.scrolls.get(&id).copied().unwrap_or((0.0, 0.0));
        let mut follow = false;

        if hover && input.scroll != 0.0 {
            sy -= input.scroll as f64 * rh * WHEEL_LINES;
        }
        if input.left_pressed && hover {
            self.gui.claim();
            if self.gui.focus != Some(id) {
                self.gui.focus = Some(id);
                self.gui.editor = None;
            }
            if my < body_y {
                // a header: sort by it (again: the other way)
                if let Some(c) = (0..ncols).find(|&c| mx >= col_x[c] && mx < col_x[c + 1]) {
                    let next = match sort {
                        Some((sc, false)) if sc == c => (c, true),
                        _ => (c, false),
                    };
                    self.gui.sorts.insert(id, next);
                }
            } else {
                let row = ((my - body_y + sy) / rh).floor();
                if row >= 0.0 && (row as usize) < n {
                    let row = row as usize;
                    let now = std::time::Instant::now();
                    if let Some((lid, lrow, at)) = self.gui.last_click {
                        if lid == id && lrow == row && now.duration_since(at).as_millis() < 450 {
                            self.gui.activated = Some(id);
                        }
                    }
                    self.gui.last_click = Some((id, row, now));
                    pos = Some(row);
                }
            }
        }
        if self.gui.focus == Some(id) && n > 0 {
            let page = ((body_h / rh).floor() as usize).max(1);
            for &k in &input.keys {
                let p = pos.unwrap_or(0);
                pos = Some(match k {
                    EditKey::Up => p.saturating_sub(1),
                    EditKey::Down if pos.is_some() => (p + 1).min(n - 1),
                    EditKey::Down => 0,
                    EditKey::Home => 0,
                    EditKey::End => n - 1,
                    EditKey::PageUp => p.saturating_sub(page),
                    EditKey::PageDown => (p + page).min(n - 1),
                    EditKey::Enter => {
                        if pos.is_some() {
                            self.gui.activated = Some(id);
                        }
                        continue;
                    }
                    _ => continue,
                });
                follow = true;
            }
            // typing a letter jumps to the next row that starts with it
            for ch in &input.chars {
                let lower = ch.to_lowercase().to_string();
                let start = pos.map_or(0, |p| p + 1);
                if let Some(k) = (0..n).map(|i| (start + i) % n).find(|&i| {
                    rows[order[i]].first().is_some_and(|t| t.to_lowercase().starts_with(&lower))
                }) {
                    pos = Some(k);
                    follow = true;
                }
            }
        }
        if follow {
            if let Some(p) = pos {
                let top = p as f64 * rh;
                if top < sy {
                    sy = top;
                }
                if top + rh > sy + body_h {
                    sy = top + rh - body_h;
                }
            }
        }
        let total_h = n as f64 * rh;
        sy = sy.min((total_h - body_h).max(0.0)).max(0.0);
        self.gui.scrolls.insert(id, (0.0, sy));
        let focused = self.gui.focus == Some(id);

        // draw
        let t = self.gui.theme;
        let g = self.graphics.as_mut().ok_or_else(|| crate::builtins::NO_WINDOW.to_string())?;
        let c = &mut g.canvas;
        c.rect(x, y, w, h, t.panel);
        let saved = c.clip();
        if let Some(hs) = &headers {
            c.rect(x, y, w, head_h, t.field);
            for col in 0..ncols {
                c.set_clip(col_x[col], y, widths[col] - 1.0, head_h);
                let label = hs.get(col).map(String::as_str).unwrap_or("");
                self.fonts.draw(c, col_x[col] + pad, y + (head_h - fh) / 2.0, label, t.muted)?;
                if let Some((_, desc)) = sort.filter(|s| s.0 == col) {
                    // a small triangle: up when ascending, down when descending
                    let (ax, ay) = (col_x[col + 1] - 14.0, y + head_h / 2.0 - 2.0);
                    for i in 0..5 {
                        let half = if desc { 4.0 - i as f64 } else { i as f64 };
                        c.rect(ax - half, ay + i as f64, 2.0 * half + 1.0, 1.0, t.muted);
                    }
                }
                c.restore_clip(saved);
                if col > 0 {
                    c.rect(col_x[col], y + 4.0, 1.0, head_h - 8.0, t.border);
                }
            }
            c.rect(x, y + head_h - 1.0, w, 1.0, t.border);
        }
        c.set_clip(x + 1.0, body_y, w - 2.0, body_h - 1.0);
        let first = (sy / rh).floor() as usize;
        let last = (((sy + body_h) / rh).ceil() as usize).min(n);
        let hover_row = if hover && my >= body_y { Some(((my - body_y + sy) / rh).floor() as usize) } else { None };
        for (i, &orig) in order.iter().enumerate().take(last).skip(first) {
            let ry = body_y + i as f64 * rh - sy;
            let is_sel = pos == Some(i);
            let bg = if is_sel {
                if focused { t.accent } else { t.selection }
            } else if hover_row == Some(i) {
                t.row_hover
            } else if i % 2 == 1 {
                t.row_alt
            } else {
                t.panel
            };
            c.rect(x, ry, w, rh, bg);
            let fg = if is_sel && focused { t.accent_text } else { t.text };
            for col in 0..ncols {
                let cell = rows[orig].get(col).map(String::as_str).unwrap_or("");
                let inner = c.clip();
                let (cx0, cy0) = (col_x[col].max(inner.0 as f64), ry.max(inner.1 as f64));
                let cx1 = (col_x[col + 1] - 2.0).min(inner.2 as f64);
                let cy1 = (ry + rh).min(inner.3 as f64);
                c.set_clip(cx0, cy0, (cx1 - cx0).max(0.0), (cy1 - cy0).max(0.0));
                self.fonts.draw(c, col_x[col] + pad, ry + (rh - fh) / 2.0, cell, fg)?;
                c.restore_clip(inner);
            }
        }
        c.restore_clip(saved);
        if total_h > body_h {
            let track = body_h - 6.0;
            let thumb = (track * body_h / total_h).max(20.0);
            let top = body_y + 3.0 + (track - thumb) * (sy / (total_h - body_h));
            c.rect(x + w - 6.0, top, 4.0, thumb, t.border);
        }
        let edge = if focused { t.focus } else { t.border };
        c.rect(x, y, w, 1.0, edge);
        c.rect(x, y + h - 1.0, w, 1.0, edge);
        c.rect(x, y, 1.0, h, edge);
        c.rect(x + w - 1.0, y, 1.0, h, edge);
        Ok(pos.map_or(0, |p| order[p] as i64 + 1))
    }
}

const KIND_MENUBAR: u64 = 5;
const KIND_CONTEXT: u64 = 6;

/// One entry of a menu: "Label", "Label|Ctrl+S" (a shortcut) or "-" (a separator).
#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: Option<String>,
    pub separator: bool,
}

pub fn parse_item(s: &str) -> MenuItem {
    let s = s.trim();
    if s == "-" {
        return MenuItem { label: String::new(), shortcut: None, separator: true };
    }
    match s.split_once('|') {
        Some((l, k)) => MenuItem { label: l.trim().to_string(), shortcut: Some(k.trim().to_string()), separator: false },
        None => MenuItem { label: s.to_string(), shortcut: None, separator: false },
    }
}

/// "File: New, Open, -, Quit" -> ("File", items).
pub fn parse_menu(spec: &str) -> (String, Vec<MenuItem>) {
    match spec.split_once(':') {
        Some((title, rest)) => (title.trim().to_string(), rest.split(',').map(parse_item).filter(|i| i.separator || !i.label.is_empty()).collect()),
        None => (spec.trim().to_string(), Vec::new()),
    }
}

/// The letter of a "Ctrl+<letter>" shortcut.
fn ctrl_letter(shortcut: &str) -> Option<char> {
    let s = shortcut.trim().to_lowercase();
    let rest = s.strip_prefix("ctrl+")?;
    let mut cs = rest.chars();
    match (cs.next(), cs.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => Some(c),
        _ => None,
    }
}

/// An open menu: which widget owns it, which of its menus, and where it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenMenu {
    pub owner: u64,
    pub index: usize,
    pub x: f64,
    pub y: f64,
}

/// A drop-down to draw at the end of the frame, above everything.
#[derive(Debug, Clone)]
pub struct Popup {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub items: Vec<MenuItem>,
    pub hover: Option<usize>,
}

impl crate::builtins::Env {
    /// The size of a drop-down and the y of each item from its top.
    fn popup_layout(&mut self, items: &[MenuItem]) -> Result<(f64, f64, Vec<(f64, f64)>), String> {
        let fh = self.fonts.height();
        let (ih, sep) = ((fh * 1.75).round(), 9.0);
        let mut w: f64 = 160.0;
        for it in items {
            let lw = self.fonts.width(&it.label)? as f64;
            let kw = match &it.shortcut {
                Some(k) => self.fonts.width(k)? as f64 + 32.0,
                None => 0.0,
            };
            w = w.max(lw + kw + 40.0);
        }
        let mut y = 4.0;
        let mut rows = Vec::new();
        for it in items {
            let h = if it.separator { sep } else { ih };
            rows.push((y, h));
            y += h;
        }
        Ok((w, y + 4.0, rows))
    }

    /// Opens a drop-down at (x, y) (kept inside the window); returns the item
    /// clicked this frame, if any, and records the drop-down for drawing.
    fn run_popup(&mut self, items: &[MenuItem], x: f64, y: f64, input: &FrameInput, (mx, my): (f64, f64)) -> Result<(Option<String>, bool), String> {
        let (w, h, rows) = self.popup_layout(items)?;
        let (cw, chh) = self.graphics.as_ref().map_or((w, h), |g| (g.canvas.w as f64, g.canvas.h as f64));
        let x = x.min(cw - w - 2.0).max(0.0);
        let y = if y + h > chh { (y - h).max(0.0) } else { y };
        let inside = mx >= x && mx < x + w && my >= y && my < y + h;
        let hover = if inside {
            rows.iter().position(|&(ry, rh)| my >= y + ry && my < y + ry + rh).filter(|&i| !items[i].separator)
        } else {
            None
        };
        let mut chosen = None;
        let mut close = false;
        if input.left_pressed {
            if let Some(i) = hover {
                chosen = Some(items[i].label.clone());
                close = true;
                self.gui.claim();
            } else if inside {
                self.gui.claim(); // a separator or the border: nothing
            } else {
                close = true;
            }
        }
        if input.keys.contains(&EditKey::Escape) {
            close = true;
        }
        if !close {
            self.gui.popups.push(Popup { x, y, w, h, items: items.to_vec(), hover });
        }
        Ok((chosen, close))
    }

    /// A menu bar along the top of the window. Each menu is "Title: item, item"
    /// ("-" a separator, "Item|Ctrl+S" a shortcut). Returns the label of the
    /// item chosen this frame (by click or shortcut), or "".
    pub(crate) fn menubar(&mut self, specs: &[String]) -> Result<String, String> {
        let id = widget_id(KIND_MENUBAR, 0.0, 0.0, 0.0, 0.0);
        let menus: Vec<(String, Vec<MenuItem>)> = specs.iter().map(|s| parse_menu(s)).collect();
        let g = self.graphics.as_mut().ok_or_else(|| crate::builtins::NO_WINDOW.to_string())?;
        let input = g.input.clone();
        let (mx, my) = g.raw_mouse();
        let (mx, my) = (mx as f64, my as f64);
        let width = g.canvas.w as f64;
        let fh = self.fonts.height();
        let bar_h = (fh * 1.9).round();
        let mut titles = Vec::new();
        let mut tx = 6.0;
        for (t, _) in &menus {
            let tw = self.fonts.width(t)? as f64 + 24.0;
            titles.push((tx, tw));
            tx += tw;
        }
        let over = if my >= 0.0 && my < bar_h { titles.iter().position(|&(x, w)| mx >= x && mx < x + w) } else { None };
        let mut result = String::new();

        // shortcuts: Ctrl+letter, unless a text field needs it for editing
        if input.ctrl {
            let editing = self.gui.editor.is_some();
            for k in &input.keys {
                if let EditKey::Letter(c) = k {
                    if editing && "acvxzy".contains(*c) {
                        continue;
                    }
                    let hit = menus.iter().flat_map(|(_, items)| items).find(|it| it.shortcut.as_deref().and_then(ctrl_letter) == Some(*c));
                    if let Some(it) = hit {
                        result = it.label.clone();
                    }
                }
            }
        }

        let mut open = self.gui.menu.filter(|m| m.owner == id).map(|m| m.index);
        if input.left_pressed {
            if let Some(i) = over {
                self.gui.claim();
                open = if open == Some(i) { None } else { Some(i) };
                self.gui.menu = open.map(|i| OpenMenu { owner: id, index: i, x: titles[i].0, y: bar_h });
            }
        } else if let (Some(m), Some(i)) = (open, over) {
            if i != m {
                // moving along the bar switches menus, as in desktop apps
                open = Some(i);
                self.gui.menu = Some(OpenMenu { owner: id, index: i, x: titles[i].0, y: bar_h });
            }
        }
        if let Some(i) = open {
            let clicked_title = input.left_pressed && over.is_some();
            if !clicked_title {
                let (chosen, close) = self.run_popup(&menus[i].1, titles[i].0, bar_h, &input, (mx, my))?;
                if let Some(c) = chosen {
                    result = c;
                }
                if close {
                    self.gui.menu = None;
                    open = None;
                }
            } else {
                let items = menus[i].1.clone();
                self.run_popup(&items, titles[i].0, bar_h, &FrameInput::default(), (mx, my))?;
            }
        }

        // the bar
        let t = self.gui.theme;
        let g = self.graphics.as_mut().ok_or_else(|| crate::builtins::NO_WINDOW.to_string())?;
        let c = &mut g.canvas;
        c.rect(0.0, 0.0, width, bar_h, t.panel);
        c.rect(0.0, bar_h - 1.0, width, 1.0, t.border);
        for (i, (title, _)) in menus.iter().enumerate() {
            let (x, w) = titles[i];
            if open == Some(i) {
                c.rect(x, 2.0, w, bar_h - 4.0, t.accent);
            } else if over == Some(i) {
                c.rect(x, 2.0, w, bar_h - 4.0, t.row_hover);
            }
            let fg = if open == Some(i) { t.accent_text } else { t.text };
            self.fonts.draw(c, x + 12.0, (bar_h - fh) / 2.0, title, fg)?;
        }
        Ok(result)
    }

    /// A right-click menu for the area (x, y, w, h): it opens at the pointer;
    /// returns the label of the item chosen this frame, or "".
    pub(crate) fn context_menu(&mut self, x: f64, y: f64, w: f64, h: f64, items: &[String]) -> Result<String, String> {
        let id = widget_id(KIND_CONTEXT, x, y, w, h);
        let items: Vec<MenuItem> = items.iter().map(|s| parse_item(s)).collect();
        let g = self.graphics.as_mut().ok_or_else(|| crate::builtins::NO_WINDOW.to_string())?;
        let input = g.input.clone();
        let (mx, my) = g.raw_mouse();
        let (mx, my) = (mx as f64, my as f64);
        if input.right_pressed && mx >= x && mx < x + w && my >= y && my < y + h {
            self.gui.menu = Some(OpenMenu { owner: id, index: 0, x: mx, y: my });
            self.gui.claim();
            // the right click that opens it is not a choice
            let (px, py) = (mx, my);
            self.run_popup(&items, px, py, &FrameInput::default(), (mx, my))?;
            return Ok(String::new());
        }
        let Some(m) = self.gui.menu.filter(|m| m.owner == id) else {
            return Ok(String::new());
        };
        let (chosen, close) = self.run_popup(&items, m.x, m.y, &input, (mx, my))?;
        if close || input.right_pressed {
            self.gui.menu = None;
        }
        Ok(chosen.unwrap_or_default())
    }

    /// The end of a GUI frame (in `show`): the open drop-downs are drawn above
    /// everything, the widgets stop seeing the pointer over them, the focus
    /// moves (Tab, clicks on nothing) and the pointer changes its look.
    pub(crate) fn finish_gui_frame(&mut self) -> Result<(), String> {
        let popups = std::mem::take(&mut self.gui.popups);
        let t = self.gui.theme;
        let fh = self.fonts.height();
        if let Some(g) = self.graphics.as_mut() {
            g.blocked = popups.iter().map(|p| (p.x, p.y, p.w, p.h)).collect();
            let c = &mut g.canvas;
            let saved = c.clip();
            c.reset_clip();
            for p in &popups {
                // a soft shadow, the box, the items
                c.rect(p.x + 3.0, p.y + 4.0, p.w, p.h, if t.dark { 0x05080f } else { 0xc7cad1 });
                c.rect(p.x, p.y, p.w, p.h, t.panel);
                c.rect(p.x, p.y, p.w, 1.0, t.border);
                c.rect(p.x, p.y + p.h - 1.0, p.w, 1.0, t.border);
                c.rect(p.x, p.y, 1.0, p.h, t.border);
                c.rect(p.x + p.w - 1.0, p.y, 1.0, p.h, t.border);
                let mut iy = p.y + 4.0;
                let ih = (fh * 1.75).round();
                for (i, it) in p.items.iter().enumerate() {
                    if it.separator {
                        c.rect(p.x + 8.0, iy + 4.0, p.w - 16.0, 1.0, t.border);
                        iy += 9.0;
                        continue;
                    }
                    let hot = p.hover == Some(i);
                    if hot {
                        c.rect(p.x + 4.0, iy, p.w - 8.0, ih, t.accent);
                    }
                    let fg = if hot { t.accent_text } else { t.text };
                    self.fonts.draw(c, p.x + 16.0, iy + (ih - fh) / 2.0, &it.label, fg)?;
                    if let Some(k) = &it.shortcut {
                        let kw = self.fonts.width(k)? as f64;
                        self.fonts.draw(c, p.x + p.w - 16.0 - kw, iy + (ih - fh) / 2.0, k, if hot { t.accent_text } else { t.muted })?;
                    }
                    iy += ih;
                }
            }
            c.restore_clip(saved);
            self.gui.end_frame(&g.input);
            g.set_pointer(if self.gui.over_text { crate::graphics::Pointer::Text } else { crate::graphics::Pointer::Arrow });
        }
        Ok(())
    }
}

impl crate::builtins::Env {
    /// Starts a column: the next rows stack from (x, y), `w` wide, `gap` apart.
    pub(crate) fn column(&mut self, x: f64, y: f64, w: f64, gap: f64) {
        self.gui.column = Some((x, y, w.max(1.0), gap.max(0.0)));
    }

    /// Reserves the next row of the column, `h` high; returns its y.
    pub(crate) fn row(&mut self, h: f64) -> Result<f64, String> {
        let (x, y, w, gap) = self.gui.column.ok_or("start a column first, like gtq.column(20, 20, 400, 10)")?;
        self.gui.column = Some((x, y + h.max(0.0) + gap, w, gap));
        Ok(y)
    }

    pub(crate) fn column_part(&self, x_part: bool) -> Result<f64, String> {
        let (x, _, w, _) = self.gui.column.ok_or("start a column first, like gtq.column(20, 20, 400, 10)")?;
        Ok(if x_part { x } else { w })
    }

    /// A labelled text field on the next row of the column: the label on the
    /// left third, the field on the rest.
    pub(crate) fn form_field(&mut self, label: &str, value: &str, password: bool) -> Result<String, String> {
        let fh = self.fonts.height();
        let h = (fh * 2.1).round().max(24.0);
        let y = self.row(h)?;
        let (x, w) = (self.column_part(true)?, self.column_part(false)?);
        let label_w = (w * 0.32).round();
        let muted = self.gui.theme.muted;
        let g = self.graphics.as_mut().ok_or_else(|| crate::builtins::NO_WINDOW.to_string())?;
        self.fonts.draw(&mut g.canvas, x, y + (h - fh) / 2.0, label, muted)?;
        self.text_field(x + label_w, y, w - label_w, h, value, password)
    }
}

/// A window that is not the current one, with its own GUI state.
pub struct WindowSlot {
    pub graphics: crate::graphics::Graphics,
    pub gui: GuiState,
    pub prev_down: bool,
}

impl crate::builtins::Env {
    /// The slot index of window number `id` (1 is the main window).
    fn slot_of(&self, id: i64) -> Result<usize, String> {
        let idx = usize::try_from(id - 1).map_err(|_| format!("there is no window {}", id))?;
        let exists = idx == self.current && self.graphics.is_some() || self.windows.get(idx).is_some_and(Option::is_some);
        if !exists {
            return Err(format!("window {} is not open (it was closed, or never opened)", id));
        }
        Ok(idx)
    }

    /// Opens another window; returns its number (the main window is 1).
    pub(crate) fn new_window(&mut self, w: i64, h: i64, title: &str) -> Result<i64, String> {
        if w <= 0 || h <= 0 || w > 10_000 || h > 10_000 {
            return Err(format!("the window size must be between 1 and 10000, here it is {} x {}", w, h));
        }
        let headless = match (&self.graphics, self.windows.first()) {
            (Some(g), _) if self.current == 0 => g.is_headless(),
            (_, Some(Some(main))) => main.graphics.is_headless(),
            _ => return Err("open the main window first, with window(...)".to_string()),
        };
        if self.windows.is_empty() {
            self.windows.push(None); // the main window's place
        }
        let mut g = if headless {
            crate::graphics::Graphics::headless(w as usize, h as usize)
        } else {
            crate::graphics::Graphics::new(w as usize, h as usize, title)?
        };
        g.unthrottled();
        let mut gui = GuiState::default();
        gui.theme = self.gui.theme;
        // a free number, or a new one
        let idx = match self.windows.iter().enumerate().skip(1).find(|(i, s)| s.is_none() && *i != self.current) {
            Some((i, _)) => i,
            None => {
                self.windows.push(None);
                self.windows.len() - 1
            }
        };
        self.windows[idx] = Some(WindowSlot { graphics: g, gui, prev_down: false });
        Ok(idx as i64 + 1)
    }

    /// Makes window `id` the one the drawing and the widgets use.
    pub(crate) fn use_window(&mut self, id: i64) -> Result<(), String> {
        let idx = self.slot_of(id)?;
        if idx == self.current {
            return Ok(());
        }
        let incoming = self.windows[idx].take().expect("an open window");
        if let Some(g) = self.graphics.take() {
            let gui = std::mem::take(&mut self.gui);
            self.windows[self.current] = Some(WindowSlot { graphics: g, gui, prev_down: self.prev_down });
        }
        self.graphics = Some(incoming.graphics);
        self.gui = incoming.gui;
        self.prev_down = incoming.prev_down;
        self.current = idx;
        // the classic widgets' palette follows the theme of this window
        set_light(!self.gui.theme.dark);
        Ok(())
    }

    /// Whether window `id` is open (the user has not closed it).
    pub(crate) fn window_open(&self, id: i64) -> bool {
        let Ok(idx) = usize::try_from(id - 1) else { return false };
        if idx == self.current {
            return self.graphics.as_ref().is_some_and(|g| g.is_open());
        }
        self.windows.get(idx).and_then(Option::as_ref).is_some_and(|s| s.graphics.is_open())
    }

    /// Closes window `id` (not the main one).
    pub(crate) fn close_window_n(&mut self, id: i64) -> Result<(), String> {
        if id == 1 {
            return Err("gtq.close: the main window closes when the program ends (or the user closes it)".to_string());
        }
        let idx = self.slot_of(id)?;
        if idx == self.current {
            self.use_window(1)?;
        }
        self.windows[idx] = None;
        Ok(())
    }

    /// `show()`: ends the frame of every window and shows them all.
    pub(crate) fn show_all(&mut self) -> Result<(), String> {
        let start = self.current;
        let others: Vec<usize> = (0..self.windows.len()).filter(|&i| i != start && self.windows[i].is_some()).collect();
        for idx in std::iter::once(start).chain(others) {
            self.use_window(idx as i64 + 1)?;
            self.prev_down = self.graphics.as_ref().is_some_and(crate::graphics::Graphics::click);
            // the end of a GUI frame: menus on top, focus changes, pointer look
            self.finish_gui_frame()?;
            self.graphics.as_mut().ok_or_else(|| crate::builtins::NO_WINDOW.to_string())?.show();
        }
        self.use_window(start as i64 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EditKey::*;

    fn ed(s: &str) -> Editor {
        Editor::new(s, false)
    }

    #[test]
    fn typing_and_deleting() {
        let mut e = ed("helo");
        e.key(Left, false, false);
        e.insert("l");
        assert_eq!((e.string(), e.caret), ("hello".to_string(), 4));
        e.key(Backspace, false, false);
        e.key(End, false, false);
        e.insert(" wörld");
        assert_eq!(e.string(), "helo wörld");
        e.key(Home, false, false);
        e.key(Delete, false, false);
        assert_eq!(e.string(), "elo wörld");
    }

    #[test]
    fn selection_replace_and_shortcuts() {
        let mut e = ed("hello world");
        e.key(Home, false, false);
        e.key(Right, true, true); // Ctrl+Shift+Right selects "hello"
        assert_eq!(e.selected(), "hello");
        let r = e.key(Letter('c'), true, false);
        assert_eq!(r.copy.as_deref(), Some("hello"));
        e.insert("bye");
        assert_eq!(e.string(), "bye world");
        e.key(Letter('a'), true, false);
        let r = e.key(Letter('x'), true, false);
        assert_eq!((r.copy.as_deref(), e.string().as_str()), (Some("bye world"), ""));
        assert!(e.key(Letter('v'), true, false).paste);
    }

    #[test]
    fn word_moves_and_word_delete() {
        let mut e = ed("one two  three");
        e.key(Backspace, true, false);
        assert_eq!(e.string(), "one two  ");
        e.key(Left, true, false);
        assert_eq!(e.caret, 4);
        e.key(Delete, true, false);
        assert_eq!(e.string(), "one   ");
    }

    #[test]
    fn single_line_drops_newlines_and_submits_on_enter() {
        let mut e = ed("");
        e.insert("a\nb\tc");
        assert_eq!(e.string(), "abc");
        assert!(e.key(Enter, false, false).submitted);
    }

    #[test]
    fn text_area_lines_and_vertical_moves() {
        let mut e = Editor::new("abcd\nxy\nlong line", true);
        e.key(Letter('a'), true, false);
        e.key(Left, false, false); // collapse to the start
        e.key(Right, false, false);
        e.key(Right, false, false);
        e.key(Right, false, false); // column 3 of "abcd"
        e.key(Down, false, false); // "xy" is shorter: its end
        assert_eq!(e.caret, 7);
        e.key(Down, false, false);
        assert_eq!(e.caret, 10); // column 2 of "long line"
        e.key(Home, false, false);
        assert_eq!(e.caret, 8);
        e.key(Enter, false, false);
        assert_eq!(e.string(), "abcd\nxy\n\nlong line");
    }

    #[test]
    fn undo_and_redo() {
        let mut g = GuiState::default();
        let mut e = ed("a");
        g.remember(&e);
        e.insert("b");
        g.remember(&e);
        e.insert("c");
        g.undo(&mut e);
        assert_eq!(e.string(), "ab");
        g.undo(&mut e);
        assert_eq!(e.string(), "a");
        g.redo(&mut e);
        assert_eq!(e.string(), "ab");
    }

    #[test]
    fn tab_cycles_focus_and_a_click_on_nothing_drops_it() {
        let mut g = GuiState::default();
        let (a, b) = (widget_id(1, 0.0, 0.0, 10.0, 10.0), widget_id(1, 0.0, 20.0, 10.0, 10.0));
        let tab = FrameInput { keys: vec![Tab], ..Default::default() };
        g.register(a);
        g.register(b);
        g.end_frame(&tab);
        assert_eq!(g.focus, Some(a));
        g.register(a);
        g.register(b);
        g.end_frame(&tab);
        assert_eq!(g.focus, Some(b));
        g.register(a);
        g.register(b);
        g.end_frame(&FrameInput { left_pressed: true, ..Default::default() });
        assert_eq!(g.focus, None);
    }
}

/// The widgets themselves, driven frame by frame with simulated input on a
/// headless window: the same code a program runs.
#[cfg(all(test, feature = "native"))]
mod widget_tests {
    use crate::builtins::{Builtin, Env};
    use crate::graphics::{EditKey, FrameInput, Graphics};
    use crate::vm::Value;

    struct Form {
        env: Env,
        fields: [String; 2],
        secret: String,
    }

    impl Form {
        fn new() -> Form {
            let mut env = Env::default();
            env.graphics = Some(Graphics::headless(400, 240));
            let mut f = Form { env, fields: ["Ada".into(), "Lovelace".into()], secret: String::new() };
            f.draw();
            f
        }

        /// Ends the frame and starts the next with this input and pointer.
        fn step(&mut self, x: i64, y: i64, input: FrameInput) {
            self.env.graphics.as_mut().unwrap().simulate(x, y, input);
            self.env.call(Builtin::Show, &[]).unwrap();
            self.draw();
        }

        /// The program's frame: two text fields and a password field.
        fn draw(&mut self) {
            for (i, f) in self.fields.iter_mut().enumerate() {
                let y = 10.0 + 50.0 * i as f64;
                let args = [Value::Real(10.0), Value::Real(y), Value::Real(300.0), Value::Real(32.0), Value::Str(f.as_str().into())];
                *f = crate::vm::display(&self.env.call(Builtin::GtqTextbox, &args).unwrap());
            }
            let args =
                [Value::Real(10.0), Value::Real(110.0), Value::Real(300.0), Value::Real(32.0), Value::Str(self.secret.as_str().into())];
            self.secret = crate::vm::display(&self.env.call(Builtin::GtqPassword, &args).unwrap());
        }

        fn submitted(&mut self) -> bool {
            matches!(self.env.call(Builtin::GtqSubmitted, &[]).unwrap(), Value::Bool(true))
        }

        fn click(&mut self, x: i64, y: i64) {
            self.step(x, y, FrameInput { left: true, ..Default::default() });
            self.step(x, y, FrameInput::default());
        }

        fn typ(&mut self, s: &str) {
            self.step(0, 0, FrameInput { chars: s.chars().collect(), ..Default::default() });
        }

        fn key(&mut self, k: EditKey, shift: bool) {
            self.step(0, 0, FrameInput { keys: vec![k], shift, ..Default::default() });
        }
    }

    #[test]
    fn click_type_tab_and_click_away() {
        let mut f = Form::new();
        // click at the right end of the first field, then type
        f.click(305, 25);
        f.typ(" B.");
        assert_eq!(f.fields, ["Ada B.".to_string(), "Lovelace".to_string()]);
        // Tab: the second field, all selected, so typing replaces it
        f.key(EditKey::Tab, false);
        f.typ("King");
        assert_eq!(f.fields[1], "King");
        // Shift+Tab goes back; Backspace on the selection empties it
        f.key(EditKey::Tab, true);
        f.key(EditKey::Backspace, false);
        assert_eq!(f.fields[0], "");
        // a click on nothing drops the focus: typing changes nothing
        f.click(380, 220);
        f.typ("zzz");
        assert_eq!(f.fields, ["".to_string(), "King".to_string()]);
    }

    #[test]
    fn selection_with_shift_arrows_and_undo() {
        let mut f = Form::new();
        f.click(305, 75); // the end of "Lovelace"
        for _ in 0..4 {
            f.key(EditKey::Left, true);
        }
        f.typ("ly"); // replaces "lace"
        assert_eq!(f.fields[1], "Lovely");
        // Ctrl+Z undoes the typing (one step), Ctrl+Y redoes it
        f.step(0, 0, FrameInput { keys: vec![EditKey::Letter('z')], ctrl: true, ..Default::default() });
        assert_eq!(f.fields[1], "Lovelace");
        f.step(0, 0, FrameInput { keys: vec![EditKey::Letter('y')], ctrl: true, ..Default::default() });
        assert_eq!(f.fields[1], "Lovely");
    }

    #[test]
    fn enter_submits_and_password_returns_the_real_text() {
        let mut f = Form::new();
        f.click(100, 125);
        f.typ("s3cret");
        assert_eq!(f.secret, "s3cret");
        f.key(EditKey::Enter, false);
        assert!(f.submitted(), "Enter in a field is reported for this frame");
        f.step(0, 0, FrameInput::default());
        assert!(!f.submitted(), "and only for that frame");
    }

    /// A program with one text area.
    struct Notes {
        env: Env,
        text: String,
    }

    impl Notes {
        fn new(text: &str) -> Notes {
            let mut env = Env::default();
            env.graphics = Some(Graphics::headless(400, 240));
            let mut n = Notes { env, text: text.into() };
            n.draw();
            n
        }
        fn draw(&mut self) {
            let args = [Value::Real(10.0), Value::Real(10.0), Value::Real(300.0), Value::Real(120.0), Value::Str(self.text.as_str().into())];
            self.text = crate::vm::display(&self.env.call(Builtin::GtqTextarea, &args).unwrap());
        }
        fn step(&mut self, x: i64, y: i64, input: FrameInput) {
            self.env.graphics.as_mut().unwrap().simulate(x, y, input);
            self.env.call(Builtin::Show, &[]).unwrap();
            self.draw();
        }
        fn keys(&mut self, keys: &[EditKey], shift: bool) {
            for &k in keys {
                self.step(0, 0, FrameInput { keys: vec![k], shift, ..Default::default() });
            }
        }
        fn typ(&mut self, s: &str) {
            self.step(0, 0, FrameInput { chars: s.chars().collect(), ..Default::default() });
        }
    }

    #[test]
    fn text_area_lines_arrows_and_selection() {
        let mut n = Notes::new("");
        n.step(100, 60, FrameInput { left: true, ..Default::default() });
        n.step(100, 60, FrameInput::default());
        n.typ("hello");
        n.keys(&[EditKey::Enter], false);
        n.typ("world");
        assert_eq!(n.text, "hello
world");
        // up to the first line, end of it, then type there
        n.keys(&[EditKey::Up, EditKey::End], false);
        n.typ("!");
        assert_eq!(n.text, "hello!
world");
        // Shift+Down selects across the line break; typing replaces the selection
        n.keys(&[EditKey::Home], false);
        n.keys(&[EditKey::Down], true);
        n.typ("X");
        assert_eq!(n.text, "Xworld");
    }

    #[test]
    fn text_area_scrolls_without_trouble() {
        let long: Vec<String> = (1..=60).map(|i| format!("line {}", i)).collect();
        let mut n = Notes::new(&long.join("
"));
        // the wheel over the area, then Page Down and Ctrl+End with the focus
        n.step(100, 60, FrameInput { scroll: -2.0, ..Default::default() });
        n.step(100, 60, FrameInput { left: true, ..Default::default() });
        n.step(100, 60, FrameInput::default());
        n.keys(&[EditKey::PageDown, EditKey::PageDown], false);
        n.step(0, 0, FrameInput { keys: vec![EditKey::End], ctrl: true, ..Default::default() });
        n.typ("!");
        assert!(n.text.ends_with("line 60!"));
    }

    /// A program with a list box and a table.
    struct Lists {
        env: Env,
        pick: i64,
        row: i64,
        opened: bool,
    }

    impl Lists {
        fn new() -> Lists {
            let mut env = Env::default();
            env.graphics = Some(Graphics::headless(500, 400));
            let mut l = Lists { env, pick: 0, row: 0, opened: false };
            l.draw();
            l
        }
        fn list(items: &[&str]) -> Value {
            Value::Seq(std::rc::Rc::new(std::cell::RefCell::new(items.iter().map(|s| Value::Str((*s).into())).collect())))
        }
        fn draw(&mut self) {
            let fruit = Lists::list(&["apple", "banana", "cherry", "blueberry", "date"]);
            let a = [Value::Real(10.0), Value::Real(10.0), Value::Real(200.0), Value::Real(150.0), fruit, Value::Int(self.pick)];
            self.pick = match self.env.call(Builtin::GtqListbox, &a).unwrap() {
                Value::Int(n) => n,
                _ => unreachable!(),
            };
            let headers = Lists::list(&["Name", "Age"]);
            let rows = Value::Seq(std::rc::Rc::new(std::cell::RefCell::new(vec![
                Lists::list(&["Ada", "36"]),
                Lists::list(&["Alan", "41"]),
                Lists::list(&["Grace", "9"]),
            ])));
            let a = [Value::Real(10.0), Value::Real(200.0), Value::Real(300.0), Value::Real(150.0), headers, rows, Value::Int(self.row)];
            self.row = match self.env.call(Builtin::GtqTable, &a).unwrap() {
                Value::Int(n) => n,
                _ => unreachable!(),
            };
            self.opened = matches!(self.env.call(Builtin::GtqActivated, &[]).unwrap(), Value::Bool(true));
        }
        fn step(&mut self, x: i64, y: i64, input: FrameInput) {
            self.env.graphics.as_mut().unwrap().simulate(x, y, input);
            self.env.call(Builtin::Show, &[]).unwrap();
            self.draw();
        }
        fn click(&mut self, x: i64, y: i64) {
            self.step(x, y, FrameInput { left: true, ..Default::default() });
            self.step(x, y, FrameInput::default());
        }
        fn key(&mut self, k: EditKey) {
            self.step(0, 0, FrameInput { keys: vec![k], ..Default::default() });
        }
        /// The y of the middle of row `i` (0-based, as displayed) of the list.
        fn list_row(&mut self, i: usize) -> i64 {
            let rh = (self.env.fonts.height() * 1.6).round();
            (10.0 + rh * i as f64 + rh / 2.0) as i64
        }
    }

    #[test]
    fn list_click_keys_letters_and_double_click() {
        let mut l = Lists::new();
        let y = l.list_row(1);
        l.click(50, y);
        assert_eq!(l.pick, 2, "the second item: banana");
        l.key(EditKey::Down);
        l.key(EditKey::Down);
        assert_eq!(l.pick, 4);
        l.key(EditKey::Home);
        assert_eq!(l.pick, 1);
        // typing "b" jumps to the next item starting with b
        l.step(0, 0, FrameInput { chars: vec!['b'], ..Default::default() });
        assert_eq!(l.pick, 2);
        l.step(0, 0, FrameInput { chars: vec!['b'], ..Default::default() });
        assert_eq!(l.pick, 4, "then to blueberry");
        // a double-click activates, for one frame
        let y = l.list_row(2);
        l.click(50, y);
        assert!(!l.opened);
        l.step(50, y, FrameInput { left: true, ..Default::default() });
        assert!(l.opened);
        l.step(50, y, FrameInput::default());
        assert!(!l.opened);
    }

    #[test]
    fn table_sorts_by_header_but_returns_the_original_row() {
        let mut l = Lists::new();
        let rh = (l.env.fonts.height() * 1.6).round();
        let head = (200.0 + rh / 2.0) as i64;
        let first_row = (200.0 + rh + rh / 2.0) as i64;
        // sort by Age (the second column): 9, 36, 41
        l.click(250, head);
        l.click(50, first_row);
        assert_eq!(l.row, 3, "Grace (age 9) is the third row of the data");
        // again: descending, 41 first
        l.click(250, head);
        l.click(50, first_row);
        assert_eq!(l.row, 2, "Alan");
        // Enter on the focused table activates the selected row
        l.key(EditKey::Enter);
        assert!(l.opened);
    }

    /// A program with a menu bar, a button right under the File menu, a text
    /// field and an area with a right-click menu.
    struct App {
        env: Env,
        chosen: String,
        context: String,
        pressed: bool,
        field: String,
    }

    impl App {
        fn new() -> App {
            let mut env = Env::default();
            env.graphics = Some(Graphics::headless(500, 400));
            let mut a = App { env, chosen: String::new(), context: String::new(), pressed: false, field: "abc".into() };
            a.draw();
            a
        }
        fn list(items: &[&str]) -> Value {
            Value::Seq(std::rc::Rc::new(std::cell::RefCell::new(items.iter().map(|s| Value::Str((*s).into())).collect())))
        }
        fn draw(&mut self) {
            let menus = App::list(&["File: New, Open, Save|Ctrl+S, -, Quit", "Edit: Copy|Ctrl+C, Paste"]);
            self.chosen = crate::vm::display(&self.env.call(Builtin::GtqMenubar, &[menus]).unwrap());
            // a button where the File menu drops down
            let b = [Value::Real(0.0), Value::Real(30.0), Value::Real(300.0), Value::Real(200.0), Value::Str("under".into())];
            self.pressed = matches!(self.env.call(Builtin::GtqButton, &b).unwrap(), Value::Bool(true));
            let f = [Value::Real(320.0), Value::Real(60.0), Value::Real(160.0), Value::Real(30.0), Value::Str(self.field.as_str().into())];
            self.field = crate::vm::display(&self.env.call(Builtin::GtqTextbox, &f).unwrap());
            let c = [Value::Real(320.0), Value::Real(200.0), Value::Real(160.0), Value::Real(150.0), App::list(&["Rename", "-", "Delete"])];
            self.context = crate::vm::display(&self.env.call(Builtin::GtqContextMenu, &c).unwrap());
        }
        fn step(&mut self, x: i64, y: i64, input: FrameInput) {
            self.env.graphics.as_mut().unwrap().simulate(x, y, input);
            self.env.call(Builtin::Show, &[]).unwrap();
            self.draw();
        }
        /// Presses and releases the left button at (x, y); true if anything
        /// was chosen or the button fired during the two frames.
        fn click(&mut self, x: i64, y: i64) -> (String, bool) {
            self.step(x, y, FrameInput { left: true, ..Default::default() });
            let r = (self.chosen.clone(), self.pressed);
            self.step(x, y, FrameInput::default());
            r
        }
        /// The y of the middle of item `i` of a drop-down opened at `top`.
        fn item_y(&mut self, top: f64, i: usize) -> i64 {
            let ih = (self.env.fonts.height() * 1.75).round();
            (top + 4.0 + ih * i as f64 + ih / 2.0) as i64
        }
        fn bar_h(&mut self) -> f64 {
            (self.env.fonts.height() * 1.9).round()
        }
    }

    #[test]
    fn menu_items_parse() {
        let (t, items) = super::parse_menu("File: New, Save|Ctrl+S, -, Quit");
        assert_eq!(t, "File");
        assert_eq!(items.len(), 4);
        assert_eq!(items[1].shortcut.as_deref(), Some("Ctrl+S"));
        assert!(items[2].separator);
        assert_eq!(super::ctrl_letter("ctrl+s"), Some('s'));
        assert_eq!(super::ctrl_letter("Alt+F4"), None);
    }

    #[test]
    fn menubar_opens_chooses_and_blocks_what_is_under_it() {
        let mut a = App::new();
        let bar = a.bar_h();
        // open File, then click "Open": the item is chosen, the button under it is not pressed
        a.click(20, (bar / 2.0) as i64);
        let y = a.item_y(bar, 1);
        let (chosen, pressed) = a.click(40, y);
        assert_eq!(chosen, "Open");
        assert!(!pressed, "the click went to the menu, not to the button under it");
        // the menu is closed now: the same spot presses the button
        let (_, pressed) = a.click(40, y);
        assert!(pressed);
        // open, then Escape closes it without choosing
        a.click(20, (bar / 2.0) as i64);
        a.step(0, 0, FrameInput { keys: vec![EditKey::Escape], ..Default::default() });
        let y0 = a.item_y(bar, 0);
        let (chosen, _) = a.click(40, y0);
        assert_eq!(chosen, "");
    }

    #[test]
    fn shortcuts_fire_but_leave_editing_keys_to_a_focused_field() {
        let mut a = App::new();
        a.step(0, 0, FrameInput { keys: vec![EditKey::Letter('s')], ctrl: true, ..Default::default() });
        assert_eq!(a.chosen, "Save");
        a.step(0, 0, FrameInput { keys: vec![EditKey::Letter('c')], ctrl: true, ..Default::default() });
        assert_eq!(a.chosen, "Copy", "no field has the focus: Ctrl+C is the menu's");
        a.click(400, 75); // focus the field
        a.step(0, 0, FrameInput { keys: vec![EditKey::Letter('c')], ctrl: true, ..Default::default() });
        assert_eq!(a.chosen, "", "the focused field keeps Ctrl+C for copying");
        a.step(0, 0, FrameInput { keys: vec![EditKey::Letter('s')], ctrl: true, ..Default::default() });
        assert_eq!(a.chosen, "Save", "other shortcuts still work");
    }

    #[test]
    fn context_menu_opens_on_right_click() {
        let mut a = App::new();
        a.step(350, 250, FrameInput { right: true, ..Default::default() });
        a.step(350, 250, FrameInput::default());
        assert_eq!(a.context, "");
        let y = a.item_y(250.0, 0);
        a.step(360, y, FrameInput { left: true, ..Default::default() });
        assert_eq!(a.context, "Rename");
        // a right click outside the area opens nothing
        a.step(100, 380, FrameInput { right: true, ..Default::default() });
        a.step(100, 380, FrameInput::default());
        a.step(110, 390, FrameInput { left: true, ..Default::default() });
        assert_eq!(a.context, "");
    }

    #[test]
    fn column_rows_stack_and_form_fields_edit() {
        let mut env = Env::default();
        env.graphics = Some(Graphics::headless(400, 300));
        let r = |v: Value| match v {
            Value::Real(x) => x,
            other => panic!("{:?}", other),
        };
        env.call(Builtin::GtqColumn, &[Value::Real(20.0), Value::Real(50.0), Value::Real(300.0), Value::Real(10.0)]).unwrap();
        assert_eq!(r(env.call(Builtin::GtqRow, &[Value::Real(30.0)]).unwrap()), 50.0);
        assert_eq!(r(env.call(Builtin::GtqRow, &[Value::Real(20.0)]).unwrap()), 90.0);
        assert_eq!(r(env.call(Builtin::GtqColumnX, &[]).unwrap()), 20.0);
        assert_eq!(r(env.call(Builtin::GtqColumnW, &[]).unwrap()), 300.0);
        // a form field is a row: click its right part and type
        env.call(Builtin::GtqColumn, &[Value::Real(20.0), Value::Real(50.0), Value::Real(300.0), Value::Real(10.0)]).unwrap();
        let v = env.call(Builtin::GtqFormText, &[Value::Str("Name".into()), Value::Str("".into())]).unwrap();
        assert_eq!(crate::vm::display(&v), "");
        let mut value = String::new();
        for input in [
            FrameInput { left: true, ..Default::default() },
            FrameInput::default(),
            FrameInput { chars: "Ada".chars().collect(), ..Default::default() },
        ] {
            env.graphics.as_mut().unwrap().simulate(250, 60, input);
            env.call(Builtin::Show, &[]).unwrap();
            env.call(Builtin::GtqColumn, &[Value::Real(20.0), Value::Real(50.0), Value::Real(300.0), Value::Real(10.0)]).unwrap();
            let v = env.call(Builtin::GtqFormText, &[Value::Str("Name".into()), Value::Str(value.as_str().into())]).unwrap();
            value = crate::vm::display(&v);
        }
        assert_eq!(value, "Ada");
        // a row without a column is a clear error
        let mut fresh = Env::default();
        fresh.graphics = Some(Graphics::headless(10, 10));
        assert!(fresh.call(Builtin::GtqRow, &[Value::Real(5.0)]).unwrap_err().contains("gtq.column"));
    }

    #[test]
    fn light_theme_translates_the_classic_palette() {
        super::set_light(false);
        assert_eq!(super::ui_color("#1e293b").unwrap(), 0x1e293b);
        super::set_light(true);
        assert_eq!(super::ui_color("#1e293b").unwrap(), 0xffffff, "panels turn white");
        assert_eq!(super::ui_color("#e2e8f0").unwrap(), 0x111827, "text turns dark");
        assert_eq!(super::ui_color("#ff8800").unwrap(), 0xff8800, "other colours stay");
        super::set_light(false);
    }

    #[test]
    fn several_windows_keep_their_own_input_and_focus() {
        let mut env = Env::default();
        env.graphics = Some(Graphics::headless(300, 200));
        let id = match env.call(Builtin::GtqWindow, &[Value::Int(200), Value::Int(100), Value::Str("Second".into())]).unwrap() {
            Value::Int(n) => n,
            other => panic!("{:?}", other),
        };
        assert_eq!(id, 2);
        let field = |env: &mut Env, v: &str| -> String {
            let a = [Value::Real(10.0), Value::Real(10.0), Value::Real(150.0), Value::Real(30.0), Value::Str(v.into())];
            crate::vm::display(&env.call(Builtin::GtqTextbox, &a).unwrap())
        };
        let (mut main_text, mut second_text) = (String::from("main"), String::from("second"));
        // the user clicks the field in window 2 and types; window 1 gets nothing
        let frames = [
            FrameInput { left: true, ..Default::default() },
            FrameInput::default(),
            FrameInput { chars: vec!['!'], ..Default::default() },
        ];
        for input in frames {
            env.call(Builtin::GtqUse, &[Value::Int(2)]).unwrap();
            env.graphics.as_mut().unwrap().simulate(150, 20, input);
            env.call(Builtin::GtqUse, &[Value::Int(1)]).unwrap();
            env.call(Builtin::Show, &[]).unwrap();
            main_text = field(&mut env, &main_text);
            env.call(Builtin::GtqUse, &[Value::Int(2)]).unwrap();
            second_text = field(&mut env, &second_text);
            env.call(Builtin::GtqUse, &[Value::Int(1)]).unwrap();
        }
        assert_eq!((main_text.as_str(), second_text.as_str()), ("main", "second!"));
        // each window keeps its theme, also for the classic widgets
        env.call(Builtin::GtqUse, &[Value::Int(2)]).unwrap();
        env.call(Builtin::GtqTheme, &[Value::Str("light".into())]).unwrap();
        env.call(Builtin::GtqUse, &[Value::Int(1)]).unwrap();
        assert_eq!(super::ui_color("#1e293b").unwrap(), 0x1e293b, "window 1 is still dark");
        env.call(Builtin::GtqUse, &[Value::Int(2)]).unwrap();
        assert_eq!(super::ui_color("#1e293b").unwrap(), 0xffffff, "window 2 is light");
        env.call(Builtin::GtqUse, &[Value::Int(1)]).unwrap();
        // closing: window 2 is gone, the main one cannot be closed this way
        let open = |env: &mut Env, n: i64| matches!(env.call(Builtin::GtqIsOpen, &[Value::Int(n)]).unwrap(), Value::Bool(true));
        assert!(open(&mut env, 2));
        env.call(Builtin::GtqClose, &[Value::Int(2)]).unwrap();
        assert!(!open(&mut env, 2));
        assert!(env.call(Builtin::GtqUse, &[Value::Int(2)]).unwrap_err().contains("not open"));
        assert!(env.call(Builtin::GtqClose, &[Value::Int(1)]).is_err());
        assert!(open(&mut env, 1));
        // the freed number is used again
        let again = env.call(Builtin::GtqWindow, &[Value::Int(50), Value::Int(50), Value::Str("Third".into())]).unwrap();
        assert!(matches!(again, Value::Int(2)));
    }

    #[test]
    fn the_program_can_change_the_text_of_the_focused_field() {
        let mut f = Form::new();
        f.click(305, 25);
        f.fields[0] = String::new(); // the program clears it (say, after "Send")
        f.step(0, 0, FrameInput::default());
        f.typ("new");
        assert_eq!(f.fields[0], "new");
    }

    /// A picture of the widgets, for looking at them: run with
    /// `GLOBW_GALLERY=<folder> cargo test --lib gallery -- --ignored`.
    #[test]
    #[ignore]
    fn gallery() {
        let dir = std::env::var("GLOBW_GALLERY").unwrap_or_else(|_| ".".into());
        for theme in ["dark", "light"] {
            let mut env = Env::default();
            env.graphics = Some(Graphics::headless(520, 330));
            env.call(Builtin::GtqTheme, &[Value::Str(theme.into())]).unwrap();
            let mut name = String::from("Ada Lovelace");
            let mut notes = String::from("Notes on the Analytical Engine
It weaves algebraic patterns
just as the Jacquard loom
weaves flowers and leaves.

line 6
line 7
line 8
line 9");
            let frames: Vec<(i64, i64, FrameInput)> = vec![
                (300, 120, FrameInput { left: true, ..Default::default() }), // click in the area
                (300, 120, FrameInput::default()),
                (0, 0, FrameInput { keys: vec![EditKey::Down, EditKey::Home], ..Default::default() }),
                (0, 0, FrameInput { keys: vec![EditKey::Down, EditKey::Right, EditKey::Right], shift: true, ..Default::default() }),
                (0, 0, FrameInput::default()),
            ];
            for (x, y, input) in frames {
                env.gui.blink = std::time::Instant::now(); // the caret visible in the picture
                env.graphics.as_mut().unwrap().simulate(x, y, input);
                env.call(Builtin::Show, &[]).unwrap();
                let bg = env.call(Builtin::GtqThemeColor, &[Value::Str("window".into())]).unwrap();
                env.call(Builtin::Clear, &[bg]).unwrap();
                let a = [Value::Real(20.0), Value::Real(20.0), Value::Real(480.0), Value::Real(34.0), Value::Str(name.as_str().into())];
                name = crate::vm::display(&env.call(Builtin::GtqTextbox, &a).unwrap());
                let a = [Value::Real(20.0), Value::Real(70.0), Value::Real(480.0), Value::Real(130.0), Value::Str(notes.as_str().into())];
                notes = crate::vm::display(&env.call(Builtin::GtqTextarea, &a).unwrap());
            }
            let png = env.graphics.as_ref().unwrap().canvas.to_png();
            std::fs::write(format!("{}/gallery_{}.png", dir, theme), png).unwrap();
        }
    }

    #[test]
    #[ignore]
    fn gallery_lists() {
        let dir = std::env::var("GLOBW_GALLERY").unwrap_or_else(|_| ".".into());
        let list = |items: &[&str]| Value::Seq(std::rc::Rc::new(std::cell::RefCell::new(items.iter().map(|s| Value::Str((*s).into())).collect())));
        for theme in ["dark", "light"] {
            let mut env = Env::default();
            env.graphics = Some(Graphics::headless(620, 260));
            env.call(Builtin::GtqTheme, &[Value::Str(theme.into())]).unwrap();
            let (mut pick, mut row) = (3i64, 0i64);
            let rh = (env.fonts.height() * 1.6).round();
            let frames = vec![
                (420, (20.0 + rh / 2.0) as i64, FrameInput { left: true, ..Default::default() }), // sort by Age
                (420, (20.0 + rh / 2.0) as i64, FrameInput::default()),
                (300, (20.0 + rh * 2.5) as i64, FrameInput { left: true, ..Default::default() }), // select a row
                (300, (20.0 + rh * 2.5) as i64, FrameInput::default()),
                (300, (20.0 + rh * 4.5) as i64, FrameInput::default()), // hover another
            ];
            let fruit = ["apple", "banana", "cherry", "date", "elderberry", "fig", "grape", "kiwi", "lemon", "mango"];
            for (x, y, input) in frames {
                env.graphics.as_mut().unwrap().simulate(x, y, input);
                env.call(Builtin::Show, &[]).unwrap();
                let bg = env.call(Builtin::GtqThemeColor, &[Value::Str("window".into())]).unwrap();
                env.call(Builtin::Clear, &[bg]).unwrap();
                let a = [Value::Real(20.0), Value::Real(20.0), Value::Real(170.0), Value::Real(220.0), list(&fruit), Value::Int(pick)];
                pick = match env.call(Builtin::GtqListbox, &a).unwrap() { Value::Int(n) => n, _ => 0 };
                let rows = Value::Seq(std::rc::Rc::new(std::cell::RefCell::new(vec![
                    list(&["Ada Lovelace", "36", "London"]),
                    list(&["Alan Turing", "41", "Wilmslow"]),
                    list(&["Grace Hopper", "85", "Arlington"]),
                    list(&["Katherine Johnson", "101", "Newport News"]),
                    list(&["Edsger Dijkstra", "72", "Nuenen"]),
                    list(&["Barbara Liskov", "86", "Boston"]),
                ])));
                let a = [Value::Real(210.0), Value::Real(20.0), Value::Real(390.0), Value::Real(220.0), list(&["Name", "Age", "City"]), rows, Value::Int(row)];
                row = match env.call(Builtin::GtqTable, &a).unwrap() { Value::Int(n) => n, _ => 0 };
            }
            let png = env.graphics.as_ref().unwrap().canvas.to_png();
            std::fs::write(format!("{}/gallery_lists_{}.png", dir, theme), png).unwrap();
        }
    }

    #[test]
    #[ignore]
    fn gallery_menus() {
        let dir = std::env::var("GLOBW_GALLERY").unwrap_or_else(|_| ".".into());
        let list = |items: &[&str]| Value::Seq(std::rc::Rc::new(std::cell::RefCell::new(items.iter().map(|s| Value::Str((*s).into())).collect())));
        for theme in ["dark", "light"] {
            let mut env = Env::default();
            env.graphics = Some(Graphics::headless(560, 300));
            env.call(Builtin::GtqTheme, &[Value::Str(theme.into())]).unwrap();
            let fh = env.fonts.height();
            let (bar, ih) = ((fh * 1.9).round(), (fh * 1.75).round());
            let save_y = (bar + 4.0 + ih * 2.5) as i64;
            let frames = vec![
                (20, (bar / 2.0) as i64, FrameInput { left: true, ..Default::default() }),
                (20, (bar / 2.0) as i64, FrameInput::default()),
                (60, save_y, FrameInput::default()),
            ];
            for (x, y, input) in frames {
                env.graphics.as_mut().unwrap().simulate(x, y, input);
                env.call(Builtin::Show, &[]).unwrap();
                let bg = env.call(Builtin::GtqThemeColor, &[Value::Str("window".into())]).unwrap();
                env.call(Builtin::Clear, &[bg]).unwrap();
                let menus = list(&["File: New|Ctrl+N, Open...|Ctrl+O, Save|Ctrl+S, Save as..., -, Quit|Ctrl+Q", "Edit: Undo|Ctrl+Z, Redo|Ctrl+Y, -, Cut, Copy, Paste", "View: Dark theme, Light theme", "Help: About Globw"]);
                env.call(Builtin::GtqMenubar, &[menus]).unwrap();
                let c = [Value::Real(330.0), Value::Real(100.0), Value::Real(200.0), Value::Real(150.0), list(&["Rename", "Duplicate", "-", "Remove|Ctrl+D"])];
                env.call(Builtin::GtqContextMenu, &c).unwrap();
                let f = [Value::Real(20.0), Value::Real(250.0), Value::Real(300.0), Value::Real(32.0), Value::Str("a field under the menu".into())];
                env.call(Builtin::GtqTextbox, &f).unwrap();
            }
            // the frame ends: the open menus are drawn on top
            env.graphics.as_mut().unwrap().simulate(60, save_y, FrameInput::default());
            env.call(Builtin::Show, &[]).unwrap();
            let png = env.graphics.as_ref().unwrap().canvas.to_png();
            std::fs::write(format!("{}/gallery_menus_{}.png", dir, theme), png).unwrap();
        }
    }
}
