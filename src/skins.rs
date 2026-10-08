// Board skins. Squares are drawn in screen-square space (x, y in 0..8); square
// colour parity doesn't change when the board is flipped, so skins don't care.
use macroquad::prelude::*;
use macroquad::rand::gen_range;

pub const SKINS: [&str; 5] = ["Classic", "Wood", "Steel", "Neon", "Grass"];
pub const WOOD: usize = 1;
pub const STEEL: usize = 2;
pub const NEON: usize = 3;
pub const GRASS: usize = 4;

pub const SQ: f32 = 80.0;
pub const BOARD: f32 = SQ * 8.0;

fn light(x: usize, y: usize) -> bool {
    (x + 7 - y) % 2 == 1
}

fn hash(x: i32, y: i32, seed: i32) -> f32 {
    let mut h = (x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ seed.wrapping_mul(1442695041)) as u32;
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}

/// Smooth value noise in 0..1.
fn noise(x: f32, y: f32, seed: i32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = hash(xi, yi, seed) + (hash(xi + 1, yi, seed) - hash(xi, yi, seed)) * sx;
    let b = hash(xi, yi + 1, seed) + (hash(xi + 1, yi + 1, seed) - hash(xi, yi + 1, seed)) * sx;
    a + (b - a) * sy
}

/// Procedural board texture for the skins that use one (wood, steel).
pub fn texture(skin: usize) -> Option<Texture2D> {
    if skin != WOOD && skin != STEEL {
        return None;
    }
    const N: usize = 1024;
    const S: usize = N / 8;
    let mut img = Image::gen_image_color(N as u16, N as u16, BLACK);
    for py in 0..N {
        for px in 0..N {
            let (sx, sy, u, v) = (px / S, py / S, (px % S) as f32, (py % S) as f32);
            let seed = (sx * 8 + sy) as i32;
            let l = light(sx, sy);
            let (fx, fy) = (px as f32, py as f32);
            let c = if skin == WOOD {
                // grain runs across on light squares, down on dark ones, like an inlaid board
                let (along, across) = if l { (fx, fy) } else { (fy, fx) };
                let warp = noise(along * 0.008, across * 0.03, seed) * 14.0;
                let rings = (((across + warp + hash(seed, 0, 7) * 40.0) * 0.22).sin() * 0.5 + 0.5).powf(1.6);
                let fibre = noise(along * 0.02, across * 0.9, seed + 99);
                let k = 0.8 + 0.12 * rings + 0.1 * fibre;
                let base = if l { vec3(0.87, 0.71, 0.49) } else { vec3(0.42, 0.26, 0.15) };
                let edge = if u < 1.5 || v < 1.5 || u > S as f32 - 2.5 || v > S as f32 - 2.5 { 0.7 } else { 1.0 };
                base * k * edge
            } else {
                // brushed steel with a bevel on each square
                let streak = noise(fx * 0.01, fy * 0.8, seed) * 0.7 + noise(fx * 0.05, fy * 2.0, seed + 5) * 0.3;
                let sheen = 0.06 * ((fx + fy) * 0.006).sin();
                let base = if l { vec3(0.78, 0.8, 0.84) } else { vec3(0.38, 0.41, 0.46) };
                let bevel = if u < 4.0 || v < 4.0 {
                    1.25
                } else if u > S as f32 - 5.0 || v > S as f32 - 5.0 {
                    0.65
                } else {
                    1.0
                };
                base * (0.86 + 0.14 * streak + sheen) * bevel
            };
            img.set_pixel(px as u32, py as u32, Color::new(c.x.min(1.0), c.y.min(1.0), c.z.min(1.0), 1.0));
        }
    }
    let t = Texture2D::from_image(&img);
    t.set_filter(FilterMode::Linear);
    Some(t)
}

/// Frame + squares. `alpha` is the overlay translucency.
pub fn draw_base(skin: usize, tex: Option<&Texture2D>, alpha: f32, time: f32) {
    let a = |c: Color| Color::new(c.r, c.g, c.b, c.a * alpha);
    let frame = match skin {
        WOOD => Color::new(0.22, 0.13, 0.07, 1.0),
        STEEL => Color::new(0.2, 0.22, 0.25, 1.0),
        NEON => Color::new(0.02, 0.02, 0.05, 1.0),
        GRASS => Color::new(0.12, 0.26, 0.1, 1.0),
        _ => Color::from_rgba(58, 40, 30, 255),
    };
    draw_rectangle(-16.0, -16.0, BOARD + 32.0, BOARD + 32.0, a(frame));
    match skin {
        WOOD => draw_rectangle_lines(-6.0, -6.0, BOARD + 12.0, BOARD + 12.0, 2.0, a(Color::new(0.85, 0.65, 0.3, 0.8))),
        STEEL => {
            for i in 0..=8 {
                for (x, y) in [(i as f32 * SQ, -8.0), (i as f32 * SQ, BOARD + 8.0), (-8.0, i as f32 * SQ), (BOARD + 8.0, i as f32 * SQ)] {
                    draw_circle(x, y, 3.0, a(Color::new(0.6, 0.63, 0.68, 1.0)));
                    draw_circle(x - 0.8, y - 0.8, 1.4, a(Color::new(0.85, 0.87, 0.9, 1.0)));
                }
            }
        }
        GRASS => {
            for i in 0..90 {
                let t = i as f32 / 90.0 * 4.0;
                let side = t as usize;
                let f = t.fract() * (BOARD + 24.0) - 12.0;
                let (x, y) = match side {
                    0 => (f, -10.0),
                    1 => (BOARD + 10.0, f),
                    2 => (f, BOARD + 10.0),
                    _ => (-10.0, f),
                };
                let g = 0.22 + 0.12 * hash(i, 3, 11);
                draw_circle(x, y, 7.0 + 4.0 * hash(i, 1, 9), a(Color::new(0.1, g, 0.08, 1.0)));
            }
        }
        _ => {}
    }
    if let Some(t) = tex {
        draw_texture_ex(t, 0.0, 0.0, a(WHITE), DrawTextureParams { dest_size: Some(vec2(BOARD, BOARD)), ..Default::default() });
        return;
    }
    for y in 0..8 {
        for x in 0..8 {
            let l = light(x, y);
            let col = match skin {
                NEON => if l { Color::new(0.07, 0.06, 0.15, 1.0) } else { Color::new(0.02, 0.02, 0.06, 1.0) },
                GRASS => {
                    // mowed stripes
                    let stripe = if x % 2 == 0 { 0.05 } else { 0.0 };
                    if l { Color::new(0.4 + stripe, 0.62 + stripe, 0.24, 1.0) } else { Color::new(0.26 + stripe, 0.47 + stripe, 0.17, 1.0) }
                }
                _ => if l { Color::from_rgba(238, 218, 186, 255) } else { Color::from_rgba(180, 135, 99, 255) },
            };
            draw_rectangle(x as f32 * SQ, y as f32 * SQ, SQ, SQ, a(col));
        }
    }
    if skin == NEON {
        // glowing grid, hue drifting along the board
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

/// (fill, outline, glow) for a piece on this skin.
pub fn piece_colors(skin: usize, white: bool) -> (Color, Color, Option<Color>) {
    match (skin, white) {
        (WOOD, true) => (Color::new(0.96, 0.86, 0.66, 1.0), Color::new(0.25, 0.14, 0.06, 1.0), None),
        (WOOD, false) => (Color::new(0.2, 0.12, 0.07, 1.0), Color::new(0.9, 0.75, 0.5, 0.8), None),
        (STEEL, true) => (Color::new(0.88, 0.9, 0.94, 1.0), Color::new(0.15, 0.16, 0.2, 1.0), None),
        (STEEL, false) => (Color::new(0.2, 0.22, 0.26, 1.0), Color::new(0.75, 0.78, 0.84, 0.9), None),
        (NEON, true) => (Color::new(0.04, 0.08, 0.12, 1.0), Color::new(0.2, 0.95, 1.0, 1.0), Some(Color::new(0.2, 0.95, 1.0, 1.0))),
        (NEON, false) => (Color::new(0.1, 0.03, 0.1, 1.0), Color::new(1.0, 0.3, 0.9, 1.0), Some(Color::new(1.0, 0.3, 0.9, 1.0))),
        (_, true) => (Color::new(0.99, 0.97, 0.92, 1.0), Color::new(0.1, 0.08, 0.08, 1.0), None),
        (_, false) => (Color::new(0.13, 0.13, 0.16, 1.0), Color::new(0.92, 0.9, 0.86, 0.75), None),
    }
}

pub fn coord_color(skin: usize) -> Color {
    match skin {
        NEON => Color::new(0.4, 0.9, 1.0, 0.6),
        STEEL => Color::new(0.1, 0.12, 0.15, 0.7),
        GRASS => Color::new(0.08, 0.2, 0.05, 0.8),
        _ => Color::new(0.3, 0.2, 0.15, 0.7),
    }
}

/// One simulated blade: a damped spring around its rest lean, pushed by wind,
/// the cursor, moving pieces and capture blasts.
struct Blade {
    base: Vec2,
    h: f32,
    w: f32,
    rest: f32,
    a: f32,
    v: f32,
    tint: f32,
}

pub struct Grass {
    blades: Vec<Blade>,
}

impl Grass {
    pub fn new() -> Grass {
        let mut blades = vec![];
        for sq in 0..64 {
            let (x, y) = ((sq % 8) as f32 * SQ, (sq / 8) as f32 * SQ);
            for _ in 0..34 {
                blades.push(Blade {
                    base: vec2(x + gen_range(2.0, SQ - 2.0), y + gen_range(8.0, SQ)),
                    h: gen_range(9.0, 19.0),
                    w: gen_range(1.6, 3.2),
                    rest: gen_range(-0.25, 0.25),
                    a: 0.0,
                    v: 0.0,
                    tint: gen_range(0.0, 1.0),
                });
            }
        }
        // draw back to front so nearer blades overlap farther ones
        blades.sort_by(|a, b| a.base.y.total_cmp(&b.base.y));
        Grass { blades }
    }

    /// `pushers`: (position, radius) of things brushing through the grass.
    pub fn update(&mut self, dt: f32, time: f32, pushers: &[(Vec2, f32)]) {
        let dt = dt.min(1.0 / 30.0);
        for b in &mut self.blades {
            let gust = 0.18 * (time * 1.3 + b.base.x * 0.012 + b.base.y * 0.004).sin() + 0.08 * (time * 3.1 + b.base.x * 0.05).sin();
            let mut target = b.rest + gust;
            for &(p, r) in pushers {
                let d = b.base - p;
                let dist = d.length();
                if dist < r {
                    // bend away from whatever is passing, harder the closer it is
                    target += d.x.signum() * (1.0 - dist / r) * 1.3;
                }
            }
            let acc = 90.0 * (target - b.a) - 7.0 * b.v;
            b.v += acc * dt;
            b.a = (b.a + b.v * dt).clamp(-1.45, 1.45);
        }
    }

    /// Flattens blades away from `at` (captures).
    pub fn blast(&mut self, at: Vec2, strength: f32) {
        for b in &mut self.blades {
            let d = b.base - at;
            let dist = d.length().max(8.0);
            if dist < 160.0 {
                b.v += d.x.signum() * strength * (1.0 - dist / 160.0) * 30.0;
            }
        }
    }

    pub fn draw(&self, alpha: f32) {
        for b in &self.blades {
            let dir = |ang: f32| vec2(ang.sin(), -ang.cos());
            let mid = b.base + dir(b.a * 0.45) * b.h * 0.5;
            let tip = mid + dir(b.a) * b.h * 0.5;
            let side = vec2(b.w * 0.5, 0.0);
            let dark = Color::new(0.12 + 0.08 * b.tint, 0.35 + 0.12 * b.tint, 0.08, alpha);
            let lit = Color::new(0.35 + 0.2 * b.tint, 0.7 + 0.15 * b.tint, 0.2, alpha);
            draw_triangle(b.base - side, b.base + side, mid + side * 0.6, dark);
            draw_triangle(b.base - side, mid + side * 0.6, mid - side * 0.6, dark);
            draw_triangle(mid - side * 0.6, mid + side * 0.6, tip, lit);
        }
    }
}
