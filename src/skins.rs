// Board skins. Squares are drawn in screen-square space (x, y in 0..8); square
// colour parity doesn't change when the board is flipped, so skins don't care.
// Pieces and boards are Staunton renders by James Clarke (MIT), see assets/CREDITS.md.
use macroquad::models::{draw_mesh, Mesh, Vertex};
use macroquad::prelude::*;
use macroquad::rand::gen_range;

pub const SKINS: [&str; 5] = ["Classic", "Wood", "Steel", "Neon", "Grass"];
pub const WOOD: usize = 1;
pub const STEEL: usize = 2;
pub const NEON: usize = 3;
pub const GRASS: usize = 4;

pub const SQ: f32 = 80.0;
pub const BOARD: f32 = SQ * 8.0;

macro_rules! piece_set {
    ($dir:literal) => {
        [
            include_bytes!(concat!("../assets/pieces/", $dir, "/White-King.png")).as_slice(),
            include_bytes!(concat!("../assets/pieces/", $dir, "/White-Queen.png")),
            include_bytes!(concat!("../assets/pieces/", $dir, "/White-Rook.png")),
            include_bytes!(concat!("../assets/pieces/", $dir, "/White-Bishop.png")),
            include_bytes!(concat!("../assets/pieces/", $dir, "/White-Knight.png")),
            include_bytes!(concat!("../assets/pieces/", $dir, "/White-Pawn.png")),
            include_bytes!(concat!("../assets/pieces/", $dir, "/Black-King.png")),
            include_bytes!(concat!("../assets/pieces/", $dir, "/Black-Queen.png")),
            include_bytes!(concat!("../assets/pieces/", $dir, "/Black-Rook.png")),
            include_bytes!(concat!("../assets/pieces/", $dir, "/Black-Bishop.png")),
            include_bytes!(concat!("../assets/pieces/", $dir, "/Black-Knight.png")),
            include_bytes!(concat!("../assets/pieces/", $dir, "/Black-Pawn.png")),
        ]
    };
}

/// A piece render plus the box its visible pixels occupy (renders have uneven padding).
pub struct Sprite {
    pub tex: Texture2D,
    pub bbox: Rect,
}

/// 12 piece sprites for a skin, white K Q R B N P then black.
pub fn pieces(skin: usize) -> Vec<Sprite> {
    let set: [&[u8]; 12] = match skin {
        WOOD => piece_set!("wood"),
        STEEL => piece_set!("metal"),
        NEON => piece_set!("glass"),
        GRASS => piece_set!("modernwood"),
        _ => piece_set!("basic"),
    };
    set.iter()
        .map(|b| {
            let img = Image::from_file_with_format(b, None).unwrap();
            let (w, h) = (img.width as u32, img.height as u32);
            let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
            for y in 0..h {
                for x in 0..w {
                    if img.get_pixel(x, y).a > 0.15 {
                        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                    }
                }
            }
            let tex = Texture2D::from_image(&img);
            tex.set_filter(FilterMode::Linear);
            Sprite { tex, bbox: Rect::new(x0 as f32, y0 as f32, (x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32) }
        })
        .collect()
}

pub fn piece_index(white: bool, role: shakmaty::Role) -> usize {
    use shakmaty::Role::*;
    let r = match role {
        King => 0,
        Queen => 1,
        Rook => 2,
        Bishop => 3,
        Knight => 4,
        Pawn => 5,
    };
    if white { r } else { r + 6 }
}

pub fn load(bytes: &[u8]) -> Texture2D {
    let t = Texture2D::from_file_with_format(bytes, None);
    t.set_filter(FilterMode::Linear);
    t
}

/// Board texture for skins that have one.
pub fn board(skin: usize) -> Option<Texture2D> {
    let bytes: &[u8] = match skin {
        WOOD => include_bytes!("../assets/boards/rosewood.png"),
        STEEL => include_bytes!("../assets/boards/brushed-aluminium.png"),
        NEON => include_bytes!("../assets/boards/purple-black.png"),
        _ => return None,
    };
    Some(load(bytes))
}

fn light(x: usize, y: usize) -> bool {
    (x + 7 - y) % 2 == 1
}

/// Frame + squares. `alpha` is the overlay translucency.
pub fn draw_base(skin: usize, tex: Option<&Texture2D>, alpha: f32, time: f32) {
    let a = |c: Color| Color::new(c.r, c.g, c.b, c.a * alpha);
    let frame = match skin {
        WOOD => Color::new(0.3, 0.12, 0.08, 1.0),
        STEEL => Color::new(0.2, 0.22, 0.25, 1.0),
        NEON => Color::new(0.02, 0.02, 0.05, 1.0),
        GRASS => Color::new(0.2, 0.15, 0.09, 1.0),
        _ => Color::from_rgba(58, 40, 30, 255),
    };
    draw_rectangle(-16.0, -16.0, BOARD + 32.0, BOARD + 32.0, a(frame));
    if let Some(t) = tex {
        // neon dims its board so the glow carries the look
        let tint = if skin == NEON { Color::new(0.3, 0.26, 0.42, 1.0) } else { WHITE };
        draw_texture_ex(t, 0.0, 0.0, a(tint), DrawTextureParams { dest_size: Some(vec2(BOARD, BOARD)), ..Default::default() });
    } else {
        for y in 0..8 {
            for x in 0..8 {
                let l = light(x, y);
                let col = match skin {
                    // the soil the grass grows from, mowed in stripes
                    GRASS => if l { Color::new(0.24, 0.36, 0.12, 1.0) } else { Color::new(0.15, 0.25, 0.08, 1.0) },
                    _ => if l { Color::from_rgba(238, 218, 186, 255) } else { Color::from_rgba(180, 135, 99, 255) },
                };
                draw_rectangle(x as f32 * SQ, y as f32 * SQ, SQ, SQ, a(col));
            }
        }
    }
    if skin == NEON {
        // glowing grid over the real board, hue drifting along it
        for i in 0..=8 {
            let p = i as f32 * SQ;
            for (w, k) in [(9.0, 0.06), (4.0, 0.15), (1.5, 0.9)] {
                let pulse = 0.75 + 0.25 * (time * 2.0 + i as f32 * 0.6).sin();
                let c1 = Color::new(0.1, 0.9, 1.0, k * pulse * alpha);
                let c2 = Color::new(1.0, 0.2, 0.9, k * pulse * alpha);
                draw_line(p, 0.0, p, BOARD, w, if i % 2 == 0 { c1 } else { c2 });
                draw_line(0.0, p, BOARD, p, w, if i % 2 == 0 { c2 } else { c1 });
            }
        }
    }
}

/// Glow colour behind pieces (neon only).
pub fn glow(skin: usize, white: bool) -> Option<Color> {
    (skin == NEON).then(|| if white { Color::new(0.2, 0.95, 1.0, 1.0) } else { Color::new(1.0, 0.3, 0.9, 1.0) })
}

pub fn coord_color(skin: usize) -> Color {
    match skin {
        NEON => Color::new(0.4, 0.9, 1.0, 0.7),
        STEEL => Color::new(0.1, 0.12, 0.15, 0.8),
        GRASS => Color::new(0.9, 1.0, 0.7, 0.6),
        _ => Color::new(0.3, 0.2, 0.15, 0.7),
    }
}

const SEGMENTS: usize = 4;

/// One simulated blade: a damped spring around its rest lean, pushed by gusts,
/// the cursor, pieces (moving or resting), and capture blasts. Trampling sticks
/// for a moment and springs back.
struct Blade {
    base: Vec2,
    h: f32,
    w: f32,
    rest: f32,
    a: f32,
    v: f32,
    flat: f32,
    flat_dir: f32,
    hue: f32,
}

pub struct Grass {
    blades: Vec<Blade>,
}

impl Grass {
    pub fn new() -> Grass {
        let mut blades: Vec<Blade> = (0..64 * 95)
            .map(|_| Blade {
                base: vec2(gen_range(-4.0, BOARD + 4.0), gen_range(2.0, BOARD + 2.0)),
                h: gen_range(14.0, 30.0),
                w: gen_range(2.0, 4.0),
                rest: gen_range(-0.3, 0.3),
                a: 0.0,
                v: 0.0,
                flat: 0.0,
                flat_dir: 1.0,
                hue: gen_range(0.0, 1.0),
            })
            .collect();
        // back to front, so nearer blades overlap farther ones
        blades.sort_by(|a, b| a.base.y.total_cmp(&b.base.y));
        Grass { blades }
    }

    /// `pushers`: (position, radius) of things brushing through; `resting`: piece bases.
    pub fn update(&mut self, dt: f32, t: f32, pushers: &[(Vec2, f32)], resting: &[Vec2]) {
        let dt = dt.min(1.0 / 30.0);
        // gusts roll across the field as travelling waves, with a slow swell
        let swell = 0.6 + 0.4 * (t * 0.27).sin();
        for b in &mut self.blades {
            let p = b.base;
            let gust = swell * (0.22 * ((p.x * 0.8 + p.y * 0.6) * 0.018 - t * 1.7).sin() + 0.1 * (p.x * 0.05 - t * 3.4 + p.y * 0.013).sin());
            let mut target = b.rest + gust;
            for &(q, r) in pushers {
                let d = p - q;
                let dist = d.length();
                if dist < r {
                    let k = 1.0 - dist / r;
                    target += d.x.signum() * k * 1.4;
                    if k > b.flat {
                        b.flat = k;
                        b.flat_dir = d.x.signum();
                    }
                }
            }
            for &q in resting {
                let d = p - q;
                if d.y.abs() < 14.0 && d.x.abs() < 26.0 {
                    target += d.x.signum() * (1.0 - d.x.abs() / 26.0) * 0.9; // nest around the piece
                }
            }
            target += b.flat_dir * b.flat * 0.9;
            b.flat = (b.flat - dt * 0.7).max(0.0);
            let acc = 70.0 * (target - b.a) - 6.0 * b.v;
            b.v += acc * dt;
            b.a = (b.a + b.v * dt).clamp(-1.5, 1.5);
        }
    }

    /// Flattens blades away from `at` (captures).
    pub fn blast(&mut self, at: Vec2, strength: f32) {
        for b in &mut self.blades {
            let d = b.base - at;
            let dist = d.length().max(8.0);
            if dist < 170.0 {
                let k = strength * (1.0 - dist / 170.0);
                b.v += d.x.signum() * k * 30.0;
                b.flat = b.flat.max(k);
                b.flat_dir = d.x.signum();
            }
        }
    }

    /// Index of the first blade rooted at or below `y` (blades are sorted by root y).
    pub fn split(&self, y: f32) -> usize {
        self.blades.partition_point(|b| b.base.y < y)
    }

    pub fn len(&self) -> usize {
        self.blades.len()
    }

    /// Draws blades `from..to` as tapered, curved strips shaded from root to tip.
    pub fn draw(&self, from: usize, to: usize, alpha: f32) {
        const PER_MESH: usize = 180; // stays under macroquad's per-draw index budget
        for chunk in self.blades[from..to].chunks(PER_MESH) {
            let mut vertices = Vec::with_capacity(chunk.len() * (SEGMENTS + 1) * 2);
            let mut indices: Vec<u16> = Vec::with_capacity(chunk.len() * SEGMENTS * 6);
            for b in chunk {
                let h = b.h * (1.0 - 0.45 * b.flat);
                let base_col = vec3(0.07 + 0.05 * b.hue, 0.18 + 0.06 * b.hue, 0.04);
                let tip_col = vec3(0.45 + 0.25 * b.hue, 0.75 + 0.12 * b.hue, 0.22 + 0.1 * (1.0 - b.hue)) * (1.0 + 0.18 * b.a.sin());
                let mut p = b.base;
                let first = vertices.len() as u16;
                for s in 0..=SEGMENTS {
                    let k = s as f32 / SEGMENTS as f32;
                    let half = b.w * 0.5 * (1.0 - k).powf(0.8);
                    let c = base_col.lerp(tip_col, k.powf(0.7));
                    let col = Color::new(c.x.min(1.0), c.y.min(1.0), c.z.min(1.0), alpha);
                    vertices.push(Vertex::new(p.x - half, p.y, 0.0, 0.0, 0.0, col));
                    vertices.push(Vertex::new(p.x + half, p.y, 0.0, 0.0, 0.0, col));
                    // the blade curves more towards the tip
                    let ang = b.a * (k + 1.0 / SEGMENTS as f32).powf(1.4);
                    p += vec2(ang.sin(), -ang.cos()) * h / SEGMENTS as f32;
                }
                for s in 0..SEGMENTS as u16 {
                    let i = first + s * 2;
                    indices.extend_from_slice(&[i, i + 1, i + 2, i + 1, i + 3, i + 2]);
                }
            }
            draw_mesh(&Mesh { vertices, indices, texture: None });
        }
    }
}
