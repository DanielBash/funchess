// Stockfish does the thinking; we decide which of its candidate moves a human of
// the chosen ELO would actually play. Low ELO = big noise + loves checks/captures/king hunts.
// The same process also grades the player's moves (analysis runs while they think).
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
    Move { game_id: u64, mv: Option<Move>, tag: Option<&'static str>, mood: Mood },
    Analysis { game_id: u64, ply: usize, lines: Vec<(Move, i32)> },
}

pub const MAGNUS: f32 = 2850.0;

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
        for req in req_rx {
            let reply = match req {
                Req::Think { game_id, pos, elo, clock } => {
                    let (mv, tag, mood) = think(&inp, &mut out, &pos, elo, clock);
                    Reply::Move { game_id, mv, tag, mood }
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

fn think(inp: &Mutex<ChildStdin>, out: &mut BufReader<ChildStdout>, pos: &Chess, e: f32, clock: Option<f32>) -> (Option<Move>, Option<&'static str>, Mood) {
    // low on time: think less, play sloppier
    let hurry = clock.map_or(0.0, |t| ((20.0 - t) / 20.0).clamp(0.0, 1.0));
    // ponytail: ELO->(depth, noise, lapses) curve is vibes, not calibrated. Tune here.
    // Even "Magnus" is human: shallow-ish, a bit noisy, and has lapses where he just
    // plays something natural-looking from the candidate list.
    let lapse = gen_range(0.0, 1.0) < 0.25 * (-e / 700.0).exp() + 0.07;
    let depth = ((1.0 + e / 350.0) * (1.0 - 0.6 * hurry)).max(1.0) as u32;
    let movetime = (2500.0 * (1.0 - 0.9 * hurry)) as u32;
    let multipv = if e < 1500.0 { 64 } else { 8 };
    let lines = search(inp, out, pos, multipv, &format!("depth {depth} movetime {movetime}"));
    let Some(&(best_mv, best)) = lines.first() else { return (None, None, Mood::Calm) };

    let mut sigma = 700.0 * (-e / 550.0).exp() + 25.0 + 120.0 * hurry;
    if lapse {
        sigma = sigma.max(250.0);
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
    let mut pick = (best_mv, best, 0.0, None, f32::MIN);
    for &(m, cp) in &lines {
        let st = style(pos, m, e, best);
        let (react, why) = react(pos, m);
        let mut after = pos.clone();
        after.play_unchecked(m);
        let plan = match mood {
            Mood::Defending => 25.0 * (danger - king_pressure(after.board(), us)) as f32,
            Mood::Attacking => 15.0 * (king_pressure(after.board(), !us) - chances) as f32,
            _ => 0.0,
        };
        // clamp so a weak player can still miss a mate (or walk into one)
        let s = cp.clamp(-1500, 1500) as f32 + spice * st + react + plan + gauss() * sigma;
        if s > pick.4 {
            pick = (m, cp, st, why, s);
        }
    }
    let loss = best.clamp(-1500, 1500) - pick.1.clamp(-1500, 1500);
    let tag = match (loss, pick.2 >= 40.0) {
        (200.., true) => Some("Spicy sac?!"),
        (200.., false) => Some("Blunder??"),
        (80.., true) => Some("Speculative!?"),
        _ if pick.3.is_some() => pick.3,
        _ if pick.0 != best_mv && pick.2 >= 40.0 => Some("Cheeky!"),
        _ => None,
    };
    (Some(pick.0), tag, mood)
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
    let wp = |cp: i32| 100.0 / (1.0 + (-0.00368208 * cp as f32).exp());
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
        for elo in [1.0, 800.0, 1600.0, 2400.0, MAGNUS] {
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
