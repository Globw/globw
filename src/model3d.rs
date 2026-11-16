//! 3D models for the built-in 3D: loaded from glTF (.glb, .gltf), Wavefront
//! (.obj, with its .mtl colors and textures) or STL files, or built in the
//! program one triangle at a time, then drawn as often as wanted with
//! `model.draw`.
//!
//! Every model is kept as a list of triangles in Globw space (x right, y up,
//! z forwards). The files use right-handed spaces, so z is mirrored on loading
//! (and STL, which stands on z, is turned to stand on y); the corners are then
//! put in the order whose normal points out of the front face. Each triangle has
//! one color, like every 3D shape in Globw: with a texture, it is the texture's
//! color at the triangle's center.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::render3d::Vec3;

/// The color of a model whose file says nothing about colors.
pub const DEFAULT_COLOR: u32 = 0xC8C8C8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelTri {
    pub p: [Vec3; 3],
    pub color: u32,
    /// drawn from both sides (glTF `doubleSided`, and triangles added by the program)
    pub two_sided: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub tris: Vec<ModelTri>,
    /// one color for every triangle instead of the file's (`model.color`)
    pub tint: Option<u32>,
}

impl Model {
    /// The smallest and largest x, y and z of the model's corners.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut hi = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for t in &self.tris {
            for p in &t.p {
                lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            }
        }
        if self.tris.is_empty() {
            (Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0))
        } else {
            (lo, hi)
        }
    }

    /// Moves every corner so that `p` becomes the origin.
    pub fn shift(&mut self, p: Vec3) {
        for t in &mut self.tris {
            for q in &mut t.p {
                *q = *q - p;
            }
        }
    }
}

/// Loads a model file, choosing the format from its extension.
pub fn load(path: &str) -> Result<Model, String> {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let bytes = std::fs::read(path).map_err(|e| format!("model.load: cannot read \"{}\": {}", path, e))?;
    let base = Path::new(path).parent().map(Path::to_path_buf).unwrap_or_default();
    let m = match ext.as_str() {
        "glb" | "gltf" => gltf(&bytes, &base),
        "obj" => obj(&bytes, &base),
        "stl" => stl(&bytes),
        _ => Err("the file must end in .glb, .gltf, .obj or .stl".to_string()),
    }
    .map_err(|e| format!("model.load: \"{}\": {}", path, e))?;
    if m.tris.is_empty() {
        return Err(format!("model.load: \"{}\" has no triangles", path));
    }
    Ok(m)
}

// ------------------------------------------------------------------ colors

fn to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// A linear RGB color (0..1) as `0xRRGGBB` for the screen.
fn pack_linear(c: [f64; 3]) -> u32 {
    let ch = |v: f64| (to_srgb(v) * 255.0).round() as u32;
    (ch(c[0]) << 16) | (ch(c[1]) << 8) | ch(c[2])
}

/// A decoded picture, as linear RGB.
struct Picture {
    w: usize,
    h: usize,
    px: Vec<[f32; 3]>,
}

impl Picture {
    /// The color at texture coordinates (u, v), repeating outside 0..1.
    fn at(&self, u: f64, v: f64) -> [f64; 3] {
        let fx = u - u.floor();
        let fy = v - v.floor();
        let x = ((fx * self.w as f64) as usize).min(self.w - 1);
        let y = ((fy * self.h as f64) as usize).min(self.h - 1);
        let p = self.px[y * self.w + x];
        [p[0] as f64, p[1] as f64, p[2] as f64]
    }
}

/// Decodes a PNG or JPEG picture (desktop builds only; `None` otherwise or if it fails).
fn decode_picture(bytes: &[u8]) -> Option<Picture> {
    #[cfg(feature = "native")]
    {
        let lin = |b: u8| to_linear(b as f64 / 255.0) as f32;
        if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
            dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
            let mut reader = dec.read_info().ok()?;
            let mut buf = vec![0; reader.output_buffer_size()?];
            let info = reader.next_frame(&mut buf).ok()?;
            let (w, h) = (info.width as usize, info.height as usize);
            let n = match info.color_type {
                png::ColorType::Grayscale => 1,
                png::ColorType::GrayscaleAlpha => 2,
                png::ColorType::Rgb => 3,
                png::ColorType::Rgba => 4,
                png::ColorType::Indexed => return None,
            };
            let px = buf[..w * h * n]
                .chunks_exact(n)
                .map(|c| if n < 3 { [lin(c[0]); 3] } else { [lin(c[0]), lin(c[1]), lin(c[2])] })
                .collect();
            return (w > 0 && h > 0).then_some(Picture { w, h, px });
        }
        if bytes.starts_with(&[0xFF, 0xD8]) {
            let mut dec = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
            let data = dec.decode().ok()?;
            let info = dec.info()?;
            let (w, h) = (info.width as usize, info.height as usize);
            let px = match info.pixel_format {
                jpeg_decoder::PixelFormat::L8 => data.iter().map(|&g| [lin(g); 3]).collect(),
                jpeg_decoder::PixelFormat::RGB24 => data.chunks_exact(3).map(|c| [lin(c[0]), lin(c[1]), lin(c[2])]).collect(),
                _ => return None,
            };
            return (w > 0 && h > 0).then_some(Picture { w, h, px });
        }
        None
    }
    #[cfg(not(feature = "native"))]
    {
        let _ = bytes;
        None
    }
}

// ------------------------------------------------------------------ building

/// Collects triangles in the file's right-handed space and turns them into Globw's.
struct Builder {
    tris: Vec<ModelTri>,
    /// STL stands on z: turn it to stand on y
    z_up: bool,
}

impl Builder {
    fn convert(&self, p: [f64; 3]) -> Vec3 {
        if self.z_up {
            // (x, y, z) -> (x, z, y): a mirror, like the glTF one below
            Vec3::new(p[0], p[2], p[1])
        } else {
            Vec3::new(p[0], p[1], -p[2])
        }
    }

    /// Adds a triangle whose corners go counter-clockwise seen from its front
    /// in the file's space. The conversion mirrors, so b and c are swapped.
    fn add(&mut self, a: [f64; 3], b: [f64; 3], c: [f64; 3], color: u32, two_sided: bool) {
        let p = [self.convert(a), self.convert(c), self.convert(b)];
        if p.iter().all(|v| v.x.is_finite() && v.y.is_finite() && v.z.is_finite()) {
            self.tris.push(ModelTri { p, color, two_sided });
        }
    }

    fn finish(self) -> Model {
        Model { tris: self.tris, tint: None }
    }
}

// ------------------------------------------------------------------ glTF

type Mat4 = [f64; 16]; // column-major, as in glTF

const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut r = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            r[col * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum();
        }
    }
    r
}

fn mat_apply(m: &Mat4, p: [f64; 3]) -> [f64; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

fn mat_det3(m: &Mat4) -> f64 {
    m[0] * (m[5] * m[10] - m[9] * m[6]) - m[4] * (m[1] * m[10] - m[9] * m[2]) + m[8] * (m[1] * m[6] - m[5] * m[2])
}

/// A node's own transform: `matrix`, or translation × rotation × scale.
fn node_matrix(node: &serde_json::Value) -> Mat4 {
    let nums = |key: &str| -> Option<Vec<f64>> { node.get(key)?.as_array()?.iter().map(|v| v.as_f64()).collect() };
    if let Some(m) = nums("matrix").filter(|m| m.len() == 16) {
        let mut r = [0.0; 16];
        r.copy_from_slice(&m);
        return r;
    }
    let t = nums("translation").filter(|v| v.len() == 3).unwrap_or(vec![0.0, 0.0, 0.0]);
    let q = nums("rotation").filter(|v| v.len() == 4).unwrap_or(vec![0.0, 0.0, 0.0, 1.0]);
    let s = nums("scale").filter(|v| v.len() == 3).unwrap_or(vec![1.0, 1.0, 1.0]);
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let r = [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
    ];
    [
        r[0] * s[0], r[1] * s[0], r[2] * s[0], 0.0,
        r[3] * s[1], r[4] * s[1], r[5] * s[1], 0.0,
        r[6] * s[2], r[7] * s[2], r[8] * s[2], 0.0,
        t[0], t[1], t[2], 1.0,
    ]
}

struct Gltf<'a> {
    json: serde_json::Value,
    buffers: Vec<Vec<u8>>,
    base: &'a Path,
    pictures: HashMap<usize, Option<Picture>>,
}

fn data_uri(uri: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let rest = uri.strip_prefix("data:")?;
    let (_, data) = rest.split_once(";base64,")?;
    base64::engine::general_purpose::STANDARD.decode(data).ok()
}

impl<'a> Gltf<'a> {
    fn load_uri(&self, uri: &str) -> Result<Vec<u8>, String> {
        if uri.starts_with("data:") {
            return data_uri(uri).ok_or_else(|| "an embedded data: address cannot be read".to_string());
        }
        let p: PathBuf = self.base.join(percent_decode(uri));
        std::fs::read(&p).map_err(|e| format!("cannot read \"{}\": {}", p.display(), e))
    }

    fn get(&self, kind: &str, i: usize) -> Result<&serde_json::Value, String> {
        self.json.get(kind).and_then(|a| a.get(i)).ok_or_else(|| format!("{} number {} is missing", kind, i))
    }

    /// The bytes of a buffer view.
    fn view(&self, i: usize) -> Result<(&[u8], usize), String> {
        let v = self.get("bufferViews", i)?;
        let buf = v["buffer"].as_u64().ok_or("a buffer view has no buffer")? as usize;
        let data = self.buffers.get(buf).ok_or("a buffer view points to a missing buffer")?;
        let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        let len = v["byteLength"].as_u64().ok_or("a buffer view has no length")? as usize;
        let stride = v["byteStride"].as_u64().unwrap_or(0) as usize;
        let bytes = data.get(off..off + len).ok_or("a buffer view goes past the end of its buffer")?;
        Ok((bytes, stride))
    }

    /// An accessor's values as numbers, `n` per element (normalized integers scaled to 0..1).
    fn accessor(&self, i: usize) -> Result<(Vec<f64>, usize), String> {
        let a = self.get("accessors", i)?;
        let count = a["count"].as_u64().ok_or("an accessor has no count")? as usize;
        let n = match a["type"].as_str().unwrap_or("") {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            other => return Err(format!("accessors of type {} are not supported", other)),
        };
        let ct = a["componentType"].as_u64().unwrap_or(0);
        let size = match ct {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err(format!("component type {} is not supported", ct)),
        };
        let norm = a["normalized"].as_bool().unwrap_or(false);
        let mut out = vec![0.0; count * n];
        let Some(view) = a["bufferView"].as_u64() else {
            return Ok((out, n)); // all zeros (sparse accessors are not read)
        };
        let (bytes, stride) = self.view(view as usize)?;
        let off = a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let stride = if stride == 0 { size * n } else { stride };
        for e in 0..count {
            for k in 0..n {
                let at = off + e * stride + k * size;
                let b = bytes.get(at..at + size).ok_or("an accessor goes past the end of its buffer view")?;
                out[e * n + k] = match ct {
                    5120 => {
                        let v = b[0] as i8 as f64;
                        if norm { (v / 127.0).max(-1.0) } else { v }
                    }
                    5121 => {
                        let v = b[0] as f64;
                        if norm { v / 255.0 } else { v }
                    }
                    5122 => {
                        let v = i16::from_le_bytes([b[0], b[1]]) as f64;
                        if norm { (v / 32767.0).max(-1.0) } else { v }
                    }
                    5123 => {
                        let v = u16::from_le_bytes([b[0], b[1]]) as f64;
                        if norm { v / 65535.0 } else { v }
                    }
                    5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
                    _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
                };
            }
        }
        Ok((out, n))
    }

    /// The picture of texture `i`, decoded once.
    fn picture(&mut self, i: usize) -> Option<&Picture> {
        if !self.pictures.contains_key(&i) {
            let pic = (|| {
                let tex = self.get("textures", i).ok()?;
                let src = tex["source"].as_u64()? as usize;
                let img = self.get("images", src).ok()?;
                let bytes = if let Some(v) = img["bufferView"].as_u64() {
                    self.view(v as usize).ok()?.0.to_vec()
                } else {
                    self.load_uri(img["uri"].as_str()?).ok()?
                };
                decode_picture(&bytes)
            })();
            self.pictures.insert(i, pic);
        }
        self.pictures.get(&i).and_then(|p| p.as_ref())
    }

    fn mesh(&mut self, out: &mut Builder, mesh: usize, m: &Mat4) -> Result<(), String> {
        let prims = self.get("meshes", mesh)?["primitives"].as_array().cloned().unwrap_or_default();
        let mirrored = mat_det3(m) < 0.0;
        for prim in prims {
            let mode = prim["mode"].as_u64().unwrap_or(4);
            if !(4..=6).contains(&mode) {
                continue; // points and lines
            }
            let attrs = &prim["attributes"];
            let Some(pos_acc) = attrs["POSITION"].as_u64() else { continue };
            let (pos, _) = self.accessor(pos_acc as usize)?;
            let count = pos.len() / 3;
            let corners: Vec<[f64; 3]> = (0..count).map(|i| mat_apply(m, [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]])).collect();
            let vcolor: Option<(Vec<f64>, usize)> = match attrs["COLOR_0"].as_u64() {
                Some(a) => Some(self.accessor(a as usize)?),
                None => None,
            };
            // the material: base color, texture, sides
            let mat = prim["material"].as_u64().and_then(|i| self.get("materials", i as usize).ok().cloned());
            let pbr = mat.as_ref().map(|m| m["pbrMetallicRoughness"].clone()).unwrap_or_default();
            let factor: Vec<f64> = pbr["baseColorFactor"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
                .filter(|v: &Vec<f64>| v.len() >= 3)
                .unwrap_or(vec![1.0, 1.0, 1.0]);
            let two_sided = mat.as_ref().and_then(|m| m["doubleSided"].as_bool()).unwrap_or(false);
            let tex = pbr["baseColorTexture"]["index"].as_u64().map(|i| i as usize);
            let uv_set = pbr["baseColorTexture"]["texCoord"].as_u64().unwrap_or(0);
            let uvs: Option<Vec<f64>> = match (tex, attrs[format!("TEXCOORD_{}", uv_set)].as_u64()) {
                (Some(_), Some(a)) => Some(self.accessor(a as usize)?.0),
                _ => None,
            };
            let indices: Vec<usize> = match prim["indices"].as_u64() {
                Some(a) => self.accessor(a as usize)?.0.into_iter().map(|v| v as usize).collect(),
                None => (0..count).collect(),
            };
            let tris: Vec<[usize; 3]> = match mode {
                4 => indices.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
                5 => (2..indices.len())
                    .map(|i| if i % 2 == 0 { [indices[i - 2], indices[i - 1], indices[i]] } else { [indices[i - 1], indices[i - 2], indices[i]] })
                    .collect(),
                _ => (2..indices.len()).map(|i| [indices[0], indices[i - 1], indices[i]]).collect(),
            };
            for t in tris {
                if t.iter().any(|&i| i >= count) {
                    return Err("a triangle points to a corner that does not exist".to_string());
                }
                let mut c = [factor[0], factor[1], factor[2]];
                if let Some((vc, n)) = &vcolor {
                    for k in 0..3 {
                        c[k] *= (vc[t[0] * n + k] + vc[t[1] * n + k] + vc[t[2] * n + k]) / 3.0;
                    }
                }
                if let (Some(ti), Some(uv)) = (tex, &uvs) {
                    let u = (uv[t[0] * 2] + uv[t[1] * 2] + uv[t[2] * 2]) / 3.0;
                    let v = (uv[t[0] * 2 + 1] + uv[t[1] * 2 + 1] + uv[t[2] * 2 + 1]) / 3.0;
                    if let Some(pic) = self.picture(ti) {
                        let p = pic.at(u, v);
                        for k in 0..3 {
                            c[k] *= p[k];
                        }
                    }
                }
                let (a, b, cc) = (corners[t[0]], corners[t[1]], corners[t[2]]);
                if mirrored {
                    out.add(a, cc, b, pack_linear(c), two_sided);
                } else {
                    out.add(a, b, cc, pack_linear(c), two_sided);
                }
            }
        }
        Ok(())
    }

    fn node(&mut self, out: &mut Builder, i: usize, parent: &Mat4, depth: usize) -> Result<(), String> {
        if depth > 64 {
            return Err("the nodes are nested too deeply (or loop)".to_string());
        }
        let node = self.get("nodes", i)?.clone();
        let m = mat_mul(parent, &node_matrix(&node));
        if let Some(mesh) = node["mesh"].as_u64() {
            self.mesh(out, mesh as usize, &m)?;
        }
        for child in node["children"].as_array().cloned().unwrap_or_default() {
            if let Some(c) = child.as_u64() {
                self.node(out, c as usize, &m, depth + 1)?;
            }
        }
        Ok(())
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn gltf(bytes: &[u8], base: &Path) -> Result<Model, String> {
    let (json_bytes, bin): (&[u8], Option<&[u8]>) = if bytes.starts_with(b"glTF") {
        // binary glTF: a header, then chunks of (length, type, data)
        let mut json = None;
        let mut bin = None;
        let mut at = 12;
        while at + 8 <= bytes.len() {
            let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            let kind = &bytes[at + 4..at + 8];
            let data = bytes.get(at + 8..at + 8 + len).ok_or("the file is cut short")?;
            match kind {
                b"JSON" => json = Some(data),
                b"BIN\0" => bin = Some(data),
                _ => {}
            }
            at += 8 + len;
        }
        (json.ok_or("the file has no JSON part")?, bin)
    } else {
        (bytes, None)
    };
    let json: serde_json::Value = serde_json::from_slice(json_bytes).map_err(|e| format!("the glTF description is not valid JSON: {}", e))?;
    let mut g = Gltf { json, buffers: Vec::new(), base, pictures: HashMap::new() };
    let bufs = g.json["buffers"].as_array().cloned().unwrap_or_default();
    for (i, b) in bufs.iter().enumerate() {
        let data = match b["uri"].as_str() {
            Some(uri) => g.load_uri(uri)?,
            None if i == 0 => bin.ok_or("buffer 0 has no data")?.to_vec(),
            None => return Err(format!("buffer {} has no data", i)),
        };
        g.buffers.push(data);
    }
    let mut out = Builder { tris: Vec::new(), z_up: false };
    let roots: Vec<usize> = {
        let scene = g.json["scene"].as_u64().unwrap_or(0) as usize;
        match g.json["scenes"].get(scene).and_then(|s| s["nodes"].as_array()) {
            Some(nodes) => nodes.iter().filter_map(|n| n.as_u64().map(|v| v as usize)).collect(),
            None => {
                // no scene: every node that is nobody's child
                let nodes = g.json["nodes"].as_array().cloned().unwrap_or_default();
                let mut child = vec![false; nodes.len()];
                for n in &nodes {
                    for c in n["children"].as_array().cloned().unwrap_or_default() {
                        if let Some(c) = c.as_u64().filter(|&c| (c as usize) < nodes.len()) {
                            child[c as usize] = true;
                        }
                    }
                }
                (0..nodes.len()).filter(|&i| !child[i]).collect()
            }
        }
    };
    if roots.is_empty() {
        // only meshes, no nodes
        let n = g.json["meshes"].as_array().map_or(0, Vec::len);
        for m in 0..n {
            g.mesh(&mut out, m, &IDENTITY)?;
        }
    }
    for r in roots {
        g.node(&mut out, r, &IDENTITY, 0)?;
    }
    Ok(out.finish())
}

// ------------------------------------------------------------------ OBJ

fn obj(bytes: &[u8], base: &Path) -> Result<Model, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut uvs: Vec<[f64; 2]> = Vec::new();
    // material name -> (linear color, texture)
    let mut mats: HashMap<String, ([f64; 3], Option<usize>)> = HashMap::new();
    let mut pictures: Vec<Option<Picture>> = Vec::new();
    let mut current: ([f64; 3], Option<usize>) = ([0.0; 3], None);
    let mut has_material = false;
    let mut out = Builder { tris: Vec::new(), z_up: false };
    let nums = |parts: &[&str], n: usize| -> Option<Vec<f64>> {
        let v: Vec<f64> = parts.iter().take(n).filter_map(|s| s.parse().ok()).collect();
        (v.len() == n).then_some(v)
    };
    for (lineno, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let Some(cmd) = parts.next() else { continue };
        let rest: Vec<&str> = parts.collect();
        match cmd {
            "v" => {
                let v = nums(&rest, 3).ok_or_else(|| format!("line {}: a v line needs x y z", lineno + 1))?;
                verts.push([v[0], v[1], v[2]]);
            }
            "vt" => {
                let v = nums(&rest, 2).unwrap_or(vec![0.0, 0.0]);
                uvs.push([v[0], v[1]]);
            }
            "mtllib" => {
                let name = rest.join(" ");
                if let Ok(mtl) = std::fs::read(base.join(&name)) {
                    read_mtl(&String::from_utf8_lossy(&mtl), base, &mut mats, &mut pictures);
                }
            }
            "usemtl" => {
                current = mats.get(&rest.join(" ")).copied().unwrap_or(([0.8, 0.8, 0.8], None));
                has_material = true;
            }
            "f" => {
                // corners as v, v/vt, v//vn or v/vt/vn, counted from 1 (or from the end if negative)
                let mut corner: Vec<(usize, Option<usize>)> = Vec::new();
                for c in &rest {
                    let mut it = c.split('/');
                    let idx = |s: Option<&str>, len: usize| -> Option<usize> {
                        let i: i64 = s?.parse().ok()?;
                        let k = if i < 0 { len as i64 + i } else { i - 1 };
                        (k >= 0 && (k as usize) < len).then_some(k as usize)
                    };
                    let v = idx(it.next(), verts.len()).ok_or_else(|| format!("line {}: a face points to a corner that does not exist", lineno + 1))?;
                    let t = idx(it.next(), uvs.len());
                    corner.push((v, t));
                }
                for i in 2..corner.len() {
                    let tri = [corner[0], corner[i - 1], corner[i]];
                    let mut c = if has_material { current.0 } else { [to_linear(0.784); 3] };
                    if let Some(pi) = current.1 {
                        if let (Some(Some(pic)), Some(a), Some(b), Some(d)) = (pictures.get(pi), tri[0].1, tri[1].1, tri[2].1) {
                            let u = (uvs[a][0] + uvs[b][0] + uvs[d][0]) / 3.0;
                            // OBJ textures count v from the bottom
                            let v = 1.0 - (uvs[a][1] + uvs[b][1] + uvs[d][1]) / 3.0;
                            let p = pic.at(u, v);
                            c = [c[0] * p[0], c[1] * p[1], c[2] * p[2]];
                        }
                    }
                    out.add(verts[tri[0].0], verts[tri[1].0], verts[tri[2].0], pack_linear(c), false);
                }
            }
            _ => {}
        }
    }
    Ok(out.finish())
}

fn read_mtl(text: &str, base: &Path, mats: &mut HashMap<String, ([f64; 3], Option<usize>)>, pictures: &mut Vec<Option<Picture>>) {
    let mut name = String::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let Some(cmd) = parts.next() else { continue };
        let rest: Vec<&str> = parts.collect();
        match cmd {
            "newmtl" => {
                name = rest.join(" ");
                mats.insert(name.clone(), ([0.8, 0.8, 0.8], None));
            }
            "Kd" => {
                let v: Vec<f64> = rest.iter().take(3).filter_map(|s| s.parse().ok()).collect();
                if let (3, Some(m)) = (v.len(), mats.get_mut(&name)) {
                    m.0 = [v[0], v[1], v[2]];
                }
            }
            "map_Kd" => {
                // the file name is the last word (options come first)
                if let Some(file) = rest.last() {
                    let pic = std::fs::read(base.join(file)).ok().and_then(|b| decode_picture(&b));
                    pictures.push(pic);
                    let i = pictures.len() - 1;
                    if let Some(m) = mats.get_mut(&name) {
                        // with a texture, Kd tints it (often left at 1 1 1)
                        m.1 = Some(i);
                    }
                }
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------------ STL

fn stl(bytes: &[u8]) -> Result<Model, String> {
    let mut out = Builder { tris: Vec::new(), z_up: true };
    let color = DEFAULT_COLOR;
    // binary: 80 bytes, a count, then 50 bytes a triangle
    if bytes.len() >= 84 {
        let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        if 84 + n * 50 == bytes.len() {
            for t in 0..n {
                let at = 84 + t * 50 + 12; // skip the normal
                let f = |k: usize| f32::from_le_bytes(bytes[at + k * 4..at + k * 4 + 4].try_into().unwrap()) as f64;
                out.add([f(0), f(1), f(2)], [f(3), f(4), f(5)], [f(6), f(7), f(8)], color, false);
            }
            return Ok(out.finish());
        }
    }
    // text: "vertex x y z" lines, three a facet
    let text = String::from_utf8_lossy(bytes);
    let mut corners: Vec<[f64; 3]> = Vec::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() == Some("vertex") {
            let v: Vec<f64> = parts.take(3).filter_map(|s| s.parse().ok()).collect();
            if v.len() == 3 {
                corners.push([v[0], v[1], v[2]]);
            }
        } else if line.trim_start().starts_with("endfacet") {
            if corners.len() >= 3 {
                out.add(corners[0], corners[1], corners[2], color, false);
            }
            corners.clear();
        }
    }
    Ok(out.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-triangle glTF in the XY plane, facing +z (towards a glTF viewer).
    fn tiny_gltf(color: &str, double: bool) -> String {
        use base64::Engine;
        let mut bin = Vec::new();
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bin);
        format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0,"translation":[0,0,2]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0}}]}}],
            "materials":[{{"doubleSided":{double},"pbrMetallicRoughness":{{"baseColorFactor":{color}}}}}],
            "buffers":[{{"byteLength":36,"uri":"data:application/octet-stream;base64,{b64}"}}],
            "bufferViews":[{{"buffer":0,"byteLength":36}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}}]}}"#
        )
    }

    #[test]
    fn gltf_reads_nodes_colors_and_mirrors_z() {
        let m = gltf(tiny_gltf("[1,0,0,1]", false).as_bytes(), Path::new(".")).unwrap();
        assert_eq!(m.tris.len(), 1);
        let t = m.tris[0];
        assert_eq!(t.color, 0xFF0000);
        assert!(!t.two_sided);
        // translation (0, 0, 2) in glTF lands at z = -2 in Globw
        assert!(t.p.iter().all(|p| (p.z + 2.0).abs() < 1e-9));
        // the front (glTF +z, towards its viewer) now faces Globw's -z, towards a
        // default camera: the normal by the corner order points to -z
        let n = (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]);
        assert!(n.z < 0.0);
    }

    #[test]
    fn gltf_double_sided_and_linear_factor() {
        let m = gltf(tiny_gltf("[0.21404,0.21404,0.21404,1]", true).as_bytes(), Path::new(".")).unwrap();
        assert!(m.tris[0].two_sided);
        // linear 0.214 is about sRGB 0.5
        let c = m.tris[0].color;
        assert!(((c >> 16) & 0xFF).abs_diff(128) <= 1, "{:06x}", c);
    }

    #[test]
    fn obj_faces_fan_and_negative_indices() {
        let src = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\nf -4 -3 -2\n";
        let m = obj(src.as_bytes(), Path::new(".")).unwrap();
        assert_eq!(m.tris.len(), 3);
        assert_eq!(m.tris[0].color, DEFAULT_COLOR);
        let (lo, hi) = m.bounds();
        assert_eq!((lo.x, hi.x, lo.y, hi.y), (0.0, 1.0, 0.0, 1.0));
    }

    #[test]
    fn stl_ascii_stands_on_y() {
        let src = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 0 3\nendloop\nendfacet\nendsolid t\n";
        let m = stl(src.as_bytes()).unwrap();
        let (_, hi) = m.bounds();
        assert_eq!(hi.y, 3.0); // STL's z (up) is Globw's y
    }

    #[test]
    fn stl_binary() {
        let mut b = vec![0u8; 80];
        b.extend_from_slice(&1u32.to_le_bytes());
        for v in [0.0f32, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&[0, 0]);
        let m = stl(&b).unwrap();
        assert_eq!(m.tris.len(), 1);
    }

    #[test]
    fn shift_moves_the_origin() {
        let mut m = obj(b"v 2 2 2\nv 4 2 2\nv 2 4 2\nf 1 2 3\n", Path::new(".")).unwrap();
        let (lo, hi) = m.bounds();
        m.shift((lo + hi) * 0.5);
        let (lo, hi) = m.bounds();
        assert!((lo.x + hi.x).abs() < 1e-9 && (lo.y + hi.y).abs() < 1e-9);
    }
}
