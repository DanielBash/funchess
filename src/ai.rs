// Stockfish does the thinking; we decide which of its candidate moves a human of
// the chosen ELO would actually play. Low ELO = big noise + loves checks/captures/king hunts.
// The same process also grades the player's moves (analysis runs while they think).
use crate::book;
use macroquad::rand::gen_range;
use shakmaty::{fen::Fen, uci::UciMove, Chess, EnPassantMode, Move, Position, Role, Square};
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

pub enum Req {
    /// `clock` = bot's remaining seconds, if a time control is on.
    Think { game_id: u64, pos: Chess, elo: f32, clock: Option<f32> },
    Analyse { game_id: u64, ply: usize, pos: Chess },
}

pub enum Reply {
    /// `real`: the real Lichess game this move came from (url, description).
    Move { game_id: u64, mv: Option<Move>, tag: Option<String>, mood: Mood, real: Option<(String, String)> },
    Analysis { game_id: u64, ply: usize, lines: Vec<(Move, i32)> },
}

pub const MAGNUS: f32 = 2850.0;
/// Top of the slider: "Theoretical Human".
pub const MAX_ELO: f32 = 3500.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mood {
    Calm,
    Attacking,
    Defending,
    Hurrying,
}

pub struct Engine {
    pub tx: Sender<Req>,
    pub rx: Receiver<Reply>,
    stdin: Arc<Mutex<ChildStdin>>,
}

impl Engine {
    /// Cut the current search short; stockfish ignores this when idle.
    pub fn stop(&self) {
        let _ = writeln!(self.stdin.lock().unwrap(), "stop");
    }
}

pub fn spawn() -> Result<Engine, String> {
    let exe_dir = std::env::current_exe().ok().and_then(|p| Some(p.parent()?.to_path_buf())).unwrap_or_default();
    let name = if cfg!(windows) { "stockfish.exe" } else { "stockfish" };
    let mut child = [exe_dir.join(name), std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name), name.into()]
        .into_iter()
        .find_map(|p| {
            let mut c = Command::new(p);
            c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
            #[cfg(windows)]
            std::os::windows::process::CommandExt::creation_flags(&mut c, 0x0800_0000); // CREATE_NO_WINDOW
            c.spawn().ok()
        })
        .ok_or("Stockfish not found: put it next to the funchess executable")?;
    let stdin = Arc::new(Mutex::new(child.stdin.take().unwrap()));
    let mut out = BufReader::new(child.stdout.take().unwrap());
    send(&stdin, "uci\n");
    read_until(&mut out, "uciok");

    let (tx, req_rx) = channel::<Req>();
    let (rep_tx, rx) = channel();
    let inp = stdin.clone();
    std::thread::spawn(move || {
        let _child = child; // stockfish exits on stdin EOF when this thread ends
        let mut channel: Option<(u64, u32)> = None; // (our game, real game being replayed)
        for req in req_rx {
            let reply = match req {
                Req::Think { game_id, pos, elo, clock } => {
                    let mut real = channel.filter(|c| c.0 == game_id).map(|c| c.1);
                    let replay = real_game_move(&pos, elo, &mut real)
                        .filter(|(mv, _)| !is_blunder(&inp, &mut out, &pos, *mv)); // never ruin the game
                    match replay {
                        Some((mv, info)) => {
                            channel = real.map(|g| (game_id, g));
                            let tag = Some(format!("Real game move! ({})", info.1));
                            Reply::Move { game_id, mv: Some(mv), tag, mood: Mood::Calm, real: Some(info) }
                        }
                        None => {
                            channel = None;
                            let (mv, tag, mood) = think(&inp, &mut out, &pos, elo, clock);
                            Reply::Move { game_id, mv, tag, mood, real: None }
                        }
                    }
                }
                Req::Analyse { game_id, ply, pos } => {
                    // every legal move gets a score; the UI stops this the moment the player moves
                    let lines = search(&inp, &mut out, &pos, 256, "depth 16");
                    Reply::Analysis { game_id, ply, lines }
                }
            };
            if rep_tx.send(reply).is_err() {
                break;
            }
        }
    });
    Ok(Engine { tx, rx, stdin })
}

fn send(inp: &Mutex<ChildStdin>, s: &str) {
    let mut i = inp.lock().unwrap();
    let _ = i.write_all(s.as_bytes());
    let _ = i.flush();
}

fn read_until(out: &mut BufReader<ChildStdout>, prefix: &str) -> Vec<String> {
    let mut lines = vec![];
    let mut s = String::new();
    while out.read_line(&mut s).unwrap_or(0) > 0 {
        let done = s.starts_with(prefix);
        lines.push(std::mem::take(&mut s));
        if done {
            break;
        }
    }
    lines
}

/// (move, centipawns for the side to move), best first.
fn search(inp: &Mutex<ChildStdin>, out: &mut BufReader<ChildStdout>, pos: &Chess, multipv: usize, go: &str) -> Vec<(Move, i32)> {
    let fen = Fen::from_position(pos, EnPassantMode::Legal);
    send(inp, &format!("setoption name MultiPV value {multipv}\nposition fen {fen}\ngo {go}\n"));
    let mut lines: Vec<Option<(Move, i32)>> = vec![];
    for l in read_until(out, "bestmove") {
        let t: Vec<&str> = l.split_whitespace().collect();
        let at = |k: &str| t.iter().position(|x| *x == k).and_then(|i| t.get(i + 1));
        let (Some(k), Some(kind), Some(pv)) = (at("multipv"), at("score"), at("pv")) else { continue };
        let Some(v) = at(kind).and_then(|v| v.parse::<i32>().ok()) else { continue };
        let cp = if *kind == "mate" { if v > 0 { 30000 - v * 10 } else { -30000 - v * 10 } } else { v };
        let Ok(m) = pv.parse::<UciMove>().map_err(|_| ()).and_then(|u| u.to_move(pos).map_err(|_| ())) else { continue };
        let k: usize = k.parse().unwrap_or(1);
        if lines.len() < k {
            lines.resize(k, None);
        }
        lines[k - 1] = Some((m, cp));
    }
    let mut lines: Vec<_> = lines.into_iter().flatten().collect();
    lines.sort_by_key(|l| -l.1); // a stopped search can leave slots out of order
    lines
}

fn think(inp: &Mutex<ChildStdin>, out: &mut BufReader<ChildStdout>, pos: &Chess, e: f32, clock: Option<f32>) -> (Option<Move>, Option<String>, Mood) {
    let bk = book();
    let band = book::band(e);
    // 1. In known positions, choose among the moves real players of this rating make here,
    //    but not the average one: favour what actually wins at this level, judged one reply
    //    deeper by how players of this level usually answer (so bots steer into the traps
    //    people really fall for). Stronger bots exploit harder.
    if let Some(moves) = bk.moves(book::key(pos), band) {
        let total: u32 = moves.iter().map(|m| m.count).sum();
        if total >= 12 {
            let greed = 4.0 + e / 300.0;
            let options: Vec<(Move, f32, f32)> = moves
                .iter()
                .filter_map(|bm| {
                    let mv = book::decode(bm.mv, pos)?;
                    let mut after = pos.clone();
                    after.play_unchecked(mv);
                    let (s, n) = bk.practical(book::key(&after), band).unwrap_or((bm.score(), bm.count));
                    // small samples say little: shrink toward an even score
                    let s = 0.5 + (s - 0.5) * n as f32 / (n as f32 + 300.0);
                    let share = bm.count as f32 / total as f32;
                    Some((mv, share.powf(0.75) * (greed * (s - 0.5)).exp(), s))
                })
                .collect();
            let sum: f32 = options.iter().map(|o| o.1).sum();
            let mut r = gen_range(0.0, sum);
            if let Some(&(mv, _, s)) = options.iter().find(|o| r < o.1 || { r -= o.1; false }) {
                let tag = (s >= 0.6).then(|| format!("Scores {:.0}% vs {} players", s * 100.0, book::band_label(band)));
                return (Some(mv), tag, Mood::Calm);
            }
        }
    }

    // 2. Otherwise Stockfish scores the candidates and we decide what *kind* of move this
    //    human makes, using real best/good/inaccuracy/mistake/blunder rates for this
    //    rating band and clock situation (from Lichess games with computer analysis).
    let hurry = clock.map_or(0.0, |t| ((20.0 - t) / 20.0).clamp(0.0, 1.0));
    let beyond = (e - MAGNUS).max(0.0);
    let depth = (((12.0 + beyond / 100.0) * (1.0 - 0.5 * hurry)) as u32).max(2);
    let movetime = ((2500.0 + beyond * 4.0) * (1.0 - 0.9 * hurry)) as u32;
    let lines = search(inp, out, pos, 40, &format!("depth {depth} movetime {movetime}"));
    let Some(&(best_mv, best)) = lines.first() else { return (None, None, Mood::Calm) };

    // phase-specific: e.g. real 1200s botch rook endings far more often than openings
    let ply = (pos.fullmoves().get() as usize - 1) * 2 + pos.turn().is_black() as usize;
    let mut profile = bk.errors[band][book::phase(pos, ply)][book::pressure(clock)];
    if profile.iter().sum::<u32>() < 50 {
        profile = [0; book::CATS];
        for p in bk.errors[band].iter().map(|ph| ph[book::pressure(clock)]) {
            profile.iter_mut().zip(p).for_each(|(a, b)| *a += b);
        }
    }
    if profile.iter().sum::<u32>() == 0 {
        profile[0] = 1;
    }
    let mut weights: Vec<f32> = profile.iter().map(|&c| c as f32).collect();
    let slip = slip_factor(bk, e, book::phase(pos, ply), book::pressure(clock));
    weights[1..].iter_mut().for_each(|w| *w *= slip);
    let total: f32 = weights.iter().sum();
    let mut r = gen_range(0.0, total);
    let mut target = weights.iter().position(|&w| r < w || { r -= w; false }).unwrap_or(0);
    if e < 400.0 && gen_range(0.0, 1.0) < (400.0 - e) / 800.0 {
        target = book::CATS - 1; // below the data: pure potato chaos
    }
    let decided = !(3.0..97.0).contains(&book::win_pct(best));
    if decided {
        target = 0;
    }

    // read the board like a human: under fire -> defend, have the initiative -> press it
    let us = pos.turn();
    let (danger, chances) = (king_pressure(pos.board(), us), king_pressure(pos.board(), !us));
    let mood = if hurry > 0.5 {
        Mood::Hurrying
    } else if danger >= 4 && danger > chances {
        Mood::Defending
    } else if chances >= 3 || (best > 150 && chances >= 1) {
        Mood::Attacking
    } else {
        Mood::Calm
    };
    let spice = (1.0 - e / 4000.0) * match mood {
        Mood::Attacking => 1.6,
        Mood::Defending => 0.4,
        _ => 1.0,
    };
    // the move kind we rolled may not exist here (nothing to blunder): settle for the next best kind
    let cat_of = |cp: i32| book::cat(book::win_pct(best) - book::win_pct(cp));
    while target > 0 && !lines.iter().any(|l| cat_of(l.1) == target) {
        target -= 1;
    }
    let mut pick = (best_mv, best, 0.0, None, f32::MIN);
    for &(m, cp) in lines.iter().filter(|l| cat_of(l.1) == target) {
        let st = style(pos, m, e, best);
        let (react, why) = react(pos, m);
        let mut after = pos.clone();
        after.play_unchecked(m);
        let plan = match mood {
            Mood::Defending => 25.0 * (danger - king_pressure(after.board(), us)) as f32,
            Mood::Attacking => 15.0 * (king_pressure(after.board(), !us) - chances) as f32,
            _ => 0.0,
        };
        // within the kind of move, humans pick the natural-looking one
        let s = 0.3 * cp.clamp(-1500, 1500) as f32 + spice * st + react + plan + gauss() * 20.0;
        if s > pick.4 {
            pick = (m, cp, st, why, s);
        }
    }
    let tag = match (target, pick.2 >= 40.0) {
        (4, true) => Some("Spicy sac?!"),
        (4, false) => Some("Blunder??"),
        (3, true) => Some("Speculative!?"),
        (3, false) => Some("Mistake?"),
        _ if pick.3.is_some() => pick.3,
        _ if pick.0 != best_mv && pick.2 >= 40.0 => Some("Cheeky!"),
        _ => None,
    };
    (Some(pick.0), tag.map(str::to_string), mood)
}

/// (position key, real game index, ply) for every kept game position, sorted by key.
fn real_index() -> &'static Vec<(u64, u32, u8)> {
    static INDEX: std::sync::OnceLock<Vec<(u64, u32, u8)>> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        let mut idx = vec![];
        for (g, rg) in book().games_kept.iter().enumerate() {
            let mut pos = Chess::default();
            for (ply, &code) in rg.moves.iter().enumerate() {
                idx.push((book::key(&pos), g as u32, ply as u8));
                let Some(m) = book::decode(code, &pos) else { break };
                pos.play_unchecked(m);
            }
        }
        idx.sort_unstable();
        idx
    })
}

/// Now and then, if this exact position occurred in a real game between players of
/// this level that our side went on to win, replay that game's move. Once started we
/// keep following it for as long as the opponent (you) keeps matching it.
fn real_game_move(pos: &Chess, e: f32, channel: &mut Option<u32>) -> Option<(Move, (String, String))> {
    let bk = book();
    let idx = real_index();
    let key = book::key(pos);
    let hits = &idx[idx.partition_point(|x| x.0 < key)..];
    let hits: Vec<(u32, u8)> = hits.iter().take_while(|x| x.0 == key).map(|x| (x.1, x.2)).collect();
    let (g, ply) = match channel.and_then(|g| hits.iter().find(|h| h.0 == g)) {
        Some(&h) => h,
        None => {
            let us = pos.turn() as usize;
            let band = book::band(e);
            let fits: Vec<&(u32, u8)> = hits
                .iter()
                .filter(|(g, ply)| {
                    let rg = &bk.games_kept[*g as usize];
                    *ply >= 4 && rg.winner as usize == us && book::band(rg.elo[us] as f32).abs_diff(band) <= 1
                })
                .collect();
            if fits.is_empty() || gen_range(0.0, 1.0) > 0.07 {
                return None;
            }
            *fits[gen_range(0, fits.len())]
        }
    };
    let rg = &bk.games_kept[g as usize];
    let mv = book::decode(*rg.moves.get(ply as usize)?, pos)?;
    *channel = Some(g);
    Some((mv, (format!("https://lichess.org/{}", rg.id), format!("{} vs {}", rg.elo[1], rg.elo[0]))))
}

fn is_blunder(inp: &Mutex<ChildStdin>, out: &mut BufReader<ChildStdout>, pos: &Chess, m: Move) -> bool {
    let lines = search(inp, out, pos, 40, "depth 10 movetime 800");
    let best = lines.first().map_or(0, |l| l.1);
    let cp = lines.iter().find(|l| l.0 == m).map_or(-30000, |l| l.1);
    book::cat(book::win_pct(best) - book::win_pct(cp)) == book::CATS - 1
}

/// Human statistics, built by `cargo run --bin harvest` (see README).
pub fn book() -> &'static book::Book {
    static BOOK: std::sync::OnceLock<book::Book> = std::sync::OnceLock::new();
    BOOK.get_or_init(|| book::Book::read(include_bytes!("../assets/book.bin")).unwrap_or_default())
}

/// Above the data (2500+ band, centred ~2650): how much rarer slips get. Up to ~3200
/// this follows the trend fitted on the 1800..2500+ bands for this phase and clock;
/// the last tier fades slips out entirely: a "theoretical human" who never errs but
/// still picks moves the way a person would.
fn slip_factor(bk: &book::Book, e: f32, phase: usize, pressure: usize) -> f32 {
    const CENTRES: [f32; 4] = [1900.0, 2100.0, 2350.0, 2650.0];
    if e <= 2650.0 {
        return 1.0;
    }
    let pts: Vec<(f32, f32)> = (6..10)
        .filter_map(|b| {
            let c = bk.errors[b][phase][pressure];
            let total: u32 = c.iter().sum();
            (total >= 200 && c[0] < total).then(|| (CENTRES[b - 6], (1.0 - c[0] as f32 / total as f32).ln()))
        })
        .collect();
    // least-squares slope of ln(slip share) vs rating
    let slope = if pts.len() >= 2 {
        let n = pts.len() as f32;
        let (mx, my) = (pts.iter().map(|p| p.0).sum::<f32>() / n, pts.iter().map(|p| p.1).sum::<f32>() / n);
        let cov: f32 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
        let var: f32 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
        (cov / var).min(-0.0003)
    } else {
        -0.001
    };
    let trend = (slope * (e.min(3200.0) - 2650.0)).exp();
    let fade = ((e - 3200.0) / 300.0).clamp(0.0, 1.0);
    trend * (1.0 - fade * fade * (3.0 - 2.0 * fade))
}

/// How many enemy attacks land on the king and the squares around it.
fn king_pressure(b: &shakmaty::Board, king_side: shakmaty::Color) -> i32 {
    let Some(k) = b.king_of(king_side) else { return 0 };
    let zone = shakmaty::attacks::king_attacks(k).with(k);
    zone.into_iter().map(|s| b.attacks_to(s, !king_side, b.occupied()).count() as i32).sum()
}

/// Humans reliably notice two things: their own piece being attacked, and a piece
/// left hanging by the opponent. Bonus for reacting to either, so even noisy
/// low-ELO bots visibly defend and punish instead of ignoring the board.
fn react(pos: &Chess, m: Move) -> (f32, Option<&'static str>) {
    let us = pos.turn();
    let hanging = |b: &shakmaty::Board, owner| -> f32 {
        b.by_color(owner).into_iter().filter(|&s| en_prise(b, s, owner)).filter_map(|s| b.role_at(s)).map(value).sum()
    };
    let mut after = pos.clone();
    after.play_unchecked(m);
    let saved = hanging(pos.board(), us) - hanging(after.board(), us);
    let punished = if m.capture().is_some() && en_prise(pos.board(), m.to(), !us) { m.capture().map_or(0.0, value) } else { 0.0 };
    match (saved > 0.0, punished > 0.0) {
        (_, true) => (0.5 * punished + 0.4 * saved.max(0.0), Some("Punishes!")),
        (true, _) => (0.5 * saved, Some("Defends!")),
        _ => (0.4 * saved.min(0.0), None), // walking into a fresh hang feels wrong to humans too
    }
}

/// Piece on `sq` can be taken for less than it's worth (or for free).
fn en_prise(b: &shakmaty::Board, sq: Square, owner: shakmaty::Color) -> bool {
    let Some(role) = b.role_at(sq).filter(|&r| r != Role::King) else { return false };
    let defended = b.attacks_to(sq, owner, b.occupied()).any();
    let cheapest = b
        .attacks_to(sq, !owner, b.occupied())
        .into_iter()
        .filter_map(|s| b.role_at(s))
        .map(|r| if r == Role::King { if defended { 9999.0 } else { 0.0 } } else { value(r) })
        .fold(f32::MAX, f32::min);
    cheapest < value(role) || (!defended && cheapest < f32::MAX)
}

/// Bonus (cp) for moves humans find exciting.
fn style(pos: &Chess, m: Move, e: f32, best: i32) -> f32 {
    let mut s = 0.0;
    let mut after = pos.clone();
    after.play_unchecked(m);
    if after.is_check() {
        s += 45.0;
    }
    if let Some(c) = m.capture() {
        s += 15.0 + value(c) / 10.0;
        // Magnus-ish grinding: happily trades down when ahead
        if e >= 2500.0 && best >= 60 && value(c) >= value(m.role()) - 50.0 {
            s += 25.0;
        }
    }
    if let Some(k) = pos.board().king_of(!pos.turn()) {
        if m.role() != Role::King && m.to().distance(k) <= 2 {
            s += 25.0;
        }
    }
    if m.role() == Role::Queen && e < 1100.0 {
        s += 35.0; // beginners love the queen
    }
    let r = m.to().rank().to_u32();
    if m.role() == Role::Pawn && (if pos.turn().is_white() { r >= 4 } else { r <= 3 }) {
        s += 15.0;
    }
    if m.is_castle() {
        s += 10.0;
    }
    s
}

fn value(r: Role) -> f32 {
    match r {
        Role::Pawn => 100.0,
        Role::Knight | Role::Bishop => 320.0,
        Role::Rook => 500.0,
        Role::Queen => 900.0,
        Role::King => 0.0,
    }
}

fn gauss() -> f32 {
    let (u, v) = (gen_range(1e-6f32, 1.0), gen_range(0.0f32, 1.0));
    (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Grade {
    Brilliant,
    Great,
    Book,
    Best,
    Excellent,
    Good,
    Inaccuracy,
    Mistake,
    Blunder,
}

impl Grade {
    pub fn label(self) -> (&'static str, &'static str) {
        match self {
            Grade::Brilliant => ("!!", "Brilliant!!"),
            Grade::Great => ("!", "Great move!"),
            Grade::Book => ("≡", "Book"),
            Grade::Best => ("★", "Best"),
            Grade::Excellent => ("✓", "Excellent"),
            Grade::Good => ("✓", "Good"),
            Grade::Inaccuracy => ("?!", "Inaccuracy"),
            Grade::Mistake => ("?", "Mistake"),
            Grade::Blunder => ("??", "Blunder"),
        }
    }
}

/// Win% swing, lichess-style. `lines` = analysis of `pos` before the move.
pub fn classify(pos: &Chess, lines: &[(Move, i32)], played: Move) -> Option<Grade> {
    let &(best_mv, best) = lines.first()?;
    let &(_, cp) = lines.iter().find(|l| l.0 == played)?;
    let wp = book::win_pct;
    let loss = wp(best) - wp(cp);
    let second = lines.get(1).map_or(-30000, |l| l.1);
    Some(match loss {
        _ if loss <= 2.0 && wp(cp) > 30.0 && wp(best) < 97.0 && is_sacrifice(pos, played) => Grade::Brilliant,
        _ if played == best_mv && wp(best) - wp(second) >= 15.0 => Grade::Great,
        _ if played == best_mv || loss <= 0.5 => Grade::Best,
        l if l < 2.0 => Grade::Excellent,
        l if l < 5.0 => Grade::Good,
        l if l < 10.0 => Grade::Inaccuracy,
        l if l < 20.0 => Grade::Mistake,
        _ => Grade::Blunder,
    })
}

/// A non-pawn piece left where it can be taken for less than it's worth.
fn is_sacrifice(pos: &Chess, m: Move) -> bool {
    if matches!(m.role(), Role::Pawn | Role::King) {
        return false;
    }
    let mut after = pos.clone();
    after.play_unchecked(m);
    value(m.role()) - m.capture().map_or(0.0, value) >= 200.0 && en_prise(after.board(), m.to(), pos.turn())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_elo_returns_a_legal_move() {
        let e = spawn().unwrap();
        let pos = Chess::default();
        for elo in [1.0, 800.0, 1600.0, 2400.0, MAGNUS, MAX_ELO] {
            e.tx.send(Req::Think { game_id: 0, pos: pos.clone(), elo, clock: Some(5.0) }).unwrap();
            let Reply::Move { mv, .. } = e.rx.recv().unwrap() else { panic!() };
            assert!(pos.is_legal(mv.unwrap()), "elo {elo}");
        }
    }

    #[test]
    fn grades_blunders_and_brilliancies() {
        let e = spawn().unwrap();
        let grade = |fen: &str, uci: &str| {
            let pos: Chess = fen.parse::<Fen>().unwrap().into_position(shakmaty::CastlingMode::Standard).unwrap();
            e.tx.send(Req::Analyse { game_id: 0, ply: 0, pos: pos.clone() }).unwrap();
            let Reply::Analysis { lines, .. } = e.rx.recv().unwrap() else { panic!() };
            let m = uci.parse::<UciMove>().unwrap().to_move(&pos).unwrap();
            classify(&pos, &lines, m)
        };
        // hanging the queen for nothing
        assert_eq!(grade("rnbqkbnr/ppp1pppp/3p4/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2", "d1g4"), Some(Grade::Blunder));
        // stockfish's own top choice can't be worse than Best
        let g = grade("rnbqkbnr/ppp1pppp/3p4/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2", "d2d4");
        assert!(matches!(g, Some(Grade::Brilliant | Grade::Great | Grade::Best | Grade::Excellent)), "{g:?}");
    }

    #[test]
    fn notices_attacked_pieces_and_free_material() {
        let pos = |fen: &str| -> Chess { fen.parse::<Fen>().unwrap().into_position(shakmaty::CastlingMode::Standard).unwrap() };
        let mv = |p: &Chess, u: &str| u.parse::<UciMove>().unwrap().to_move(p).unwrap();
        // black knight on f6 hit by a pawn on e5: moving it away is a "Defends!"
        let p = pos("rnbqkb1r/pppp1ppp/5n2/4P3/8/8/PPP2PPP/RNBQKBNR b KQkq - 0 3");
        assert_eq!(react(&p, mv(&p, "f6e4")).1, Some("Defends!"));
        // white queen left on g4 for the c8 bishop: taking it "Punishes!"
        let p = pos("rnbqkbnr/ppp1pppp/3p4/8/4P1Q1/8/PPPP1PPP/RNB1KBNR b KQkq - 1 2");
        assert_eq!(react(&p, mv(&p, "c8g4")).1, Some("Punishes!"));
    }
}

#[cfg(test)]
mod book_tests {
    use super::*;

    #[test]
    fn bots_open_like_smart_humans_of_their_level() {
        let e = spawn().unwrap();
        for elo in [600.0, 1200.0, 2000.0, MAGNUS] {
            let mut seen: Vec<String> = vec![];
            for _ in 0..12 {
                e.tx.send(Req::Think { game_id: 0, pos: Chess::default(), elo, clock: None }).unwrap();
                let Reply::Move { mv, tag, real, .. } = e.rx.recv().unwrap() else { panic!() };
                assert!(real.is_none(), "start position is never a replay");
                let mv = mv.unwrap();
                assert!(book().moves(book::key(&Chess::default()), book::band(elo)).unwrap().iter().any(|b| b.mv == book::encode(mv)));
                seen.push(format!("{}{}", mv.to_uci(shakmaty::CastlingMode::Standard), tag.map_or(String::new(), |t| format!(" ({t})"))));
            }
            println!("{elo}: {}", seen.join(", "));
        }
    }
}

#[cfg(test)]
#[test]
fn replays_real_games_move_for_move() {
    let rg = &book().games_kept[0];
    let mut pos = Chess::default();
    for (ply, &code) in rg.moves.iter().enumerate().take(20) {
        let mut ch = Some(0);
        let (mv, (url, _)) = real_game_move(&pos, 1500.0, &mut ch).expect("following a known game");
        assert_eq!((book::encode(mv), ch, url.len() > 20), (code, Some(0), true), "ply {ply}");
        pos.play_unchecked(mv);
    }
}

#[cfg(test)]
#[test]
fn beyond_magnus_slips_less_and_theoretical_human_never_does() {
    let bk = book();
    let f: Vec<f32> = [2600.0, MAGNUS, 3000.0, 3200.0, 3400.0, MAX_ELO].iter().map(|&e| slip_factor(bk, e, 1, 0)).collect();
    println!("middlegame slip factor: {f:?}");
    assert!(f.windows(2).all(|w| w[1] <= w[0]), "{f:?}");
    assert_eq!(f[5], 0.0);
}
