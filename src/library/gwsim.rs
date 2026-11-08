//! The "gwsim" module's helpers: a small 2D physics world of circular bodies.
//! The builtins live in `call()`; this holds the world and the step integrator.
use std::cell::RefCell;

#[derive(Clone, Copy)]
pub(super) struct Body {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub r: f64,
    pub m: f64,
    pub fixed: bool,
}

pub(super) struct SimWorld {
    pub bodies: Vec<Body>,
    pub gx: f64,
    pub gy: f64,
    pub bx0: f64,
    pub by0: f64,
    pub bx1: f64,
    pub by1: f64,
    pub rest: f64,
}

impl Default for SimWorld {
    fn default() -> Self {
        SimWorld { bodies: Vec::new(), gx: 0.0, gy: 0.0, bx0: 0.0, by0: 0.0, bx1: 0.0, by1: 0.0, rest: 0.9 }
    }
}

thread_local! {
    pub(super) static SIM: RefCell<SimWorld> = RefCell::new(SimWorld::default());
}

pub(super) fn sim_body(h: i64) -> Result<usize, String> {
    SIM.with(|s| {
        let i = usize::try_from(h).map_err(|_| "gwsim: invalid handle".to_string())?;
        if i < s.borrow().bodies.len() {
            Ok(i)
        } else {
            Err("gwsim: invalid handle".to_string())
        }
    })
}

pub(super) fn sim_step(dt: f64) {
    SIM.with(|s| {
        let w = &mut *s.borrow_mut();
        let has_bounds = w.bx1 > w.bx0 && w.by1 > w.by0;
        for b in w.bodies.iter_mut() {
            if b.fixed {
                continue;
            }
            b.vx += w.gx * dt;
            b.vy += w.gy * dt;
            b.x += b.vx * dt;
            b.y += b.vy * dt;
            if has_bounds {
                if b.x - b.r < w.bx0 {
                    b.x = w.bx0 + b.r;
                    b.vx = -b.vx * w.rest;
                }
                if b.x + b.r > w.bx1 {
                    b.x = w.bx1 - b.r;
                    b.vx = -b.vx * w.rest;
                }
                if b.y - b.r < w.by0 {
                    b.y = w.by0 + b.r;
                    b.vy = -b.vy * w.rest;
                }
                if b.y + b.r > w.by1 {
                    b.y = w.by1 - b.r;
                    b.vy = -b.vy * w.rest;
                }
            }
        }
        // pairwise circle-circle collisions (impulse based)
        let n = w.bodies.len();
        for i in 0..n {
            for j in (i + 1)..n {
                let (mut a, mut c) = (w.bodies[i], w.bodies[j]);
                if a.fixed && c.fixed {
                    continue;
                }
                let (dx, dy) = (c.x - a.x, c.y - a.y);
                let dist = dx.hypot(dy);
                let sum = a.r + c.r;
                if dist >= sum || dist <= 1e-9 {
                    continue;
                }
                let (nx, ny) = (dx / dist, dy / dist);
                let invi = if a.fixed { 0.0 } else { 1.0 / a.m };
                let invj = if c.fixed { 0.0 } else { 1.0 / c.m };
                let invsum = invi + invj;
                if invsum == 0.0 {
                    continue;
                }
                let overlap = sum - dist;
                a.x -= nx * overlap * (invi / invsum);
                a.y -= ny * overlap * (invi / invsum);
                c.x += nx * overlap * (invj / invsum);
                c.y += ny * overlap * (invj / invsum);
                let vn = (c.vx - a.vx) * nx + (c.vy - a.vy) * ny;
                if vn < 0.0 {
                    let jimp = -(1.0 + w.rest) * vn / invsum;
                    a.vx -= jimp * invi * nx;
                    a.vy -= jimp * invi * ny;
                    c.vx += jimp * invj * nx;
                    c.vy += jimp * invj * ny;
                }
                w.bodies[i] = a;
                w.bodies[j] = c;
            }
        }
    });
}
