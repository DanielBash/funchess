#![cfg_attr(windows, windows_subsystem = "windows")]
mod ai;
mod book;
mod overlay;
mod skins;

use ai::{Grade, MAGNUS, MAX_ELO};
use macroquad::prelude::*;
use macroquad::rand::gen_range;
use shakmaty::{Chess, Color as Side, File, Move, Piece, Position, Rank, Role, Square};
use std::f32::consts::PI;

use skins::{BOARD, SQ};
const PANEL: f32 = 340.0;
const STRIP: f32 = 100.0;
/// Practice lines: the bot sticks to these while you do, then continues like real players would.
const OPENINGS: [(&str, &str); 14] = [
    ("Any (bots play like humans)", ""),
    ("Italian Game", "e4 e5 Nf3 Nc6 Bc4"),
    ("Ruy Lopez", "e4 e5 Nf3 Nc6 Bb5"),
    ("Sicilian Defense", "e4 c5"),
    ("French Defense", "e4 e6"),
    ("Caro-Kann Defense", "e4 c6"),
    ("Scandinavian Defense", "e4 d5"),
    ("Queen's Gambit", "d4 d5 c4"),
    ("London System", "d4 d5 Bf4"),
    ("King's Indian Defense", "d4 Nf6 c4 g6"),
    ("Trap: Fried Liver Attack", "e4 e5 Nf3 Nc6 Bc4 Nf6 Ng5 d5 exd5 Nxd5"),
    ("Trap: Stafford Gambit", "e4 e5 Nf3 Nf6 Nxe5 Nc6"),
    ("Trap: Scholar's Mate try", "e4 e5 Qh5 Nc6 Bc4"),
    ("Trap: Englund Gambit", "d4 e5 dxe5 Nc6 Nf3 Qe7"),
];

/// (label, base seconds, increment)
const TCS: [(&str, f32, f32); 5] = [("No clock", 0.0, 0.0), ("1+0", 60.0, 0.0), ("3+2", 180.0, 2.0), ("5+0", 300.0, 0.0), ("10+0", 600.0, 0.0)];

const MAGNUS_TIER: usize = 8;
const TIERS: [(f32, &str, [f32; 3]); 12] = [
    (0.0, "Potato", [0.55, 0.8, 0.35]),
    (400.0, "Beginner", [0.3, 0.85, 0.55]),
    (800.0, "Casual", [0.25, 0.8, 0.85]),
    (1200.0, "Club Player", [0.3, 0.55, 1.0]),
    (1600.0, "Competitive", [0.55, 0.4, 1.0]),
    (2000.0, "Expert", [0.85, 0.35, 0.95]),
    (2300.0, "Master", [1.0, 0.35, 0.5]),
    (2500.0, "Grandmaster", [1.0, 0.55, 0.2]),
    (MAGNUS - 20.0, "MAGNUS CARLSEN", [1.0, 0.84, 0.2]),
    // beyond Magnus: same human style, mistakes extrapolated from the data, then faded out
    (2950.0, "Beyond Magnus", [0.55, 1.0, 0.85]),
    (3150.0, "Peak Human", [0.6, 0.85, 1.0]),
    (3350.0, "Theoretical Human", [1.0, 1.0, 1.0]),
];

fn tier(e: f32) -> usize {
    TIERS.iter().rposition(|t| e >= t.0).unwrap_or(0)
}

/// Position along the ELO bar (0..1); every tier gets an equal slice, so close-together
/// tiers (like the ones past Magnus) don't crowd.
fn elo_frac(e: f32) -> f32 {
    let i = tier(e);
    let a = TIERS[i].0.max(1.0);
    let b = TIERS.get(i + 1).map_or(MAX_ELO, |t| t.0);
    (i as f32 + ((e - a) / (b - a)).clamp(0.0, 1.0)) / TIERS.len() as f32
}

fn frac_elo(f: f32) -> f32 {
    let x = f.clamp(0.0, 1.0) * TIERS.len() as f32;
    let i = (x as usize).min(TIERS.len() - 1);
    let a = TIERS[i].0.max(1.0);
    let b = TIERS.get(i + 1).map_or(MAX_ELO, |t| t.0);
    a + (b - a) * (x - i as f32)
}

fn elo_color(e: f32) -> Color {
    let i = tier(e);
    let (a, ca) = (TIERS[i].0, TIERS[i].2);
    let (b, cb) = TIERS.get(i + 1).map_or((MAX_ELO, ca), |t| (t.0, t.2));
    let t = ((e - a) / (b - a).max(1.0)).clamp(0.0, 1.0);
    Color::new(ca[0] + (cb[0] - ca[0]) * t, ca[1] + (cb[1] - ca[1]) * t, ca[2] + (cb[2] - ca[2]) * t, 1.0)
}

fn grade_color(g: Grade) -> Color {
    match g {
        Grade::Brilliant => Color::new(0.1, 0.8, 0.8, 1.0),
        Grade::Great => Color::new(0.35, 0.55, 1.0, 1.0),
        Grade::Book => Color::new(0.68, 0.53, 0.38, 1.0),
        Grade::Best => Color::new(0.45, 0.75, 0.2, 1.0),
        Grade::Excellent => Color::new(0.5, 0.8, 0.35, 1.0),
        Grade::Good => Color::new(0.55, 0.7, 0.5, 1.0),
        Grade::Inaccuracy => Color::new(0.95, 0.78, 0.2, 1.0),
        Grade::Mistake => Color::new(1.0, 0.55, 0.15, 1.0),
        Grade::Blunder => Color::new(0.95, 0.2, 0.2, 1.0),
    }
}

fn mood_color(m: ai::Mood) -> Color {
    match m {
        ai::Mood::Calm => GRAY,
        ai::Mood::Attacking => Color::new(1.0, 0.45, 0.3, 1.0),
        ai::Mood::Defending => Color::new(0.4, 0.7, 1.0, 1.0),
        ai::Mood::Hurrying => Color::new(1.0, 0.85, 0.2, 1.0),
    }
}

fn with_a(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}

fn ease_out_back(t: f32) -> f32 {
    let c = 1.70158;
    1.0 + (c + 1.0) * (t - 1.0).powi(3) + c * (t - 1.0).powi(2)
}

/// Where the king lands for castling (shakmaty encodes castling as king-takes-rook).
fn ui_to(m: &Move) -> Square {
    match *m {
        Move::Castle { king, rook } => {
            Square::from_coords(if rook.file() > king.file() { File::G } else { File::C }, king.rank())
        }
        _ => m.to(),
    }
}

struct Particle {
    p: Vec2,
    v: Vec2,
    g: f32,
    life: f32,
    max: f32,
    col: Color,
    size: f32,
    screen: bool,
}

struct Toast {
    text: String,
    p: Vec2,
    t: f32,
    col: Color,
}

struct Slide {
    piece: Piece,
    from: Vec2,
    to: Vec2,
    dest: Square,
}

struct Anim {
    slides: Vec<Slide>,
    t: f32,
    ghost: Option<(Piece, Vec2)>,
    fx: bool,
    tag: Option<String>,
}

/// "From position" board editor.
struct Editor {
    board: shakmaty::Board,
    turn: Side,
    brush: Option<Piece>,
    err: Option<String>,
}

struct App {
    pos: Chess,
    history: Vec<Chess>,
    last: Option<(Square, Square)>,
    player: Side,
    sel: Option<Square>,
    sel_t: f32,
    dragging: bool,
    hover_amt: [f32; 64],
    anim: Option<Anim>,
    parts: Vec<Particle>,
    toasts: Vec<Toast>,
    zoom: f32,
    zoom_to: f32,
    center: Vec2,
    center_to: Vec2,
    shake: f32,
    punch: f32,
    pan_last: Option<Vec2>,
    elo: f32,
    elo_disp: f32,
    elo_vel: f32,
    elo_drag: bool,
    tier: usize,
    tier_pop: f32,
    tick_pop: f32,
    game_id: u64,
    waiting: bool,
    think_start: f64,
    pending: Option<(Move, Option<String>, f64)>,
    over_t: f32,
    engine: ai::Engine,
    skin: usize,
    skin_tex: Option<Texture2D>,
    piece_tex: Vec<Texture2D>,
    icons: Vec<Texture2D>,
    grass: Option<skins::Grass>,
    editor: Option<Editor>,
    saved: (f32, usize, usize),
    played: Vec<Move>,
    opening: usize,
    opening_name: Option<(String, f32)>,
    real_game: Option<(String, String)>,
    tc: usize,
    clock: [f32; 2], // indexed by Side as usize: black 0, white 1
    mood: ai::Mood,
    overlay: Option<overlay::Overlay>,
    analysis: Option<(usize, Vec<(Move, i32)>)>,
    analysed: Option<(u64, usize)>,
    want_grade: Option<(usize, Chess, Move, Square)>,
    badge: Option<(Square, Grade, f32)>,
    win_drag: Option<(Vec2, Vec2)>,
    font: Font,
    bold: Font,
}

impl App {
    fn new_game(&mut self, side: Side) {
        self.pos = Chess::default();
        self.history.clear();
        self.player = side;
        self.played.clear();
        self.opening_name = None;
        self.real_game = None;
        self.clock = [TCS[self.tc].1; 2];
        self.mood = ai::Mood::Calm;
        self.reset_turn_state();
    }

    /// Next move of the chosen practice line, if the game is still on it.
    fn line_move(&self) -> Option<Move> {
        let mut pos = Chess::default();
        let mut line = vec![];
        for t in OPENINGS[self.opening].1.split_whitespace() {
            let m = t.parse::<shakmaty::san::San>().ok()?.to_move(&pos).ok()?;
            pos.play_unchecked(m);
            line.push(m);
        }
        let n = self.played.len();
        (n < line.len() && *self.history.first().unwrap_or(&self.pos) == Chess::default() && line[..n] == self.played[..]).then(|| line[n])
    }

    fn flagged(&self) -> Option<Side> {
        [Side::White, Side::Black].into_iter().find(|&s| self.tc > 0 && self.clock[s as usize] <= 0.0)
    }

    fn over(&self) -> bool {
        self.pos.is_game_over() || self.flagged().is_some()
    }

    fn winner(&self) -> Option<Side> {
        self.flagged().map(|s| !s).or(self.pos.outcome().winner())
    }

    fn fmt_clock(t: f32) -> String {
        let t = t.max(0.0);
        if t < 10.0 { format!("{:.1}", t) } else { format!("{}:{:02}", t as u32 / 60, t as u32 % 60) }
    }

    fn reset_turn_state(&mut self) {
        self.last = None;
        self.sel = None;
        self.dragging = false;
        self.anim = None;
        self.game_id += 1;
        self.waiting = false;
        self.pending = None;
        self.over_t = 0.0;
        self.analysis = None;
        self.want_grade = None;
        self.badge = None;
        self.engine.stop();
    }

    fn set_skin(&mut self, skin: usize) {
        self.skin = skin % skins::SKINS.len();
        self.skin_tex = skins::board(self.skin);
        self.piece_tex = skins::pieces(self.skin);
        self.grass = (self.skin == skins::GRASS).then(skins::Grass::new);
    }

    fn save_settings(&mut self) {
        let now = (self.elo.round(), self.skin, self.tc);
        if now != self.saved && !self.elo_drag {
            self.saved = now;
            if let Some(p) = settings_path() {
                let _ = std::fs::create_dir_all(p.parent().unwrap());
                let _ = std::fs::write(p, format!("elo={}\nskin={}\ntc={}\n", now.0, now.1, now.2));
            }
        }
    }

    fn editor_play(&mut self, side: Side) {
        let Some(ed) = &mut self.editor else { return };
        let fen = format!("{} {} KQkq - 0 1", ed.board, if ed.turn == Side::White { "w" } else { "b" });
        let pos = fen
            .parse::<shakmaty::fen::Fen>()
            .map_err(|e| e.to_string())
            .and_then(|f| match f.into_position::<Chess>(shakmaty::CastlingMode::Standard) {
                Ok(p) => Ok(p),
                Err(e) => e.ignore_invalid_castling_rights().map_err(|e| e.to_string()),
            });
        match pos {
            Ok(p) => {
                self.editor = None;
                self.new_game(side);
                self.pos = p;
                self.opening_name = Some(("Custom position".into(), 0.0));
            }
            Err(e) => ed.err = Some(format!("Not a legal position: {e}")),
        }
    }

    fn board_alpha(&self) -> f32 {
        if self.overlay.is_some() { 0.88 } else { 1.0 }
    }

    fn panel_w(&self) -> f32 {
        if self.overlay.is_some() { 0.0 } else { PANEL }
    }

    fn strip_h(&self) -> f32 {
        if self.overlay.is_some() { STRIP } else { 0.0 }
    }

    /// Restart in the other mode, carrying the game over (window flags can't change at runtime).
    fn relaunch(&self, overlay: bool) {
        let fen = shakmaty::fen::Fen::from_position(&self.pos, shakmaty::EnPassantMode::Legal).to_string();
        let mut args = vec!["--fen".to_string(), fen, "--side".into(), (if self.player == Side::White { "w" } else { "b" }).into()];
        args.extend(["--elo".into(), format!("{:.0}", self.elo), "--tc".into(), self.tc.to_string()]);
        args.extend(["--clock".into(), format!("{},{}", self.clock[0], self.clock[1])]);
        args.extend(["--opening".into(), self.opening.to_string(), "--skin".into(), self.skin.to_string()]);
        if overlay {
            args.push("--overlay".into());
        }
        if let Ok(exe) = std::env::current_exe() {
            if std::process::Command::new(exe).args(args).spawn().is_ok() {
                std::process::exit(0);
            }
        }
    }

    fn try_grade(&mut self) {
        let (Some((ply, before, m, sq)), Some((aply, lines))) = (&self.want_grade, &self.analysis) else { return };
        if ply != aply {
            return;
        }
        let (sq, grade) = (*sq, ai::classify(before, lines, *m));
        // a sound move that lots of real players play here = book
        let pop = ai::book().all(book::key(before));
        let total: u32 = pop.iter().map(|b| b.count).sum();
        let share = pop.iter().find(|b| b.mv == book::encode(*m)).map_or(0.0, |b| b.count as f32 / total as f32);
        let grade = grade.map(|g| match g {
            Grade::Great | Grade::Best | Grade::Excellent | Grade::Good if share >= 0.03 && total >= 50 => Grade::Book,
            g => g,
        });
        self.want_grade = None;
        let Some(g) = grade else { return };
        self.badge = Some((sq, g, 0.0));
        let c = self.sq_pos(sq);
        let col = grade_color(g);
        let label = match g {
            Grade::Book => format!("Book · {:.0}% play this", share * 100.0),
            _ if share > 0.0 && total >= 50 => format!("{} · {:.0}% play this", g.label().1, share * 100.0),
            _ => g.label().1.to_string(),
        };
        self.toast(&label, c - vec2(0.0, 46.0), col);
        match g {
            Grade::Brilliant => {
                self.burst(c, col, 60, 340.0, false);
                self.burst(c, WHITE, 20, 200.0, false);
                self.punch += 0.07;
            }
            Grade::Great => {
                self.burst(c, col, 35, 260.0, false);
                self.punch += 0.04;
            }
            Grade::Best | Grade::Excellent => self.burst(c, col, 14, 150.0, false),
            Grade::Mistake => self.shake += 6.0,
            Grade::Blunder => {
                self.shake += 16.0;
                self.burst(c, col, 30, 220.0, false);
            }
            _ => {}
        }
    }

    fn undo(&mut self) {
        while let Some(p) = self.history.pop() {
            self.pos = p;
            self.played.pop();
            if self.pos.turn() == self.player {
                break;
            }
        }
        self.reset_turn_state();
    }

    fn sq_pos(&self, sq: Square) -> Vec2 {
        let (f, r) = (sq.file().to_u32() as f32, sq.rank().to_u32() as f32);
        let (x, y) = if self.player == Side::Black { (7.0 - f, r) } else { (f, 7.0 - r) };
        vec2(x * SQ + SQ / 2.0, y * SQ + SQ / 2.0)
    }

    fn world_sq(&self, p: Vec2) -> Option<Square> {
        if p.x < 0.0 || p.y < 0.0 || p.x >= BOARD || p.y >= BOARD {
            return None;
        }
        let (x, y) = ((p.x / SQ) as u32, (p.y / SQ) as u32);
        let (f, r) = if self.player == Side::Black { (7 - x, y) } else { (x, 7 - y) };
        Some(Square::from_coords(File::new(f), Rank::new(r)))
    }

    fn find_move(&self, from: Square, to: Square) -> Option<Move> {
        // ponytail: auto-queen, add a picker if underpromotion ever matters
        self.pos.legal_moves().into_iter().find(|m| {
            m.from() == Some(from) && ui_to(m) == to && matches!(m.promotion(), None | Some(Role::Queen))
        })
    }

    fn scale(&self) -> f32 {
        let margin = if self.overlay.is_some() { 0.95 } else { 0.86 };
        (screen_width() - self.panel_w()).min(screen_height() - self.strip_h()) * margin / BOARD * self.zoom * (1.0 + self.punch)
    }

    fn camera(&self, jitter: bool) -> Camera2D {
        let (sw, sh) = (screen_width(), screen_height());
        let s = self.scale();
        let j = if jitter { vec2(gen_range(-1.0, 1.0), gen_range(-1.0, 1.0)) * self.shake / s } else { Vec2::ZERO };
        Camera2D {
            target: self.center + j,
            zoom: vec2(2.0 * s / sw, 2.0 * s / sh),
            offset: vec2((sw - self.panel_w()) / sw - 1.0, self.strip_h() / sh),
            ..Default::default()
        }
    }

    fn bar_rect(&self) -> Rect {
        Rect::new(screen_width() - PANEL + 50.0, 225.0, 34.0, screen_height() - 225.0 - 355.0)
    }

    fn knob_y(&self) -> f32 {
        let b = self.bar_rect();
        let e = self.elo_disp.clamp(1.0, MAX_ELO);
        let f = (elo_frac(e) + (self.elo_disp - e) / 2000.0).clamp(-0.03, 1.03);
        b.y + b.h * (1.0 - f)
    }

    fn burst(&mut self, at: Vec2, col: Color, n: usize, speed: f32, screen: bool) {
        for _ in 0..n {
            let a = gen_range(0.0, PI * 2.0);
            let v = vec2(a.cos(), a.sin()) * gen_range(0.2, 1.0) * speed;
            let max = gen_range(0.4, 0.9);
            self.parts.push(Particle { p: at, v, g: 0.0, life: max, max, col, size: gen_range(2.0, 6.0), screen });
        }
    }

    fn confetti(&mut self) {
        for _ in 0..220 {
            let col = Color::new(gen_range(0.4, 1.0), gen_range(0.4, 1.0), gen_range(0.4, 1.0), 1.0);
            let max = gen_range(2.0, 3.5);
            self.parts.push(Particle {
                p: vec2(gen_range(0.0, BOARD), gen_range(-200.0, -20.0)),
                v: vec2(gen_range(-80.0, 80.0), gen_range(0.0, 200.0)),
                g: 260.0,
                life: max,
                max,
                col,
                size: gen_range(4.0, 8.0),
                screen: false,
            });
        }
    }

    fn toast(&mut self, text: &str, p: Vec2, col: Color) {
        self.toasts.push(Toast { text: text.into(), p, t: 0.0, col });
    }

    fn play(&mut self, m: Move, start: Option<Vec2>, tag: Option<String>) {
        let from = m.from().unwrap();
        let to = ui_to(&m);
        let mover = self.pos.turn();
        if self.tc > 0 {
            self.clock[mover as usize] += TCS[self.tc].2;
        }
        let cap_sq = if m.is_en_passant() {
            Some(Square::from_coords(m.to().file(), from.rank()))
        } else {
            m.capture().map(|_| m.to())
        };
        let ghost = cap_sq.and_then(|s| self.pos.board().piece_at(s).map(|p| (p, self.sq_pos(s))));
        self.badge = None;
        if mover == self.player {
            self.want_grade = Some((self.history.len(), self.pos.clone(), m, to));
            if self.analysis.as_ref().map(|a| a.0) != Some(self.history.len()) {
                self.engine.stop(); // analysis still running: take what it has now
            }
        }
        self.history.push(self.pos.clone());
        self.played.push(m);
        self.pos.play_unchecked(m);

        let mut slides = vec![Slide {
            piece: self.pos.board().piece_at(to).unwrap(),
            from: start.unwrap_or(self.sq_pos(from)),
            to: self.sq_pos(to),
            dest: to,
        }];
        if let Move::Castle { king, rook } = m {
            let rto = Square::from_coords(if rook.file() > king.file() { File::F } else { File::D }, king.rank());
            slides.push(Slide { piece: Piece { color: mover, role: Role::Rook }, from: self.sq_pos(rook), to: self.sq_pos(rto), dest: rto });
        }
        self.anim = Some(Anim { slides, t: 0.0, ghost, fx: true, tag });
        self.last = Some((from, to));
        self.sel = None;
        self.dragging = false;
        self.try_grade();
    }

    fn landed(&mut self, a: Anim) {
        if !a.fx {
            return;
        }
        let ph = book::phase(&self.pos, self.played.len());
        if ph >= 2 && book::phase(self.history.last().unwrap_or(&self.pos), self.played.len().saturating_sub(1)) != ph {
            let band = book::band(self.elo);
            let label = match ai::book().slip_rate(band, ph) {
                Some(r) => format!("{} · {} players slip on {:.0}% of moves here", book::PHASES[ph], book::band_label(band), r * 100.0),
                None => book::PHASES[ph].to_string(),
            };
            self.opening_name = Some((label, 0.0));
        } else if let Some(name) = ai::book().names.get(&book::key(&self.pos)) {
            if self.opening_name.as_ref().is_none_or(|(n, _)| n != name) {
                self.opening_name = Some((name.clone(), 0.0));
            }
        }
        let dest = a.slides[0].to;
        if let Some((p, at)) = a.ghost {
            let col = if p.color.is_white() { Color::new(0.98, 0.95, 0.88, 1.0) } else { Color::new(0.15, 0.15, 0.2, 1.0) };
            self.burst(at, col, 30, 260.0, false);
            self.burst(at, ORANGE, 10, 180.0, false);
            if let Some(g) = &mut self.grass {
                g.blast(at + vec2(0.0, SQ * 0.3), 1.0);
            }
            self.shake += 7.0;
            self.punch += 0.035;
        } else {
            self.burst(dest, Color::new(1.0, 1.0, 1.0, 0.5), 6, 70.0, false);
        }
        if let Some(t) = &a.tag {
            self.toast(t, dest - vec2(0.0, 40.0), elo_color(self.elo));
        }
        if self.pos.is_checkmate() {
            let k = self.pos.board().king_of(self.pos.turn()).map(|k| self.sq_pos(k)).unwrap_or(dest);
            self.toast("Checkmate!", k, RED);
            self.shake += 14.0;
            self.punch += 0.08;
            if self.pos.turn() != self.player {
                self.confetti();
            }
        } else if self.pos.is_check() {
            let k = self.pos.board().king_of(self.pos.turn()).map(|k| self.sq_pos(k)).unwrap_or(dest);
            self.toast("Check!", k, Color::new(1.0, 0.3, 0.3, 1.0));
            self.shake += 4.0;
        }
    }

    fn update(&mut self) {
        let dt = get_frame_time().min(0.05);
        let sw = screen_width();
        let mouse: Vec2 = mouse_position().into();
        let wm = self.camera(false).screen_to_world(mouse);
        let sh = screen_height();
        let on_board_side = mouse.x < sw - self.panel_w() && mouse.y < sh - self.strip_h();
        let wheel = mouse_wheel().1;

        // --- ELO slider ---
        let bar = self.bar_rect();
        let grab = Rect::new(bar.x - 40.0, bar.y - 30.0, bar.w + 80.0, bar.h + 60.0);
        if is_mouse_button_pressed(MouseButton::Left) && grab.contains(mouse) && self.overlay.is_none() {
            self.elo_drag = true;
        }
        if !is_mouse_button_down(MouseButton::Left) {
            self.elo_drag = false;
        }
        if self.elo_drag {
            self.elo = frac_elo((bar.y + bar.h - mouse.y) / bar.h);
        }
        if wheel != 0.0 && !on_board_side {
            self.elo = (self.elo + wheel.signum() * 25.0).clamp(1.0, MAX_ELO);
        }
        if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Down) {
            let d = if is_key_pressed(KeyCode::Up) { 100.0 } else { -100.0 };
            self.elo = (self.elo + d).clamp(1.0, MAX_ELO);
        }
        let prev = self.elo_disp;
        let acc = 240.0 * (self.elo - self.elo_disp) - 15.0 * self.elo_vel;
        self.elo_vel += acc * dt;
        self.elo_disp += self.elo_vel * dt;
        if (self.elo_disp / 100.0).floor() != (prev / 100.0).floor() {
            self.tick_pop = 1.0;
        }
        let t = tier(self.elo_disp.clamp(1.0, MAX_ELO));
        if t != self.tier {
            let up = t > self.tier;
            self.tier = t;
            self.tier_pop = 1.0;
            let at = if self.overlay.is_some() { vec2(150.0, sh - STRIP + 20.0) } else { vec2(bar.center().x, self.knob_y()) };
            let n = if up { 20 + t * 8 } else { 12 };
            self.burst(at, elo_color(self.elo_disp), n, 220.0 + t as f32 * 40.0, true);
        }
        if self.tier >= MAGNUS_TIER && self.overlay.is_none() && gen_range(0.0, 1.0) < 0.5 {
            let at = vec2(bar.center().x + gen_range(-30.0, 30.0), bar.y - 30.0 + gen_range(-20.0, 20.0));
            self.burst(at, GOLD, 1, 60.0, true);
        }
        self.tier_pop = (self.tier_pop - dt * 2.5).max(0.0);
        self.tick_pop = (self.tick_pop - dt * 6.0).max(0.0);

        // --- camera: wheel zooms toward cursor, right/middle drag pans, R resets ---
        if on_board_side && wheel != 0.0 {
            let old = self.zoom_to;
            self.zoom_to = (old * if wheel > 0.0 { 1.18 } else { 1.0 / 1.18 }).clamp(1.0, 4.0);
            self.center_to = wm - (wm - self.center_to) * (old / self.zoom_to);
        }
        if is_mouse_button_down(MouseButton::Right) || is_mouse_button_down(MouseButton::Middle) {
            if let Some(l) = self.pan_last {
                self.center_to += (l - mouse) / self.scale();
            }
            self.pan_last = Some(mouse);
        } else {
            self.pan_last = None;
        }
        if is_key_pressed(KeyCode::R) {
            self.zoom_to = 1.0;
        }
        if self.zoom_to <= 1.001 {
            self.center_to = vec2(BOARD / 2.0, BOARD / 2.0);
        }
        self.center_to = self.center_to.clamp(Vec2::ZERO, Vec2::splat(BOARD));
        let k = 1.0 - (-dt * 12.0).exp();
        self.zoom += (self.zoom_to - self.zoom) * k;
        self.center += (self.center_to - self.center) * k;
        self.shake *= (-dt * 9.0).exp();
        self.punch *= (-dt * 7.0).exp();

        // --- grass: cursor, dragged and sliding pieces brush through it ---
        if self.grass.is_some() {
            let mut pushers = vec![];
            if on_board_side {
                pushers.push((wm, 30.0));
            }
            if self.dragging {
                pushers.push((wm, 42.0));
            }
            if let Some(a) = &self.anim {
                let e = 1.0 - (1.0 - a.t.clamp(0.0, 1.0)).powi(3);
                pushers.extend(a.slides.iter().map(|s| (s.from.lerp(s.to, e), 46.0)));
            }
            let moving: Vec<Square> = self.anim.iter().flat_map(|a| a.slides.iter().map(|s| s.dest)).collect();
            let resting: Vec<Vec2> = self
                .pos
                .board()
                .iter()
                .filter(|(sq, _)| !moving.contains(sq))
                .map(|(sq, _)| self.sq_pos(sq) + vec2(0.0, SQ * 0.36))
                .collect();
            self.grass.as_mut().unwrap().update(dt, get_time() as f32, &pushers, &resting);
        }

        // --- animations / fx ---
        if let Some(a) = &mut self.anim {
            a.t += dt / if a.fx { 0.26 } else { 0.14 };
            if a.t >= 1.0 {
                let a = self.anim.take().unwrap();
                self.landed(a);
            }
        }
        for p in &mut self.parts {
            p.v.y += p.g * dt;
            p.v *= if p.g > 0.0 { 1.0 } else { (-dt * 3.0).exp() };
            p.p += p.v * dt;
            p.life -= dt;
        }
        self.parts.retain(|p| p.life > 0.0);
        for t in &mut self.toasts {
            t.t += dt;
        }
        self.toasts.retain(|t| t.t < 1.6);
        self.sel_t += dt;
        if let Some(o) = &mut self.opening_name {
            o.1 += dt;
        }
        if let Some(b) = &mut self.badge {
            b.2 += dt;
        }
        let over = self.over();
        if self.tc > 0 && !over && !self.history.is_empty() {
            self.clock[self.pos.turn() as usize] -= dt;
        }
        if over && self.anim.is_none() {
            self.over_t += dt;
        }

        if self.editor.is_some() {
            self.update_editor(on_board_side, wm);
            return;
        }

        // --- player input ---
        let hover = if on_board_side { self.world_sq(wm) } else { None };
        let my_turn = self.pos.turn() == self.player && !over && self.anim.is_none();
        for (i, h) in self.hover_amt.iter_mut().enumerate() {
            let on = my_turn && hover.map(|s| s as usize) == Some(i) && !self.dragging;
            *h += ((on as u8 as f32) - *h) * (1.0 - (-dt * 18.0).exp());
        }
        if my_turn && !self.elo_drag {
            if is_mouse_button_pressed(MouseButton::Left) && on_board_side {
                if let Some(m) = self.sel.zip(hover).and_then(|(s, h)| self.find_move(s, h)) {
                    self.play(m, None, None);
                } else if let Some(h) = hover.filter(|&h| self.pos.board().piece_at(h).is_some_and(|p| p.color == self.player)) {
                    if self.sel != Some(h) {
                        self.sel_t = 0.0;
                    }
                    self.sel = Some(h);
                    self.dragging = true;
                } else {
                    self.sel = None;
                }
            }
            if is_mouse_button_released(MouseButton::Left) && self.dragging {
                self.dragging = false;
                let s = self.sel.unwrap();
                match hover.filter(|&h| h != s).and_then(|h| self.find_move(s, h)) {
                    Some(m) => self.play(m, Some(wm), None),
                    None if hover != Some(s) => {
                        // snap back
                        let piece = self.pos.board().piece_at(s).unwrap();
                        let slide = Slide { piece, from: wm, to: self.sq_pos(s), dest: s };
                        self.anim = Some(Anim { slides: vec![slide], t: 0.0, ghost: None, fx: false, tag: None });
                    }
                    None => {}
                }
            }
        }

        // --- AI ---
        if !over && self.pos.turn() != self.player && self.anim.is_none() && !self.waiting && self.pending.is_none() {
            if let Some(m) = self.line_move() {
                self.pending = Some((m, None, get_time() + gen_range(0.4, 1.0)));
            }
        }
        if !over && self.pos.turn() != self.player && self.anim.is_none() && !self.waiting && self.pending.is_none() {
            let clock = (self.tc > 0).then(|| self.clock[!self.player as usize]);
            let req = ai::Req::Think { game_id: self.game_id, pos: self.pos.clone(), elo: self.elo, clock };
            if self.engine.tx.send(req).is_ok() {
                self.waiting = true;
                self.think_start = get_time();
            }
        }
        // grade-in-advance: analyse every legal move while the player is thinking
        let key = (self.game_id, self.history.len());
        if !over && self.pos.turn() == self.player && self.analysed != Some(key) {
            self.analysed = Some(key);
            let pop = ai::book().all(book::key(&self.pos));
            let total: u32 = pop.iter().map(|b| b.count).sum();
            if let Some(top) = pop.first().filter(|t| total >= 100 && t.count * 4 >= total && t.score() < 0.4) {
                let msg = format!("Trap! {:.0}% of players go wrong here", 100.0 * top.count as f32 / total as f32);
                self.toast(&msg, vec2(BOARD / 2.0, BOARD / 2.0), Color::new(1.0, 0.5, 0.3, 1.0));
            }
            let _ = self.engine.tx.send(ai::Req::Analyse { game_id: key.0, ply: key.1, pos: self.pos.clone() });
        }
        while let Ok(r) = self.engine.rx.try_recv() {
            match r {
                ai::Reply::Move { game_id, mv, tag, mood, real } if game_id == self.game_id => {
                    if real.is_some() {
                        self.real_game = real;
                    }
                    self.waiting = false;
                    if mood != self.mood && mood != ai::Mood::Calm {
                        if let Some(k) = self.pos.board().king_of(!self.player) {
                            let text = match mood {
                                ai::Mood::Attacking => "Going for your king!",
                                ai::Mood::Defending => "Bunkering down...",
                                _ => "Time trouble!",
                            };
                            self.toast(text, self.sq_pos(k), mood_color(mood));
                        }
                    }
                    self.mood = mood;
                    if let Some(m) = mv {
                        // humans don't move instantly; on a clock they budget, and panic when low
                        let think = match self.tc {
                            0 => gen_range(0.35, 1.2),
                            _ => (self.clock[!self.player as usize] / 40.0 * gen_range(0.3, 1.5)).clamp(0.15, 8.0),
                        };
                        self.pending = Some((m, tag, self.think_start + think as f64));
                    }
                }
                ai::Reply::Analysis { game_id, ply, lines } if game_id == self.game_id => {
                    self.analysis = Some((ply, lines));
                    self.try_grade();
                }
                _ => {}
            }
        }
        if let Some((m, tag, at)) = self.pending.clone() {
            if get_time() >= at && self.anim.is_none() {
                self.pending = None;
                self.play(m, None, tag);
            }
        }

        // --- overlay: drag window by the grip, tell the OS which pixels are clickable ---
        if self.overlay.is_some() {
            let win: Vec2 = { let p = miniquad::window::get_window_position(); vec2(p.0 as f32, p.1 as f32) };
            if is_mouse_button_pressed(MouseButton::Left) && Rect::new(0.0, sh - STRIP, 46.0, 46.0).contains(mouse) {
                self.win_drag = Some((win + mouse, win));
            }
            if !is_mouse_button_down(MouseButton::Left) {
                self.win_drag = None;
            }
            if let Some((grab, start)) = self.win_drag {
                let p = (start + win + mouse - grab).max(Vec2::ZERO);
                miniquad::window::set_window_position(p.x as u32, p.y as u32);
            }
            let hit = if is_mouse_button_down(MouseButton::Left) || self.dragging {
                vec![Rect::new(0.0, 0.0, sw, sh)]
            } else {
                let cam = self.camera(false);
                let mut squares: Vec<Square> = vec![];
                if my_turn {
                    squares.extend(self.pos.board().by_color(self.player));
                    if let Some(s) = self.sel {
                        squares.extend(self.pos.legal_moves().iter().filter(|m| m.from() == Some(s)).map(ui_to));
                    }
                }
                let mut hit: Vec<Rect> = squares
                    .into_iter()
                    .map(|s| {
                        let c = self.sq_pos(s);
                        let a = cam.world_to_screen(c - Vec2::splat(SQ / 2.0));
                        let b = cam.world_to_screen(c + Vec2::splat(SQ / 2.0));
                        Rect::new(a.x, a.y, b.x - a.x, b.y - a.y)
                    })
                    .collect();
                hit.push(Rect::new(0.0, sh - STRIP, sw, STRIP));
                hit
            };
            self.overlay.as_mut().unwrap().update(&hit);
        }
    }

    fn update_editor(&mut self, on_board_side: bool, wm: Vec2) {
        let sq = if on_board_side { self.world_sq(wm) } else { None };
        let ed = self.editor.as_mut().unwrap();
        if let (Some(sq), true) = (sq, is_mouse_button_pressed(MouseButton::Left)) {
            match ed.brush {
                Some(p) if ed.board.piece_at(sq) != Some(p) => ed.board.set_piece_at(sq, p),
                _ => ed.board.discard_piece_at(sq),
            }
            ed.err = None;
        }
    }

    fn draw_editor_world(&self, wm: Vec2) {
        let ed = self.editor.as_ref().unwrap();
        skins::draw_base(self.skin, self.skin_tex.as_ref(), 1.0, get_time() as f32);
        if let Some(g) = &self.grass {
            g.draw(0, g.len(), 1.0);
        }
        self.draw_coords();
        for (sq, p) in ed.board.iter() {
            self.draw_piece(p, self.sq_pos(sq), 1.0, 1.0);
        }
        if let Some(sq) = self.world_sq(wm) {
            let c = self.sq_pos(sq);
            draw_rectangle_lines(c.x - SQ / 2.0, c.y - SQ / 2.0, SQ, SQ, 4.0, Color::new(0.3, 0.8, 1.0, 0.8));
            if let Some(p) = ed.brush {
                self.draw_piece(p, c, 0.9, 0.45);
            }
        }
    }

    fn draw_editor_panel(&mut self) {
        let (sw, sh) = (screen_width(), screen_height());
        let x0 = sw - PANEL;
        draw_rectangle(x0, 0.0, PANEL, sh, Color::from_rgba(28, 28, 36, 255));
        self.text("SET UP A POSITION", x0 + 30.0, 44.0, 18.0, WHITE, true);
        self.text("pick a piece, click squares (click again to remove)", x0 + 30.0, 68.0, 12.0, GRAY, false);
        let roles = [Role::King, Role::Queen, Role::Rook, Role::Bishop, Role::Knight, Role::Pawn];
        let mut pick = None;
        for (row, color) in [Side::White, Side::Black].into_iter().enumerate() {
            for (i, role) in roles.into_iter().enumerate() {
                let r = Rect::new(x0 + 30.0 + i as f32 * 47.0, 90.0 + row as f32 * 52.0, 44.0, 48.0);
                let p = Piece { color, role };
                let on = self.editor.as_ref().unwrap().brush == Some(p);
                draw_rectangle(r.x, r.y, r.w, r.h, if on { Color::from_rgba(80, 110, 160, 255) } else { Color::from_rgba(50, 50, 64, 255) });
                self.draw_piece(p, r.center(), 0.62, 1.0);
                if r.contains(mouse_position().into()) && is_mouse_button_pressed(MouseButton::Left) {
                    pick = Some(Some(p));
                }
            }
        }
        let bw = PANEL - 60.0;
        let half = bw / 2.0 - 5.0;
        let row = |i: f32| 204.0 + i * 54.0;
        if self.button(Rect::new(x0 + 30.0, row(0.0), bw, 44.0), "Eraser") {
            pick = Some(None);
        }
        let turn = self.editor.as_ref().unwrap().turn;
        if self.button(Rect::new(x0 + 30.0, row(1.0), bw, 44.0), &format!("To move: {}", if turn == Side::White { "White" } else { "Black" })) {
            let ed = self.editor.as_mut().unwrap();
            ed.turn = !ed.turn;
        }
        if self.button(Rect::new(x0 + 30.0, row(2.0), half, 44.0), "Paste FEN") {
            let ed = self.editor.as_mut().unwrap();
            let text = miniquad::window::clipboard_get().unwrap_or_default();
            match text.trim().parse::<shakmaty::fen::Fen>() {
                Ok(f) => {
                    let setup = f.into_setup();
                    ed.board = setup.board;
                    ed.turn = setup.turn;
                    ed.err = None;
                }
                Err(e) => ed.err = Some(format!("Clipboard isn't a FEN ({e})")),
            }
        }
        if self.button(Rect::new(x0 + 35.0 + half, row(2.0), half, 44.0), "Clear") {
            self.editor.as_mut().unwrap().board = shakmaty::Board::empty();
        }
        if self.button(Rect::new(x0 + 30.0, row(3.0), bw, 44.0), "Starting position") {
            let ed = self.editor.as_mut().unwrap();
            ed.board = shakmaty::Board::new();
            ed.turn = Side::White;
        }
        if self.button(Rect::new(x0 + 30.0, row(4.0), half, 44.0), "Play White") {
            self.editor_play(Side::White);
        }
        if self.button(Rect::new(x0 + 35.0 + half, row(4.0), half, 44.0), "Play Black") {
            self.editor_play(Side::Black);
        }
        if self.button(Rect::new(x0 + 30.0, row(5.0), bw, 44.0), "Cancel") {
            self.editor = None;
            return;
        }
        if let Some(ed) = &mut self.editor {
            if let Some(b) = pick {
                ed.brush = b;
            }
            if let Some(e) = ed.err.clone() {
                for (i, chunk) in e.as_bytes().chunks(40).enumerate() {
                    self.text(&String::from_utf8_lossy(chunk), x0 + 30.0, row(6.0) + 10.0 + i as f32 * 16.0, 13.0, Color::new(1.0, 0.45, 0.4, 1.0), false);
                }
            }
        }
    }

    // ---------------- drawing ----------------

    fn draw_piece(&self, p: Piece, c: Vec2, scale: f32, alpha: f32) {
        let alpha = alpha * self.board_alpha();
        let tex = &self.piece_tex[skins::piece_index(p.color.is_white(), p.role)];
        let w = SQ * 1.04 * scale;
        let h = w * tex.height() / tex.width();
        // in the renders the piece's base is at 90% of the image height, centred a touch left
        let foot = c.y + SQ * 0.36 * scale;
        let (left, top) = (c.x - w * 0.54, foot - h * 0.9);
        let dest = |w: f32, h: f32| DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() };
        if let Some(g) = skins::glow(self.skin, p.color.is_white()) {
            let pulse = 0.8 + 0.2 * (get_time() as f32 * 3.0).sin();
            for grow in [14.0, 7.0] {
                draw_texture_ex(tex, left - grow / 2.0, top - grow / 2.0, with_a(g, 0.28 * alpha * pulse), dest(w + grow, h + grow));
            }
        }
        draw_texture_ex(tex, left, top, with_a(WHITE, alpha), dest(w, h));
    }

    fn text(&self, s: &str, x: f32, y: f32, size: f32, col: Color, bold: bool) -> TextDimensions {
        let font = if bold { &self.bold } else { &self.font };
        draw_text_ex(s, x, y, TextParams { font: Some(font), font_size: 64, font_scale: size / 64.0, color: col, ..Default::default() })
    }

    fn text_c(&self, s: &str, c: Vec2, size: f32, col: Color, bold: bool) {
        let font = if bold { &self.bold } else { &self.font };
        let d = measure_text(s, Some(font), 64, size / 64.0);
        self.text(s, c.x - d.width / 2.0, c.y + d.offset_y - d.height / 2.0, size, col, bold);
    }

    fn draw_board(&self) {
        let time = get_time() as f32;
        let ba = self.board_alpha();
        if self.overlay.is_none() {
            draw_rectangle(-10.0, -2.0, BOARD + 26.0, BOARD + 26.0, Color::new(0.0, 0.0, 0.0, 0.35));
        }
        skins::draw_base(self.skin, self.skin_tex.as_ref(), ba, time);
        for i in 0..64u32 {
            let c = self.sq_pos(Square::new(i));
            let h = self.hover_amt[i as usize];
            if h > 0.01 {
                draw_rectangle(c.x - SQ / 2.0, c.y - SQ / 2.0, SQ, SQ, Color::new(1.0, 1.0, 1.0, 0.18 * h));
            }
        }
        if let Some((a, b)) = self.last {
            for s in [a, b] {
                let c = self.sq_pos(s);
                draw_rectangle(c.x - SQ / 2.0, c.y - SQ / 2.0, SQ, SQ, Color::new(1.0, 0.85, 0.2, 0.35));
            }
        }
        if let Some(s) = self.sel {
            let c = self.sq_pos(s);
            draw_rectangle(c.x - SQ / 2.0, c.y - SQ / 2.0, SQ, SQ, Color::new(0.3, 0.7, 1.0, 0.4));
        }
        if self.pos.is_check() && self.anim.is_none() {
            if let Some(k) = self.pos.board().king_of(self.pos.turn()) {
                let c = self.sq_pos(k);
                let pulse = 0.8 + 0.2 * (time * 8.0).sin();
                for i in 0..7 {
                    draw_circle(c.x, c.y, SQ * 0.62 * pulse * (1.0 - i as f32 / 8.0), Color::new(1.0, 0.1, 0.1, 0.12));
                }
            }
        }
        self.draw_coords();
    }

    fn draw_coords(&self) {
        for i in 0..8u32 {
            let f = Square::from_coords(File::new(i), if self.player == Side::White { Rank::First } else { Rank::Eighth });
            let r = Square::from_coords(if self.player == Side::White { File::A } else { File::H }, Rank::new(i));
            let col = skins::coord_color(self.skin);
            let c = self.sq_pos(f);
            self.text(&f.file().char().to_string(), c.x + SQ * 0.32, c.y + SQ * 0.45, 13.0, col, true);
            let c = self.sq_pos(r);
            self.text(&r.rank().char().to_string(), c.x - SQ * 0.46, c.y - SQ * 0.3, 13.0, col, true);
        }
    }

    fn draw_world(&self, wm: Vec2) {
        if self.editor.is_some() {
            self.draw_editor_world(wm);
            return;
        }
        self.draw_board();
        let moving: Vec<Square> = self.anim.iter().flat_map(|a| a.slides.iter().map(|s| s.dest)).collect();
        // back to front: the renders are taller than a square and overlap the one behind
        let mut pieces: Vec<(Square, Piece)> = self.pos.board().iter().filter(|(sq, _)| !moving.contains(sq)).collect();
        pieces.sort_by(|a, b| self.sq_pos(a.0).y.total_cmp(&self.sq_pos(b.0).y));
        let mut drawn = 0; // grass blades drawn so far: pieces stand *in* the grass
        let ba = self.board_alpha();
        for (sq, p) in pieces {
            let dragged = self.dragging && self.sel == Some(sq);
            let h = self.hover_amt[sq as usize];
            let c = self.sq_pos(sq) - vec2(0.0, 4.0 * h);
            if let Some(g) = &self.grass {
                let upto = g.split(self.sq_pos(sq).y + SQ * 0.3);
                g.draw(drawn, upto.max(drawn), ba);
                drawn = upto.max(drawn);
            }
            self.draw_piece(p, c, 1.0 + 0.07 * h, if dragged { 0.3 } else { 1.0 });
        }
        if let Some(g) = &self.grass {
            g.draw(drawn, g.len(), ba);
            // keep last move / selection readable through the blades
            for s in self.last.iter().flat_map(|(a, b)| [*a, *b]).chain(self.sel) {
                let c = self.sq_pos(s);
                draw_rectangle_lines(c.x - SQ / 2.0 + 2.0, c.y - SQ / 2.0 + 2.0, SQ - 4.0, SQ - 4.0, 3.0, Color::new(1.0, 0.9, 0.3, 0.7));
            }
        }
        // legal move dots, popping in staggered by distance
        if let Some(s) = self.sel.filter(|_| self.anim.is_none()) {
            for m in self.pos.legal_moves().iter().filter(|m| m.from() == Some(s) && matches!(m.promotion(), None | Some(Role::Queen))) {
                let to = ui_to(m);
                let c = self.sq_pos(to);
                let delay = s.distance(to) as f32 * 0.035;
                let pop = ease_out_back(((self.sel_t - delay) * 6.0).clamp(0.0, 1.0));
                let col = Color::new(0.08, 0.1, 0.12, 0.28);
                if m.is_capture() {
                    draw_circle_lines(c.x, c.y, SQ * 0.44 * pop, 6.0, col);
                } else {
                    draw_circle(c.x, c.y, SQ * 0.15 * pop, col);
                }
            }
        }
        if let Some(a) = &self.anim {
            let t = a.t.clamp(0.0, 1.0);
            let e = 1.0 - (1.0 - t).powi(3);
            if let Some((p, at)) = a.ghost {
                self.draw_piece(p, at, 1.0 - 0.3 * t, 1.0 - t * t);
            }
            for s in &a.slides {
                let lift = if a.fx { (t * PI).sin() * 0.14 } else { 0.0 };
                self.draw_piece(s.piece, s.from.lerp(s.to, e) - vec2(0.0, lift * 30.0), 1.0 + lift, 1.0);
            }
        }
        if self.dragging {
            if let Some(p) = self.sel.and_then(|s| self.pos.board().piece_at(s)) {
                self.draw_piece(p, wm, 1.18, 1.0);
            }
        }
        if let Some(m) = self.line_move().filter(|_| self.pos.turn() == self.player && self.anim.is_none()) {
            // practice line: show the move you're drilling
            let pulse = 0.5 + 0.5 * (get_time() as f32 * 4.0).sin();
            for s in [m.from().unwrap(), ui_to(&m)] {
                let c = self.sq_pos(s);
                draw_rectangle_lines(c.x - SQ / 2.0 + 3.0, c.y - SQ / 2.0 + 3.0, SQ - 6.0, SQ - 6.0, 6.0, Color::new(0.3, 0.9, 0.5, 0.4 + 0.5 * pulse));
            }
        }
        if let Some((sq, g, t)) = self.badge {
            self.draw_badge(sq, g, t);
        }
        self.draw_particles(false);
        for t in &self.toasts {
            let pop = ease_out_back((t.t * 5.0).min(1.0));
            let a = (1.6 - t.t).min(0.4) / 0.4;
            let c = t.p - vec2(0.0, t.t * 30.0);
            for i in 0..8 {
                let o = vec2((i as f32 * PI / 4.0).cos(), (i as f32 * PI / 4.0).sin()) * 2.0;
                self.text_c(&t.text, c + o, 30.0 * pop, Color::new(0.0, 0.0, 0.0, 0.8 * a), true);
            }
            self.text_c(&t.text, c, 30.0 * pop, with_a(t.col, a), true);
        }
    }

    fn draw_badge(&self, sq: Square, g: Grade, t: f32) {
        let col = grade_color(g);
        let c = self.sq_pos(sq);
        if matches!(g, Grade::Blunder | Grade::Mistake) {
            let flash = (1.0 - t * 1.5).max(0.0);
            draw_rectangle(c.x - SQ / 2.0, c.y - SQ / 2.0, SQ, SQ, with_a(col, 0.55 * flash));
        }
        if matches!(g, Grade::Brilliant | Grade::Great) {
            // expanding shockwave rings
            for i in 0..3 {
                let k = (t * 1.4 - i as f32 * 0.18).clamp(0.0, 1.0);
                if k > 0.0 && k < 1.0 {
                    draw_circle_lines(c.x, c.y, SQ * (0.3 + 1.2 * k), 4.0 * (1.0 - k), with_a(col, 1.0 - k));
                }
            }
        }
        let pop = ease_out_back((t * 4.0).min(1.0));
        let wob = if g == Grade::Blunder { (t * 40.0).sin() * (-t * 4.0).exp() * 5.0 } else { 0.0 };
        let p = c + vec2(SQ * 0.36 + wob, -SQ * 0.36);
        let r = 15.0 * pop * (1.0 + 0.08 * (get_time() as f32 * 5.0).sin() * (g == Grade::Brilliant) as u8 as f32);
        draw_circle(p.x + 1.5, p.y + 2.5, r, Color::new(0.0, 0.0, 0.0, 0.35));
        draw_circle(p.x, p.y, r + 2.0, WHITE);
        draw_circle(p.x, p.y, r, col);
        if pop > 0.05 {
            let i = match g {
                Grade::Brilliant => 0,
                Grade::Great => 1,
                Grade::Book => 2,
                Grade::Best => 3,
                Grade::Excellent => 4,
                Grade::Good => 5,
                Grade::Inaccuracy => 6,
                Grade::Mistake => 7,
                Grade::Blunder => 8,
            };
            let s = r * 1.45;
            draw_texture_ex(&self.icons[i], p.x - s / 2.0, p.y - s / 2.0, WHITE, DrawTextureParams { dest_size: Some(vec2(s, s)), ..Default::default() });
        }
    }

    fn draw_particles(&self, screen: bool) {
        for p in self.parts.iter().filter(|p| p.screen == screen) {
            let k = p.life / p.max;
            if p.g > 0.0 {
                draw_poly(p.p.x, p.p.y, 4, p.size, p.life * 400.0, with_a(p.col, k.min(1.0)));
            } else {
                draw_circle(p.p.x, p.p.y, p.size * k, with_a(p.col, p.col.a * k));
            }
        }
    }

    fn button(&self, r: Rect, label: &str) -> bool {
        let m: Vec2 = mouse_position().into();
        let hov = r.contains(m);
        let down = hov && is_mouse_button_down(MouseButton::Left);
        let off = if down { 2.0 } else { 0.0 };
        let col = if hov { Color::from_rgba(70, 70, 90, 255) } else { Color::from_rgba(50, 50, 64, 255) };
        draw_rectangle(r.x, r.y + 3.0, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.3));
        draw_rectangle(r.x, r.y + off, r.w, r.h, col);
        let w = measure_text(label, Some(&self.bold), 64, 18.0 / 64.0).width;
        self.text_c(label, r.center() + vec2(0.0, off), 18.0 * ((r.w - 16.0) / w).min(1.0), WHITE, true);
        hov && is_mouse_button_pressed(MouseButton::Left)
    }

    fn draw_panel(&mut self) {
        let (sw, sh) = (screen_width(), screen_height());
        let time = get_time() as f32;
        let x0 = sw - PANEL;
        draw_rectangle(x0, 0.0, PANEL, sh, Color::from_rgba(28, 28, 36, 255));
        draw_rectangle(x0, 0.0, 2.0, sh, Color::from_rgba(50, 50, 64, 255));

        let e = self.elo_disp.clamp(1.0, MAX_ELO);
        let col = elo_color(e);
        let magnus = self.tier >= MAGNUS_TIER;
        let d = self.text("OPPONENT", x0 + 30.0, 44.0, 16.0, GRAY, true);
        let mood = format!("{:?}", self.mood).to_uppercase();
        self.text(&format!("·  {mood}"), x0 + 40.0 + d.width, 44.0, 16.0, mood_color(self.mood), true);
        let pop = 1.0 + 0.35 * ease_out_back(self.tier_pop) * self.tier_pop + 0.06 * self.tick_pop;
        let num_col = if self.tier == MAGNUS_TIER { Color::new(1.0, 0.8 + 0.15 * (time * 6.0).sin(), 0.25, 1.0) } else { col };
        let d = self.text(&format!("{:.0}", e), x0 + 30.0, 108.0, 60.0 * pop, num_col, true);
        self.text("ELO", x0 + 40.0 + d.width, 108.0, 18.0, GRAY, true);
        self.text(TIERS[self.tier].1, x0 + 30.0, 145.0, 24.0 * (1.0 + 0.2 * self.tier_pop), col, true);

        // the bar
        let b = self.bar_rect();
        let r = b.w / 2.0;
        let track = Color::from_rgba(45, 45, 58, 255);
        draw_rectangle(b.x, b.y, b.w, b.h, track);
        draw_circle(b.x + r, b.y, r, track);
        draw_circle(b.x + r, b.y + b.h, r, track);
        let ky = self.knob_y().clamp(b.y, b.y + b.h);
        draw_rectangle(b.x - 6.0, ky, b.w + 12.0, b.y + b.h - ky, with_a(col, 0.12 + 0.08 * self.tier_pop));
        draw_circle(b.x + r, b.y + b.h, r, elo_color(1.0));
        let n = 90;
        for i in 0..n {
            let y1 = b.y + b.h * (1.0 - i as f32 / n as f32);
            let y0 = b.y + b.h * (1.0 - (i + 1) as f32 / n as f32);
            if y1 <= ky {
                break;
            }
            let c = elo_color(frac_elo(i as f32 / n as f32));
            let shim = 0.18 * ((time * 3.0 + y0 * 0.04).sin()).max(0.0).powi(4);
            let c = Color::new((c.r + shim).min(1.0), (c.g + shim).min(1.0), (c.b + shim).min(1.0), 1.0);
            draw_rectangle(b.x, y0.max(ky), b.w, y1 - y0.max(ky), c);
        }
        // tier ladder
        let mut label_y = f32::MAX;
        for (i, t) in TIERS.iter().enumerate().skip(1) {
            let y = b.y + b.h * (1.0 - elo_frac(t.0));
            let reached = e >= t.0;
            let c = if reached { elo_color(t.0) } else { Color::from_rgba(90, 90, 105, 255) };
            draw_line(b.x + b.w + 6.0, y, b.x + b.w + 18.0, y, 2.0, c);
            // keep labels from overlapping where tiers are close together
            label_y = y.min(label_y - 17.0);
            if label_y != y {
                draw_line(b.x + b.w + 18.0, y, b.x + b.w + 24.0, label_y, 1.0, with_a(c, 0.5));
            }
            let wob = if i == self.tier { 4.0 * self.tier_pop } else { 0.0 };
            self.text(t.1, b.x + b.w + 26.0 + wob, label_y + 5.0, if i == self.tier { 16.0 } else { 13.0 }, c, i == self.tier);
            self.text(&format!("{:.0}", t.0), b.x + b.w + 212.0, label_y + 5.0, 12.0, with_a(c, 0.6), false);
        }
        // crown
        let crown = vec2(b.x + r, b.y - 42.0);
        if magnus {
            for i in 0..6 {
                draw_circle(crown.x, crown.y, 34.0 * (1.0 - i as f32 / 7.0) * (0.9 + 0.1 * (time * 5.0).sin()), Color::new(1.0, 0.8, 0.2, 0.08));
            }
        }
        let cc = if self.tier == MAGNUS_TIER { GOLD } else if magnus { col } else { Color::from_rgba(80, 80, 95, 255) };
        self.draw_crown(crown, 1.0 + 0.25 * self.tier_pop * magnus as u8 as f32, cc);

        // knob: squash/stretch with velocity, pops on every 100 and harder on tier change
        let speed = (self.elo_vel.abs() / 4000.0).min(0.5);
        let m: Vec2 = mouse_position().into();
        let near = (m - vec2(b.x + r, self.knob_y())).length() < 40.0 || self.elo_drag;
        let kr = 19.0 * (1.0 + 0.18 * self.tick_pop + 0.4 * self.tier_pop + if near { 0.12 } else { 0.0 });
        let kp = vec2(b.x + r, self.knob_y());
        for i in 0..5 {
            draw_circle(kp.x, kp.y, kr * (1.6 - i as f32 * 0.12), with_a(col, 0.06));
        }
        draw_ellipse(kp.x, kp.y, kr * (1.0 - speed * 0.4), kr * (1.0 + speed), 0.0, WHITE);
        draw_ellipse(kp.x, kp.y, kr * 0.62 * (1.0 - speed * 0.4), kr * 0.62 * (1.0 + speed), 0.0, col);
        self.draw_particles(true);

        // buttons
        let bw = PANEL - 60.0;
        let by = sh - 210.0;
        if self.button(Rect::new(x0 + 30.0, by - 108.0, bw, 44.0), &format!("Practice: {}", OPENINGS[self.opening].0)) {
            self.opening = (self.opening + 1) % OPENINGS.len();
            self.new_game(self.player);
        }
        if self.button(Rect::new(x0 + 30.0, by - 54.0, bw / 2.0 - 5.0, 44.0), &format!("Clock: {}", TCS[self.tc].0)) {
            self.tc = (self.tc + 1) % TCS.len();
            self.new_game(self.player);
        }
        if self.button(Rect::new(x0 + 35.0 + bw / 2.0, by - 54.0, bw / 2.0 - 5.0, 44.0), &format!("Skin: {}", skins::SKINS[self.skin])) {
            self.set_skin(self.skin + 1);
        }
        if self.button(Rect::new(x0 + 30.0, by, bw / 2.0 - 5.0, 44.0), "Play White") {
            self.new_game(Side::White);
        }
        if self.button(Rect::new(x0 + 35.0 + bw / 2.0, by, bw / 2.0 - 5.0, 44.0), "Play Black") {
            self.new_game(Side::Black);
        }
        let third = (bw - 10.0) / 3.0;
        if self.button(Rect::new(x0 + 30.0, by + 54.0, third, 44.0), "Undo") {
            self.undo();
        }
        if self.button(Rect::new(x0 + 35.0 + third, by + 54.0, third, 44.0), "Overlay") {
            self.relaunch(true);
        }
        if self.button(Rect::new(x0 + 40.0 + 2.0 * third, by + 54.0, third, 44.0), "Position") {
            self.editor = Some(Editor { board: self.pos.board().clone(), turn: self.pos.turn(), brush: Some(Piece { color: Side::White, role: Role::Queen }), err: None });
        }
        let status = if self.over() {
            match self.winner() {
                Some(w) if w == self.player => "You won!".to_string(),
                Some(_) => "You lost.".to_string(),
                None => "Draw.".to_string(),
            }
        } else if self.pos.turn() == self.player {
            "Your move".to_string()
        } else {
            format!("Thinking{}", ".".repeat((time * 3.0) as usize % 4))
        };
        self.text(&status, x0 + 30.0, by + 140.0, 22.0, WHITE, true);
        self.text("wheel: zoom   right-drag: pan   R: reset", x0 + 30.0, sh - 30.0, 13.0, GRAY, false);
        let games = ai::book().games as f32 / 1e6;
        self.text(&format!("scroll here: ELO · learned from {games:.1}M Lichess games"), x0 + 30.0, sh - 12.0, 12.0, GRAY, false);
    }

    fn draw_crown(&self, c: Vec2, s: f32, col: Color) {
        let w = 26.0 * s;
        let h = 18.0 * s;
        let base = c.y + h / 2.0;
        let pts = [-w, -w * 0.5, 0.0, w * 0.5, w];
        for (i, x) in pts.iter().enumerate() {
            let tip = if i % 2 == 0 { c.y - h } else { c.y - h * 0.3 };
            draw_triangle(vec2(c.x + x, tip), vec2(c.x + x - w * 0.35, base), vec2(c.x + x + w * 0.35, base), col);
            if i % 2 == 0 {
                draw_circle(c.x + x, tip, 3.5 * s, col);
            }
        }
        draw_rectangle(c.x - w, base, w * 2.0, 6.0 * s, col);
    }

    fn draw_game_over(&self) {
        if !self.over() || self.over_t <= 0.0 {
            return;
        }
        let (sw, sh) = (screen_width(), screen_height());
        let a = (self.over_t * 3.0).min(1.0);
        let pop = ease_out_back((self.over_t * 3.0).min(1.0));
        let cx = (sw - self.panel_w()) / 2.0;
        let sh = sh - self.strip_h();
        draw_rectangle(0.0, sh / 2.0 - 60.0 * pop, sw - self.panel_w(), 120.0 * pop, Color::new(0.0, 0.0, 0.0, 0.55 * a));
        let on_time = self.flagged().is_some();
        let (title, sub) = match self.winner() {
            Some(w) if w == self.player && on_time => ("ON TIME!", format!("The {:.0} ELO {} flagged", self.elo, TIERS[tier(self.elo)].1)),
            Some(_) if on_time => ("Flagged", "Out of time. Move faster!".to_string()),
            Some(w) if w == self.player => ("CHECKMATE!", format!("You beat the {:.0} ELO {}", self.elo, TIERS[tier(self.elo)].1)),
            Some(_) => ("Checkmated", format!("{} ({:.0}) got you. Undo?", TIERS[tier(self.elo)].1, self.elo)),
            None => ("Draw", "Nobody wins.".to_string()),
        };
        self.text_c(title, vec2(cx, sh / 2.0 - 14.0), 54.0 * pop, with_a(WHITE, a), true);
        self.text_c(&sub, vec2(cx, sh / 2.0 + 34.0), 20.0 * pop, with_a(LIGHTGRAY, a), false);
    }

    /// Overlay mode's compact controls: grip | - ELO + | status | undo | new | exit.
    /// Overlay controls, two rows: [grip  -  ELO  +   status] / [Undo  New  Skin  Exit].
    fn draw_strip(&mut self) {
        let (sw, sh) = (screen_width(), screen_height());
        let y = sh - STRIP;
        draw_rectangle(0.0, y, sw, STRIP, Color::new(0.11, 0.11, 0.14, 0.85));
        for i in 0..6 {
            draw_circle(16.0 + (i % 2) as f32 * 8.0, y + 14.0 + (i / 2) as f32 * 9.0, 2.2, GRAY); // grip
        }
        let e = self.elo_disp.clamp(1.0, MAX_ELO);
        let r1 = |x: f32, w: f32| Rect::new(x, y + 8.0, w, 34.0);
        if self.button(r1(52.0, 40.0), "-") {
            self.elo = (self.elo - 100.0).max(1.0);
        }
        let pop = 1.0 + 0.3 * self.tier_pop + 0.06 * self.tick_pop;
        self.text_c(&format!("{:.0}", e), vec2(150.0, y + 18.0), 24.0 * pop, elo_color(e), true);
        self.text_c(TIERS[self.tier].1, vec2(150.0, y + 38.0), 11.0, elo_color(e), true);
        if self.button(r1(208.0, 40.0), "+") {
            self.elo = (self.elo + 100.0).min(MAX_ELO);
        }
        let status = if self.over() {
            "Game over".to_string()
        } else if self.tc > 0 {
            format!("{}  vs  {}", Self::fmt_clock(self.clock[self.player as usize]), Self::fmt_clock(self.clock[!self.player as usize]))
        } else if self.pos.turn() == self.player {
            "Your move".to_string()
        } else {
            "Thinking…".to_string()
        };
        self.text_c(&status, vec2((268.0 + sw) / 2.0, y + 26.0), 16.0, LIGHTGRAY, true);
        let labels = ["Undo", "New", "Skin", "Exit"];
        let gap = 14.0;
        let w = (sw - 2.0 * gap - gap * (labels.len() - 1) as f32) / labels.len() as f32;
        for (i, label) in labels.into_iter().enumerate() {
            let r = Rect::new(gap + i as f32 * (w + gap), y + 52.0, w, 38.0);
            let text = if label == "Skin" { format!("Skin: {}", skins::SKINS[self.skin]) } else { label.to_string() };
            if self.button(r, &text) {
                match label {
                    "Undo" => self.undo(),
                    "New" => self.new_game(self.player),
                    "Skin" => self.set_skin(self.skin + 1),
                    _ => self.relaunch(false),
                }
            }
        }
        self.draw_particles(true);
    }

    fn draw_clocks(&self) {
        if self.tc == 0 || self.overlay.is_some() {
            return;
        }
        let time = get_time() as f32;
        let sh = screen_height();
        let x = (screen_width() - PANEL) / 2.0 + (sh * 0.86) / 2.0 - 130.0;
        for (side, y) in [(!self.player, 3.0), (self.player, sh - 37.0)] {
            let t = self.clock[side as usize];
            let active = self.pos.turn() == side && !self.over();
            let low = t < 20.0;
            let pulse = if low && active { 0.5 + 0.5 * (time * 10.0).sin() } else { 0.0 };
            let shake = if side != self.player && self.mood == ai::Mood::Hurrying { (time * 50.0).sin() * 2.0 } else { 0.0 };
            let bg = if active { Color::new(0.95, 0.95, 0.92, 1.0) } else { Color::new(0.2, 0.2, 0.25, 1.0) };
            let bg = if low && active { Color::new(1.0, 0.35 + 0.3 * (1.0 - pulse), 0.3 + 0.3 * (1.0 - pulse), 1.0) } else { bg };
            draw_rectangle(x + shake, y, 130.0, 34.0, bg);
            let fg = if active { BLACK } else { LIGHTGRAY };
            self.text_c(&Self::fmt_clock(t), vec2(x + 65.0 + shake, y + 17.0), 21.0 * (1.0 + 0.08 * pulse), fg, true);
        }
    }

    fn draw(&mut self) {
        clear_background(if self.overlay.is_some() { overlay::CLEAR } else { Color::from_rgba(20, 20, 26, 255) });
        let cam = self.camera(true);
        let wm = cam.screen_to_world(mouse_position().into());
        set_camera(&cam);
        self.draw_world(wm);
        set_default_camera();
        self.draw_clocks();
        if let (Some((name, t)), None) = (&self.opening_name, &self.overlay) {
            let k = (t * 3.0).min(1.0);
            self.text(name, 20.0 - 20.0 * (1.0 - k), 28.0, 17.0, with_a(Color::new(0.85, 0.75, 0.55, 1.0), k), true);
        }
        if let (Some((url, who)), None) = (&self.real_game, &self.overlay) {
            let label = format!("Bot is replaying a real game ({who}) · {} ↗", url.trim_start_matches("https://"));
            let d = measure_text(&label, Some(&self.font), 64, 14.0 / 64.0);
            let r = Rect::new(20.0, 38.0, d.width, 20.0);
            let hov = r.contains(mouse_position().into());
            self.text(&label, 20.0, 52.0, 14.0, if hov { Color::new(0.6, 0.85, 1.0, 1.0) } else { Color::new(0.45, 0.7, 0.95, 1.0) }, false);
            if hov && is_mouse_button_pressed(MouseButton::Left) {
                open_url(url);
            }
        }
        self.draw_game_over();
        if self.overlay.is_some() {
            self.draw_strip();
        } else if self.editor.is_some() {
            self.draw_editor_panel();
        } else {
            self.draw_panel();
        }
        self.save_settings();
    }
}

fn open_url(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

fn settings_path() -> Option<std::path::PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(std::path::PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from).or_else(|| Some(std::path::PathBuf::from(std::env::var_os("HOME")?).join(".config")))
    };
    Some(base?.join("funchess").join("settings"))
}

/// Saved value for `key`, unless given on the command line.
fn setting(key: &str) -> Option<String> {
    arg(&format!("--{key}")).or_else(|| {
        let text = std::fs::read_to_string(settings_path()?).ok()?;
        text.lines().find_map(|l| l.strip_prefix(&format!("{key}=")).map(str::to_string))
    })
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn overlay_mode() -> bool {
    std::env::args().any(|a| a == "--overlay")
}

fn conf() -> Conf {
    let mut c = Conf { window_title: "Fun Chess".into(), window_width: 1280, window_height: 820, sample_count: 4, ..Default::default() };
    c.icon = Some(miniquad::conf::Icon {
        small: *include_bytes!("../assets/icon16.rgba"),
        medium: *include_bytes!("../assets/icon32.rgba"),
        big: *include_bytes!("../assets/icon64.rgba"),
    });
    c.platform.linux_wm_class = "funchess"; // matches StartupWMClass in the .desktop launcher
    if overlay_mode() {
        c.window_title = overlay::TITLE.into();
        c.window_width = 560;
        c.window_height = 650;
        c.window_resizable = false;
        c.platform.framebuffer_alpha = true;
        c.platform.linux_wm_class = overlay::WM_CLASS;
    }
    c
}

#[macroquad::main(conf)]
async fn main() {
    let font = load_ttf_font_from_bytes(include_bytes!("../assets/DejaVuSans.ttf")).unwrap();
    let bold = load_ttf_font_from_bytes(include_bytes!("../assets/DejaVuSans-Bold.ttf")).unwrap();
    let engine = match ai::spawn() {
        Ok(e) => e,
        Err(e) => loop {
            clear_background(BLACK);
            draw_text_ex(&e, 20.0, 40.0, TextParams { font: Some(&font), font_size: 20, color: WHITE, ..Default::default() });
            next_frame().await;
        },
    };
    rand::srand(miniquad::date::now().to_bits());
    let pos = arg("--fen")
        .and_then(|f| f.parse::<shakmaty::fen::Fen>().ok()?.into_position(shakmaty::CastlingMode::Standard).ok())
        .unwrap_or_default();
    let player = if arg("--side").as_deref() == Some("b") { Side::Black } else { Side::White };
    let tc = setting("tc").and_then(|t| t.parse().ok()).filter(|&t: &usize| t < TCS.len()).unwrap_or(0);
    let clock = arg("--clock")
        .and_then(|c| {
            let (b, w) = c.split_once(',')?;
            Some([b.parse().ok()?, w.parse().ok()?])
        })
        .unwrap_or([TCS[tc].1; 2]);
    let elo = setting("elo").and_then(|e| e.parse().ok()).unwrap_or(1200.0f32).clamp(1.0, MAX_ELO);
    let mut app = App {
        pos,
        history: vec![],
        last: None,
        player,
        sel: None,
        sel_t: 0.0,
        dragging: false,
        hover_amt: [0.0; 64],
        anim: None,
        parts: vec![],
        toasts: vec![],
        zoom: 0.6,
        zoom_to: 1.0,
        center: vec2(BOARD / 2.0, BOARD / 2.0),
        center_to: vec2(BOARD / 2.0, BOARD / 2.0),
        shake: 0.0,
        punch: 0.0,
        pan_last: None,
        elo,
        elo_disp: 0.0,
        elo_vel: 0.0,
        elo_drag: false,
        tier: 0,
        tier_pop: 0.0,
        tick_pop: 0.0,
        game_id: 0,
        waiting: false,
        think_start: 0.0,
        pending: None,
        over_t: 0.0,
        engine,
        skin: 0,
        skin_tex: None,
        piece_tex: vec![],
        icons: [
            include_bytes!("../assets/grades/diamond.png").as_slice(),
            include_bytes!("../assets/grades/priority_high.png"),
            include_bytes!("../assets/grades/menu_book.png"),
            include_bytes!("../assets/grades/star.png"),
            include_bytes!("../assets/grades/thumb_up.png"),
            include_bytes!("../assets/grades/check.png"),
            include_bytes!("../assets/grades/trending_down.png"),
            include_bytes!("../assets/grades/question_mark.png"),
            include_bytes!("../assets/grades/dangerous.png"),
        ]
        .iter()
        .map(|b| skins::load(b))
        .collect(),
        grass: None,
        editor: None,
        saved: (0.0, usize::MAX, 0),
        played: vec![],
        opening: arg("--opening").and_then(|o| o.parse().ok()).filter(|&o: &usize| o < OPENINGS.len()).unwrap_or(0),
        opening_name: None,
        real_game: None,
        tc,
        clock,
        mood: ai::Mood::Calm,
        overlay: overlay_mode().then(overlay::Overlay::default),
        analysis: None,
        analysed: None,
        want_grade: None,
        badge: None,
        win_drag: None,
        font,
        bold,
    };
    app.set_skin(setting("skin").and_then(|s| s.parse().ok()).unwrap_or(0));
    app.saved = (app.elo.round(), app.skin, app.tc);
    loop {
        app.update();
        app.draw();
        next_frame().await;
    }
}
