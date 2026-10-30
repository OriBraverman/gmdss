//! Test helper: draws a UI into an image with a small software rasterizer and saves it as PNG in
//! `target/snapshots/`, so panels can be looked at without opening a window.

use eframe::egui::{self, epaint::Primitive, Color32, ImageData, Pos2, Rect, TextureId, Vec2};
use std::collections::HashMap;
use std::path::PathBuf;

struct Texture {
    size: [usize; 2],
    pixels: Vec<Color32>,
}

/// Renders `ui_fn` on a `size` screen and writes `target/snapshots/<name>.png`.
pub fn render(name: &str, size: Vec2, mut ui_fn: impl FnMut(&mut egui::Ui)) -> PathBuf {
    let ctx = egui::Context::default();
    ctx.set_theme(egui::Theme::Dark);
    let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), ..Default::default() };
    let mut textures: HashMap<TextureId, Texture> = HashMap::new();
    let mut last = None;
    // The first pass lays out the fonts; the second one is drawn.
    for _ in 0..2 {
        let mut out = ctx.run_ui(input.clone(), |ui| ui_fn(ui));
        for (id, delta) in out.textures_delta.set.iter().flat_map(|(id, ds)| ds.iter().map(move |d| (id, d))) {
            let ImageData::Color(img) = &delta.image;
            match delta.pos {
                None => {
                    textures.insert(*id, Texture { size: img.size, pixels: img.pixels.clone() });
                }
                Some([x0, y0]) => {
                    let t = textures.get_mut(id).expect("patch of a known texture");
                    for y in 0..img.size[1] {
                        for x in 0..img.size[0] {
                            t.pixels[(y0 + y) * t.size[0] + x0 + x] = img.pixels[y * img.size[0] + x];
                        }
                    }
                }
            }
        }
        out.textures_delta.clear();
        last = Some(out);
    }
    let out = last.unwrap();
    let (w, h) = (size.x as usize, size.y as usize);
    let mut img = vec![[20.0f32, 20.0, 20.0, 255.0]; w * h];
    for prim in ctx.tessellate(out.shapes, out.pixels_per_point) {
        let Primitive::Mesh(mesh) = prim.primitive else { continue };
        let tex = &textures[&mesh.texture_id];
        let clip = prim.clip_rect;
        for tri in mesh.indices.chunks(3) {
            let v = [&mesh.vertices[tri[0] as usize], &mesh.vertices[tri[1] as usize], &mesh.vertices[tri[2] as usize]];
            let area = edge(v[0].pos, v[1].pos, v[2].pos);
            if area.abs() < 1e-6 {
                continue;
            }
            let lo = v.iter().fold(Pos2::new(f32::MAX, f32::MAX), |m, p| m.min(p.pos)).max(clip.min).max(Pos2::ZERO);
            let hi = v.iter().fold(Pos2::new(f32::MIN, f32::MIN), |m, p| m.max(p.pos)).min(clip.max).min(Pos2::new(w as f32, h as f32));
            for y in lo.y.floor() as usize..hi.y.ceil().max(0.0) as usize {
                for x in lo.x.floor() as usize..hi.x.ceil().max(0.0) as usize {
                    let p = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let b = [edge(v[1].pos, v[2].pos, p) / area, edge(v[2].pos, v[0].pos, p) / area, edge(v[0].pos, v[1].pos, p) / area];
                    if b.iter().any(|&t| t < 0.0) {
                        continue;
                    }
                    let uv = v[0].uv.to_vec2() * b[0] + v[1].uv.to_vec2() * b[1] + v[2].uv.to_vec2() * b[2];
                    let tx = ((uv.x * tex.size[0] as f32) as usize).min(tex.size[0] - 1);
                    let ty = ((uv.y * tex.size[1] as f32) as usize).min(tex.size[1] - 1);
                    let t = tex.pixels[ty * tex.size[0] + tx].to_array();
                    let mut src = [0.0f32; 4];
                    for (c, s) in src.iter_mut().enumerate() {
                        let vc: f32 = (0..3).map(|k| b[k] * f32::from(v[k].color.to_array()[c])).sum();
                        *s = vc * f32::from(t[c]) / 255.0; // both premultiplied
                    }
                    let d = &mut img[y * w + x];
                    for c in 0..4 {
                        d[c] = src[c] + d[c] * (1.0 - src[3] / 255.0);
                    }
                }
            }
        }
    }
    let rgba: Vec<u8> = img.iter().flat_map(|p| p.map(|c| c.round().clamp(0.0, 255.0) as u8)).collect();
    let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/snapshots"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.png"));
    std::fs::write(&path, png(w, h, &rgba)).unwrap();
    path
}

fn edge(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

/// Uncompressed RGBA PNG (stored deflate blocks).
fn png(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    let crc = |data: &[u8]| {
        let mut c = !0u32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    };
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for row in rgba.chunks(w * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, blk) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let n = blk.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(blk);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc(&body).to_be_bytes());
    };
    let mut ihdr = (w as u32).to_be_bytes().to_vec();
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}
