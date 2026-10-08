// Builds assets/book.bin from a Lichess PGN dump on stdin:
//   curl -s https://database.lichess.org/standard/lichess_db_standard_rated_2026-08.pgn.zst \
//     | tee data/raw/lichess-2026-08-head.pgn.zst \
//     | zstdcat | cargo run --release --bin harvest -- 6000000 assets/book.bin data/openings/*.tsv
// (re-runs: `zstdcat data/raw/lichess-2026-08-head.pgn.zst | ...` - no download)
// Skips bullet. Opening tree: first BOOK_PLIES plies, per rating band of the mover.
// Error profile: only games with Lichess computer analysis ([%eval]).
#[path = "../book.rs"]
#[allow(dead_code)]
mod book;

use book::*;
use shakmaty::{san::San, Chess, Position};
use std::collections::HashMap;
use std::io::{BufRead, BufWriter};

const BOOK_PLIES: usize = 30;
const KEEP_PER_BAND: usize = 1000;
const KEEP_PLIES: usize = 80;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let max_games: u64 = args.get(1).and_then(|a| a.parse().ok()).expect("usage: harvest <games> <out> [openings.tsv...]");
    let out = &args[2];
    let mut book = Book::default();

    for tsv in &args[3..] {
        for line in std::fs::read_to_string(tsv).unwrap().lines().skip(1) {
            let mut cols = line.split('\t');
            let (Some(_eco), Some(name), Some(pgn)) = (cols.next(), cols.next(), cols.next()) else { continue };
            let mut pos = Chess::default();
            if pgn.split_whitespace().filter(|t| !t.ends_with('.')).all(|t| play_san(&mut pos, t)) {
                book.names.entry(key(&pos)).or_insert_with(|| name.to_string());
            }
        }
    }

    let mut tags: HashMap<String, String> = HashMap::new();
    let mut kept_per_band = [0usize; BANDS.len()];
    let stdin = std::io::stdin().lock();
    for line in stdin.lines() {
        let Ok(line) = line else { break };
        if let Some(tag) = line.strip_prefix('[') {
            if let Some((k, v)) = tag.split_once(' ') {
                tags.insert(k.to_string(), v.trim_end_matches(']').trim_matches('"').to_string());
            }
        } else if line.starts_with("1.") {
            let event = tags.get("Event").map_or("", |s| s.as_str());
            let base: f32 = tags.get("TimeControl").and_then(|t| t.split('+').next()?.parse().ok()).unwrap_or(0.0);
            if !event.contains("Bullet") && base >= 180.0 {
                let elo = |k: &str| tags.get(k).and_then(|e| e.parse::<f32>().ok());
                let result = match tags.get("Result").map(|s| s.as_str()) {
                    Some("1-0") => [0, 2], // half-points for [black, white]
                    Some("0-1") => [2, 0],
                    _ => [1, 1],
                };
                if let (Some(w), Some(b)) = (elo("WhiteElo"), elo("BlackElo")) {
                    // same-band decisive games become replayable "real games"
                    let keep = band(w) == band(b)
                        && result != [1, 1]
                        && kept_per_band[band(w)] < KEEP_PER_BAND;
                    let moves = game(&mut book, &line, [band(b), band(w)], result, keep);
                    let id = tags.get("Site").and_then(|s| s.rsplit('/').next()).unwrap_or_default().to_string();
                    if keep && moves.len() >= 30 && !id.is_empty() {
                        kept_per_band[band(w)] += 1;
                        book.games_kept.push(RealGame { id, elo: [b as u16, w as u16], winner: (result[1] == 2) as u8, moves });
                    }
                    book.games += 1;
                    if book.games % 250_000 == 0 {
                        // drop one-off positions so memory stays flat
                        book.positions.retain(|_, v| v.iter().map(|m| m.count).sum::<u32>() >= 2);
                        eprintln!("{} games, {} positions", book.games, book.positions.len());
                    }
                    if book.games >= max_games {
                        break;
                    }
                }
            }
            tags.clear();
        }
    }

    // keep positions seen >= 12 times in a band, and moves played >= 1% (and twice)
    book.positions.retain(|_, moves| {
        let total: u32 = moves.iter().map(|m| m.count).sum();
        moves.retain(|m| m.count >= 2 && m.count * 100 >= total);
        moves.sort_by_key(|m| std::cmp::Reverse(m.count));
        moves.truncate(255);
        total >= 12 && !moves.is_empty()
    });
    let mut w = BufWriter::new(std::fs::File::create(out).unwrap());
    book.write(&mut w).unwrap();
    eprintln!("done: {} games, {} positions, {} names, {} real games kept", book.games, book.positions.len(), book.names.len(), book.games_kept.len());
    for (b, e) in book.errors.iter().enumerate() {
        let rates: Vec<String> = (0..PHASES.len()).map(|p| book.slip_rate(b, p).map_or("-".into(), |r| format!("{:.1}%", r * 100.0))).collect();
        eprintln!("band {:>4}: mistakes+blunders by phase {:?}  ({} moves)", BANDS[b], rates, e.iter().flatten().flatten().sum::<u32>());
    }
}

fn play_san(pos: &mut Chess, tok: &str) -> bool {
    let tok = tok.trim_end_matches(['!', '?']);
    match tok.parse::<San>().ok().and_then(|s| s.to_move(pos).ok()) {
        Some(m) => {
            pos.play_unchecked(m);
            true
        }
        None => false,
    }
}

/// `bands`/`result` are indexed by shakmaty Color (black 0, white 1).
fn game(book: &mut Book, text: &str, bands: [usize; 2], result: [u32; 2], keep: bool) -> Vec<u16> {
    let mut kept = vec![];
    let has_eval = text.contains("%eval");
    let mut pos = Chess::default();
    let mut eval = 20; // white POV, before the move
    let mut ply = 0;
    let mut rest = text;
    let mut mover_band = 0;
    let mut mover_white = true;
    let mut mover_phase = 0;
    while let Some(c) = rest.chars().next() {
        if c.is_whitespace() {
            rest = &rest[1..];
        } else if c == '{' {
            let end = rest.find('}').unwrap_or(rest.len() - 1);
            let comment = &rest[1..end];
            rest = &rest[end + 1..];
            // comment belongs to the move just played
            let after = field(comment, "%eval").and_then(|e| match e.strip_prefix('#') {
                Some(m) => m.parse::<i32>().ok().map(|m| if m > 0 { 30000 - m * 10 } else { -30000 - m * 10 }),
                None => e.parse::<f32>().ok().map(|p| (p * 100.0) as i32),
            });
            let clock = field(comment, "%clk").and_then(|c| {
                let p: Vec<f32> = c.split(':').filter_map(|x| x.parse().ok()).collect();
                (p.len() == 3).then(|| p[0] * 3600.0 + p[1] * 60.0 + p[2])
            });
            if let (Some(after), true) = (after, ply > 0) {
                let sign = if mover_white { 1 } else { -1 };
                let before_wp = win_pct(sign * eval);
                if (3.0..97.0).contains(&before_wp) {
                    let loss = before_wp - win_pct(sign * after);
                    book.errors[mover_band][mover_phase][pressure(clock)][cat(loss)] += 1;
                }
                eval = after;
            }
        } else {
            let end = rest.find(|c: char| c.is_whitespace() || c == '{').unwrap_or(rest.len());
            let tok = &rest[..end];
            rest = &rest[end..];
            if tok.ends_with('.') || matches!(tok, "1-0" | "0-1" | "1/2-1/2" | "*") {
                continue;
            }
            if ply >= BOOK_PLIES && !has_eval && !(keep && ply < KEEP_PLIES) {
                return kept;
            }
            let turn = pos.turn();
            let Some(m) = tok.trim_end_matches(['!', '?']).parse::<San>().ok().and_then(|s| s.to_move(&pos).ok()) else { return kept };
            if keep && ply < KEEP_PLIES {
                kept.push(encode(m));
            }
            mover_band = bands[turn as usize];
            mover_phase = phase(&pos, ply);
            mover_white = turn.is_white();
            if ply < BOOK_PLIES {
                let moves = book.positions.entry((key(&pos), mover_band as u8)).or_default();
                let code = encode(m);
                let hp = result[turn as usize];
                match moves.iter_mut().find(|x| x.mv == code) {
                    Some(x) => {
                        x.count += 1;
                        x.half_points += hp;
                    }
                    None => moves.push(BookMove { mv: code, count: 1, half_points: hp }),
                }
            }
            pos.play_unchecked(m);
            ply += 1;
        }
    }
    kept
}

fn field<'a>(comment: &'a str, name: &str) -> Option<&'a str> {
    let i = comment.find(name)? + name.len();
    comment[i..].split([' ', ']']).find(|s| !s.is_empty())
}
