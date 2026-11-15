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
}
