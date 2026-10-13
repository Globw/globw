//! Software 3D: a camera, shapes made of triangles, near-plane clipping,
//! back-face culling, flat shading and a depth buffer, drawn on a `Canvas`.
//!
//! World space: x to the right, y up, z forwards (away from the viewer).

use std::ops::{Add, Mul, Sub};

use crate::canvas::Canvas;

/// Anything closer to the eye than this is cut away.
const NEAR: f64 = 0.1;
/// Half of the 60-degree vertical field of view.
const HALF_FOV_DEG: f64 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }

    fn normalize(self) -> Vec3 {
        let len = self.dot(self).sqrt();
        if len == 0.0 {
            self
        } else {
            self * (1.0 / len)
        }
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, k: f64) -> Vec3 {
        Vec3::new(self.x * k, self.y * k, self.z * k)
    }
}

/// Rotates (a, b) by `deg` degrees: the 2D rotation used for every axis.
fn turn2(a: f64, b: f64, deg: f64) -> (f64, f64) {
    let (s, c) = deg.to_radians().sin_cos();
    (a * c - b * s, a * s + b * c)
}

/// Multiplies each color channel by `k` (0..1).
fn shade(color: u32, k: f64) -> u32 {
    let ch = |shift: u32| ((((color >> shift) & 0xFF) as f64 * k).round().clamp(0.0, 255.0) as u32) << shift;
    ch(16) | ch(8) | ch(0)
}

/// The camera and the rotation applied to the next shapes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scene {
    pub cam: Vec3,
    /// horizontal direction in degrees: 0 looks towards +z, 90 towards +x
    pub turn: f64,
    /// vertical angle in degrees: positive looks up
    pub tilt: f64,
    /// rotation of the next shapes around x, y, z, in degrees
    pub rot: (f64, f64, f64),
}

impl Default for Scene {
    fn default() -> Scene {
        Scene { cam: Vec3::new(0.0, 0.0, 0.0), turn: 0.0, tilt: 0.0, rot: (0.0, 0.0, 0.0) }
    }
}

impl Scene {
    /// World space → camera space (x right, y up, z forwards from the eye).
    fn to_camera(&self, p: Vec3) -> Vec3 {
        let d = p - self.cam;
        // undo the turn: a point straight ahead of the turned eye lands on +z
        let (x, z) = turn2(d.x, d.z, self.turn);
        // undo the tilt the same way in the y/z plane
        let (z, y) = turn2(z, d.y, -self.tilt);
        Vec3::new(x, y, z)
    }

    fn screen(c: Vec3, w: usize, h: usize) -> (f64, f64, f64) {
        let f = (h as f64 / 2.0) / HALF_FOV_DEG.to_radians().tan();
        (w as f64 / 2.0 + c.x * f / c.z, h as f64 / 2.0 - c.y * f / c.z, c.z)
    }

    /// Screen position and distance of a world point, or `None` if it is behind the eye.
    pub fn project(&self, p: Vec3, w: usize, h: usize) -> Option<(f64, f64, f64)> {
        let c = self.to_camera(p);
        (c.z > NEAR).then(|| Scene::screen(c, w, h))
    }

    /// Applies `rot` (x, then y, then z) to an offset from a shape's center.
    fn rotate(&self, v: Vec3) -> Vec3 {
        let (ax, ay, az) = self.rot;
        let (y, z) = turn2(v.y, v.z, ax);
        let (z, x) = turn2(z, v.x, ay);
        let (x, y) = turn2(x, y, az);
        Vec3::new(x, y, z)
    }

    /// Draws one triangle. One-sided triangles are skipped when seen from behind.
    pub fn triangle(&self, canvas: &mut Canvas, a: Vec3, b: Vec3, c: Vec3, color: u32, two_sided: bool) {
        let normal = (b - a).cross(c - a).normalize();
        let facing = normal.dot(a - self.cam);
        if !two_sided && facing >= 0.0 {
            return; // back face
        }
        let light = Vec3::new(-0.4, 0.8, -0.5).normalize();
        let lambert = normal.dot(light);
        let k = 0.35 + 0.65 * if two_sided { lambert.abs() } else { lambert.max(0.0) };
        let color = shade(color, k);

        // clip against the near plane (Sutherland–Hodgman with one plane)
        let input = [self.to_camera(a), self.to_camera(b), self.to_camera(c)];
        let mut poly: Vec<Vec3> = Vec::with_capacity(4);
        for i in 0..3 {
            let (p, q) = (input[i], input[(i + 1) % 3]);
            let (p_in, q_in) = (p.z >= NEAR, q.z >= NEAR);
            if p_in {
                poly.push(p);
            }
            if p_in != q_in {
                let t = (NEAR - p.z) / (q.z - p.z);
                poly.push(p + (q - p) * t);
            }
        }
        if poly.len() < 3 {
            return;
        }
        let pts: Vec<(f64, f64, f64)> = poly.iter().map(|&v| Scene::screen(v, canvas.w, canvas.h)).collect();
        for i in 1..pts.len() - 1 {
            raster(canvas, pts[0], pts[i], pts[i + 1], color);
        }
    }

    /// Draws a closed mesh: each face is turned outwards, then drawn one-sided.
    fn mesh(&self, canvas: &mut Canvas, center: Vec3, verts: &[Vec3], tris: &[[usize; 3]], color: u32) {
        let world: Vec<Vec3> = verts.iter().map(|&v| center + self.rotate(v)).collect();
        for t in tris {
            let (mut a, b, mut c) = (world[t[0]], world[t[1]], world[t[2]]);
            let normal = (b - a).cross(c - a);
            let mid = (a + b + c) * (1.0 / 3.0);
            if normal.dot(mid - center) < 0.0 {
                std::mem::swap(&mut a, &mut c);
            }
            self.triangle(canvas, a, b, c, color, false);
        }
    }

    /// A box centered on `center`, with sizes along x, y and z.
    pub fn box_(&self, canvas: &mut Canvas, center: Vec3, size: Vec3, color: u32) {
        let h = size * 0.5;
        let verts: Vec<Vec3> = (0..8)
            .map(|i| {
                let sx = if i & 1 == 0 { -h.x } else { h.x };
                let sy = if i & 2 == 0 { -h.y } else { h.y };
                let sz = if i & 4 == 0 { -h.z } else { h.z };
                Vec3::new(sx, sy, sz)
            })
            .collect();
        // each face as two triangles; orientation is fixed by `mesh`
        const TRIS: [[usize; 3]; 12] = [
            [0, 1, 3], [0, 3, 2], // z-
            [4, 5, 7], [4, 7, 6], // z+
            [0, 1, 5], [0, 5, 4], // y-
            [2, 3, 7], [2, 7, 6], // y+
            [0, 2, 6], [0, 6, 4], // x-
            [1, 3, 7], [1, 7, 5], // x+
        ];
        self.mesh(canvas, center, &verts, &TRIS, color);
    }

    pub fn cube(&self, canvas: &mut Canvas, center: Vec3, size: f64, color: u32) {
        self.box_(canvas, center, Vec3::new(size, size, size), color);
    }

    pub fn sphere(&self, canvas: &mut Canvas, center: Vec3, radius: f64, color: u32) {
        const RINGS: usize = 12;
        const SEGMENTS: usize = 16;
        let mut verts = Vec::with_capacity((RINGS + 1) * SEGMENTS);
        for r in 0..=RINGS {
            let lat = std::f64::consts::PI * r as f64 / RINGS as f64;
            for s in 0..SEGMENTS {
                let lon = 2.0 * std::f64::consts::PI * s as f64 / SEGMENTS as f64;
                verts.push(Vec3::new(lat.sin() * lon.cos(), lat.cos(), lat.sin() * lon.sin()) * radius);
            }
        }
        let mut tris = Vec::with_capacity(RINGS * SEGMENTS * 2);
        for r in 0..RINGS {
            for s in 0..SEGMENTS {
                let (a, b) = (r * SEGMENTS + s, r * SEGMENTS + (s + 1) % SEGMENTS);
                let (c, d) = (a + SEGMENTS, b + SEGMENTS);
                if r > 0 {
                    tris.push([a, b, d]);
                }
                if r < RINGS - 1 {
                    tris.push([a, d, c]);
                }
            }
        }
        self.mesh(canvas, center, &verts, &tris, color);
    }
}

/// Fills a screen-space triangle, testing and writing the depth buffer.
/// Depth is interpolated as 1/z, which is linear on screen.
fn raster(canvas: &mut Canvas, a: (f64, f64, f64), b: (f64, f64, f64), c: (f64, f64, f64), color: u32) {
    let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
    if area.abs() < 1e-12 {
        return;
    }
    let x0 = a.0.min(b.0).min(c.0).floor().max(0.0) as usize;
    let y0 = a.1.min(b.1).min(c.1).floor().max(0.0) as usize;
    let x1 = (a.0.max(b.0).max(c.0).ceil() as i64).min(canvas.w as i64 - 1);
    let y1 = (a.1.max(b.1).max(c.1).ceil() as i64).min(canvas.h as i64 - 1);
    if x1 < 0 || y1 < 0 {
        return;
    }
    let (inv_a, inv_b, inv_c) = (1.0 / a.2, 1.0 / b.2, 1.0 / c.2);
    for py in y0..=y1 as usize {
        let y = py as f64 + 0.5;
        for px in x0..=x1 as usize {
            let x = px as f64 + 0.5;
            let wa = ((b.0 - x) * (c.1 - y) - (b.1 - y) * (c.0 - x)) / area;
            let wb = ((c.0 - x) * (a.1 - y) - (c.1 - y) * (a.0 - x)) / area;
            let wc = 1.0 - wa - wb;
            if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                continue;
            }
            let z = (1.0 / (wa * inv_a + wb * inv_b + wc * inv_c)) as f32;
            let i = py * canvas.w + px;
            if z < canvas.depth[i] {
                canvas.depth[i] = z;
                canvas.pixels[i] = color;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: u32 = 0xFF0000;
    const BLUE: u32 = 0x0000FF;
    const GRAY: u32 = 0x808080;

    fn red(c: u32) -> u32 {
        (c >> 16) & 0xFF
    }

    fn blue(c: u32) -> u32 {
        c & 0xFF
    }

    #[test]
    fn point_in_front_projects_to_center() {
        let s = Scene::default();
        let (x, y, z) = s.project(Vec3::new(0.0, 0.0, 5.0), 200, 100).unwrap();
        assert!((x - 100.0).abs() < 1e-9 && (y - 50.0).abs() < 1e-9 && (z - 5.0).abs() < 1e-9);
        assert!(s.project(Vec3::new(0.0, 0.0, -1.0), 200, 100).is_none());
        let (_, y_up, _) = s.project(Vec3::new(0.0, 1.0, 5.0), 200, 100).unwrap();
        assert!(y_up < 50.0, "y grows upwards in 3D, downwards on screen");
    }

    #[test]
    fn cube_in_front_covers_center() {
        let mut c = Canvas::new(200, 100);
        Scene::default().cube(&mut c, Vec3::new(0.0, 0.0, 5.0), 1.0, RED);
        assert!(red(c.get(100, 50)) > 0 && blue(c.get(100, 50)) == 0);
        assert_eq!(c.get(0, 0), 0);
    }

    #[test]
    fn nearer_shape_wins_whatever_the_order() {
        let s = Scene::default();
        for near_first in [true, false] {
            let mut c = Canvas::new(200, 100);
            let near = |c: &mut Canvas| s.cube(c, Vec3::new(0.0, 0.0, 4.0), 1.0, RED);
            let far = |c: &mut Canvas| s.cube(c, Vec3::new(0.0, 0.0, 8.0), 6.0, BLUE);
            if near_first {
                near(&mut c);
                far(&mut c);
            } else {
                far(&mut c);
                near(&mut c);
            }
            let p = c.get(100, 50);
            assert!(red(p) > 0 && blue(p) == 0, "near_first={} pixel={:06x}", near_first, p);
            assert!(blue(c.get(100, 10)) > 0, "the far cube is visible around the near one");
        }
    }

    #[test]
    fn behind_camera_draws_nothing() {
        let mut c = Canvas::new(200, 100);
        Scene::default().cube(&mut c, Vec3::new(0.0, 0.0, -5.0), 1.0, RED);
        assert!(c.pixels.iter().all(|&p| p == 0));
    }

    #[test]
    fn turning_the_camera_moves_the_view() {
        let s = Scene { turn: 180.0, ..Scene::default() };
        let mut c = Canvas::new(200, 100);
        s.cube(&mut c, Vec3::new(0.0, 0.0, 5.0), 1.0, RED);
        assert!(c.pixels.iter().all(|&p| p == 0));
        s.cube(&mut c, Vec3::new(0.0, 0.0, -5.0), 1.0, RED);
        assert!(red(c.get(100, 50)) > 0);
        let up = Scene { tilt: 90.0, ..Scene::default() };
        let (x, y, _) = up.project(Vec3::new(0.0, 5.0, 0.0), 200, 100).unwrap();
        assert!((x - 100.0).abs() < 1e-6 && (y - 50.0).abs() < 1e-6);
    }

    #[test]
    fn crossing_the_near_plane_is_clipped() {
        let mut c = Canvas::new(200, 100);
        Scene::default().box_(&mut c, Vec3::new(0.0, -1.0, 0.0), Vec3::new(20.0, 0.2, 20.0), GRAY);
        assert_ne!(c.get(100, 99), 0, "the floor is visible below the horizon");
        assert_eq!(c.get(100, 0), 0, "and nothing is drawn above it");
    }

    #[test]
    fn shading_differs_between_faces() {
        let mut c = Canvas::new(200, 200);
        let s = Scene { rot: (30.0, 45.0, 0.0), ..Scene::default() };
        s.cube(&mut c, Vec3::new(0.0, 0.0, 5.0), 2.5, RED);
        let mut shades: Vec<u32> = c.pixels.iter().copied().filter(|&p| p != 0).collect();
        shades.sort();
        shades.dedup();
        assert!(shades.len() >= 2, "{:?}", shades);
    }

    #[test]
    fn sphere_is_round_and_two_sided_triangles_show_from_behind() {
        let mut c = Canvas::new(200, 200);
        let s = Scene::default();
        s.sphere(&mut c, Vec3::new(0.0, 0.0, 5.0), 1.0, BLUE);
        assert!(blue(c.get(100, 100)) > 0);
        assert_eq!(c.get(100, 10), 0);
        let mut t = Canvas::new(200, 200);
        let (a, b, d) = (Vec3::new(-1.0, -1.0, 3.0), Vec3::new(1.0, -1.0, 3.0), Vec3::new(0.0, 1.0, 3.0));
        s.triangle(&mut t, a, b, d, RED, true);
        s.triangle(&mut t, d, b, a, RED, true);
        assert!(red(t.get(100, 100)) > 0);
    }

    #[test]
    fn clear_resets_depth() {
        let mut c = Canvas::new(50, 50);
        Scene::default().cube(&mut c, Vec3::new(0.0, 0.0, 3.0), 1.0, RED);
        assert!(c.depth.iter().any(|d| d.is_finite()));
        c.clear(0);
        assert!(c.depth.iter().all(|d| d.is_infinite()));
    }
}
