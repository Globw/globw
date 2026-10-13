//! Built-in procedures and functions beyond input/output and lists:
//! the graphics window, dialog boxes, colors, random numbers and time.
//! The checker reads their signatures here; the VM calls `Env::call`.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::canvas::parse_color;
use crate::dialogs;
use crate::graphics::Graphics;
use crate::render3d::Vec3;
use crate::tir::BaseType::{self, Bool, Int, Real, Str};
use crate::vm::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    Window,
    Open,
    Show,
    Clear,
    Rect,
    Circle,
    Line,
    Text,
    Key,
    MouseX,
    MouseY,
    Click,
    Message,
    Confirm,
    Ask,
    Rgb,
    Random,
    Time,
    Camera,
    Rotate,
    Cube,
    Box3,
    Sphere,
    Triangle3,
    Sin,
    Cos,
    Sqrt,
    Round,
}

use Builtin::*;

const ALL: [Builtin; 28] = [
    Window, Open, Show, Clear, Rect, Circle, Line, Text, Key, MouseX, MouseY, Click, Message, Confirm, Ask, Rgb, Random,
    Time, Camera, Rotate, Cube, Box3, Sphere, Triangle3, Sin, Cos, Sqrt, Round,
];

impl Builtin {
    pub fn name(self) -> &'static str {
        match self {
            Window => "window",
            Open => "open",
            Show => "show",
            Clear => "clear",
            Rect => "rect",
            Circle => "circle",
            Line => "line",
            Text => "text",
            Key => "key",
            MouseX => "mouse_x",
            MouseY => "mouse_y",
            Click => "click",
            Message => "message",
            Confirm => "confirm",
            Ask => "ask",
            Rgb => "rgb",
            Random => "random",
            Time => "time",
            Camera => "camera",
            Rotate => "rotate",
            Cube => "cube",
            Box3 => "box",
            Sphere => "sphere",
            Triangle3 => "triangle3d",
            Sin => "sin",
            Cos => "cos",
            Sqrt => "sqrt",
            Round => "round",
        }
    }

    pub fn params(self) -> &'static [BaseType] {
        match self {
            Window => &[Int, Int, Str],
            Open | Show | MouseX | MouseY | Click | Time => &[],
            Clear | Key | Message | Confirm | Ask => &[Str],
            Rect => &[Real, Real, Real, Real, Str],
            Circle => &[Real, Real, Real, Str],
            Line => &[Real, Real, Real, Real, Str],
            Text => &[Real, Real, Str, Str],
            Rgb => &[Int, Int, Int],
            Random => &[Int, Int],
            Camera => &[Real, Real, Real, Real, Real],
            Rotate => &[Real, Real, Real],
            Cube | Sphere => &[Real, Real, Real, Real, Str],
            Box3 => &[Real, Real, Real, Real, Real, Real, Str],
            Triangle3 => &[Real, Real, Real, Real, Real, Real, Real, Real, Real, Str],
            Sin | Cos | Sqrt | Round => &[Real],
        }
    }

    /// The return type of a function; `None` for a procedure.
    pub fn ret(self) -> Option<BaseType> {
        match self {
            Open | Key | Click | Confirm => Some(Bool),
            MouseX | MouseY | Random => Some(Int),
            Ask | Rgb => Some(Str),
            Time | Sin | Cos | Sqrt => Some(Real),
            Round => Some(Int),
            Window | Show | Clear | Rect | Circle | Line | Text | Message | Camera | Rotate | Cube | Box3 | Sphere
            | Triangle3 => None,
        }
    }

    pub fn id(self) -> u16 {
        ALL.iter().position(|&b| b == self).expect("every builtin is listed") as u16
    }

    pub fn from_id(id: u16) -> Builtin {
        ALL[id as usize]
    }
}

pub fn lookup(name: &str) -> Option<Builtin> {
    ALL.iter().copied().find(|b| b.name() == name)
}

const NO_WINDOW: &str = "there is no window: call window(...) first";

fn int(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        other => panic!("internal error: expected an int, found {:?}", other),
    }
}

fn real(v: &Value) -> f64 {
    match v {
        Value::Real(r) => *r,
        other => panic!("internal error: expected a float, found {:?}", other),
    }
}

fn text(v: &Value) -> &str {
    match v {
        Value::Str(s) => s,
        other => panic!("internal error: expected a string, found {:?}", other),
    }
}

/// Run-time state of the built-ins: the window, the clock and the random generator.
pub struct Env {
    graphics: Option<Graphics>,
    start: Instant,
    rng: u64,
}

impl Default for Env {
    fn default() -> Env {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0x9E37_79B9, |d| d.as_nanos() as u64);
        Env { graphics: None, start: Instant::now(), rng: seed | 1 }
    }
}

impl Env {
    fn next_random(&mut self) -> u64 {
        // xorshift64
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    fn window(&mut self) -> Result<&mut Graphics, String> {
        self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())
    }

    /// Runs a built-in; procedures return `Value::Null`.
    pub fn call(&mut self, b: Builtin, args: &[Value]) -> Result<Value, String> {
        Ok(match b {
            Rgb => {
                let c = |v: &Value| int(v).clamp(0, 255);
                Value::Str(format!("#{:02x}{:02x}{:02x}", c(&args[0]), c(&args[1]), c(&args[2])).into())
            }
            Random => {
                let (lo, hi) = (int(&args[0]), int(&args[1]));
                if lo > hi {
                    return Err(format!("random: the minimum ({}) is greater than the maximum ({})", lo, hi));
                }
                let span = (hi as i128 - lo as i128 + 1) as u128;
                Value::Int((lo as i128 + (self.next_random() as u128 % span) as i128) as i64)
            }
            Time => Value::Real(self.start.elapsed().as_secs_f64()),
            Sin => Value::Real(real(&args[0]).to_radians().sin()),
            Cos => Value::Real(real(&args[0]).to_radians().cos()),
            Sqrt => {
                let x = real(&args[0]);
                if x < 0.0 {
                    return Err(format!("sqrt of a negative number ({})", crate::vm::format_real(x)));
                }
                Value::Real(x.sqrt())
            }
            Round => {
                let r = real(&args[0]).round();
                if !(-9.2e18..=9.2e18).contains(&r) {
                    return Err("number too large: the result exceeds the integer limits".to_string());
                }
                Value::Int(r as i64)
            }
            Window => {
                let (w, h) = (int(&args[0]), int(&args[1]));
                if w <= 0 || h <= 0 || w > 10_000 || h > 10_000 {
                    return Err(format!("the window size must be between 1 and 10000, here it is {} x {}", w, h));
                }
                self.graphics = None; // close the previous window first
                self.graphics = Some(Graphics::new(w as usize, h as usize, text(&args[2]))?);
                Value::Null
            }
            Open => Value::Bool(self.graphics.as_ref().is_some_and(|g| g.is_open())),
            Show => {
                self.window()?.show();
                Value::Null
            }
            Clear => {
                let c = parse_color(text(&args[0]))?;
                self.window()?.canvas.clear(c);
                Value::Null
            }
            Rect => {
                let c = parse_color(text(&args[4]))?;
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                self.window()?.canvas.rect(x, y, w, h, c);
                Value::Null
            }
            Circle => {
                let c = parse_color(text(&args[3]))?;
                let (x, y, r) = (real(&args[0]), real(&args[1]), real(&args[2]));
                self.window()?.canvas.circle(x, y, r, c);
                Value::Null
            }
            Line => {
                let c = parse_color(text(&args[4]))?;
                let (x1, y1, x2, y2) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                self.window()?.canvas.line(x1, y1, x2, y2, c);
                Value::Null
            }
            Text => {
                let c = parse_color(text(&args[3]))?;
                let (x, y) = (real(&args[0]), real(&args[1]));
                self.window()?.canvas.text(x, y, text(&args[2]), c);
                Value::Null
            }
            Key => Value::Bool(self.window()?.key(text(&args[0]))?),
            MouseX => Value::Int(self.window()?.mouse().0),
            MouseY => Value::Int(self.window()?.mouse().1),
            Click => Value::Bool(self.window()?.click()),
            Message => {
                dialogs::message(text(&args[0]))?;
                Value::Null
            }
            Confirm => Value::Bool(dialogs::confirm(text(&args[0]))?),
            Ask => Value::Str(dialogs::ask(text(&args[0]))?.into()),
            Camera => {
                let s = &mut self.window()?.scene;
                s.cam = Vec3::new(real(&args[0]), real(&args[1]), real(&args[2]));
                s.turn = real(&args[3]);
                s.tilt = real(&args[4]);
                Value::Null
            }
            Rotate => {
                self.window()?.scene.rot = (real(&args[0]), real(&args[1]), real(&args[2]));
                Value::Null
            }
            Cube | Sphere => {
                let c = parse_color(text(&args[4]))?;
                let center = Vec3::new(real(&args[0]), real(&args[1]), real(&args[2]));
                let g = self.window()?;
                if b == Cube {
                    g.scene.cube(&mut g.canvas, center, real(&args[3]), c);
                } else {
                    g.scene.sphere(&mut g.canvas, center, real(&args[3]), c);
                }
                Value::Null
            }
            Box3 => {
                let c = parse_color(text(&args[6]))?;
                let center = Vec3::new(real(&args[0]), real(&args[1]), real(&args[2]));
                let size = Vec3::new(real(&args[3]), real(&args[4]), real(&args[5]));
                let g = self.window()?;
                g.scene.box_(&mut g.canvas, center, size, c);
                Value::Null
            }
            Triangle3 => {
                let c = parse_color(text(&args[9]))?;
                let p = |k: usize| Vec3::new(real(&args[k]), real(&args[k + 1]), real(&args[k + 2]));
                let g = self.window()?;
                g.scene.triangle(&mut g.canvas, p(0), p(3), p(6), c, true);
                Value::Null
            }
        })
    }

    /// At the end of the program an open window keeps showing its last frame
    /// until the user closes it.
    pub fn finish(&mut self) {
        if let Some(g) = self.graphics.as_mut() {
            g.keep_open();
        }
    }
}
