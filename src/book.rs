// Human move statistics harvested from Lichess games by src/bin/harvest.rs:
// what players of each rating band play in common positions and how it works out
// for them, how often they slip up (by game phase and clock pressure), and opening names.
use shakmaty::{uci::UciMove, zobrist::Zobrist64, CastlingMode, Chess, EnPassantMode, Move, Position, Role, Square};
use std::collections::HashMap;
use std::io::{self, Write};

/// Lower edge of each rating band (Lichess ratings).
pub const BANDS: [u16; 10] = [0, 800, 1000, 1200, 1400, 1600, 1800, 2000, 2200, 2500];
/// Game phases; endings split by what's left on the board.
pub const PHASES: [&str; 6] = ["Opening", "Middlegame", "Pawn ending", "Rook ending", "Minor-piece ending", "Queen ending"];
/// Clock pressure buckets: >= 60s left, 15-60s, < 15s.
pub const PRESSURE: usize = 3;
/// Move quality by win% lost: < 2, < 5, < 10, < 20, >= 20 (best .. blunder).
pub const CATS: usize = 5;
const CAT_EDGES: [f32; CATS - 1] = [2.0, 5.0, 10.0, 20.0];

pub fn band(elo: f32) -> usize {
    BANDS.iter().rposition(|&b| elo >= b as f32).unwrap_or(0)
}

pub fn band_label(band: usize) -> String {
    match BANDS.get(band + 1) {
        _ if band == 0 => "under-800".into(),
        Some(next) => format!("{}-{}", BANDS[band], next),
        None => format!("{}+", BANDS[band]),
    }
}

pub fn pressure(clock_secs: Option<f32>) -> usize {
    match clock_secs {
        Some(t) if t < 15.0 => 2,
        Some(t) if t < 60.0 => 1,
        _ => 0,
    }
}

pub fn phase(pos: &Chess, ply: usize) -> usize {
    let b = pos.board();
    let pts = |r: Role, v: u32| b.by_role(r).count() as u32 * v;
    let material = pts(Role::Knight, 3) + pts(Role::Bishop, 3) + pts(Role::Rook, 5) + pts(Role::Queen, 9);
    match material {
        0 => 2,
        _ if material <= 26 && b.by_role(Role::Queen).any() => 5,
        _ if material <= 26 && b.by_role(Role::Rook).any() => 3,
        _ if material <= 26 => 4,
        _ if ply < 20 => 0,
        _ => 1,
    }
}

pub fn cat(loss: f32) -> usize {
    CAT_EDGES.iter().position(|&e| loss < e).unwrap_or(CATS - 1)
}

/// Lichess win% for a centipawn score.
pub fn win_pct(cp: i32) -> f32 {
    100.0 / (1.0 + (-0.00368208 * cp as f32).exp())
}

pub fn key(pos: &Chess) -> u64 {
    pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0
}

pub fn encode(m: Move) -> u16 {
    match m.to_uci(CastlingMode::Standard) {
        UciMove::Normal { from, to, promotion } => from as u16 | (to as u16) << 6 | (promotion.map_or(0, |r| r as u16) << 12),
        _ => 0,
    }
}

pub fn decode(code: u16, pos: &Chess) -> Option<Move> {
    let from = Square::new((code & 63) as u32);
    let to = Square::new((code >> 6 & 63) as u32);
    let promotion = Role::try_from(code >> 12).ok();
    UciMove::Normal { from, to, promotion }.to_move(pos).ok()
}

#[derive(Clone, Copy, Debug)]
pub struct BookMove {
    pub mv: u16,
    pub count: u32,
    /// Points scored by the mover, x2 (win 2, draw 1).
    pub half_points: u32,
}

impl BookMove {
    pub fn score(&self) -> f32 {
        self.half_points as f32 / (2.0 * self.count.max(1) as f32)
    }
}

/// A real Lichess game between two players of the same band, kept so bots can
/// occasionally replay it move for move.
pub struct RealGame {
    pub id: String,
    /// [black, white]
    pub elo: [u16; 2],
    /// 0 = black won, 1 = white won
    pub winner: u8,
    pub moves: Vec<u16>,
}

#[derive(Default)]
pub struct Book {
    pub games_kept: Vec<RealGame>,
    pub games: u64,
    /// [band][phase][pressure][move quality] counts
    pub errors: [[[[u32; CATS]; PRESSURE]; PHASES.len()]; BANDS.len()],
    pub positions: HashMap<(u64, u8), Vec<BookMove>>,
    pub names: HashMap<u64, String>,
}

impl Book {
    /// Moves real players of `band` chose here, most popular first.
    pub fn moves(&self, key: u64, band: usize) -> Option<&[BookMove]> {
        self.positions.get(&(key, band as u8)).map(|v| v.as_slice())
    }

    /// Expected score for whoever plays into `after`, given how players of `band`
    /// usually answer there, and how many games that's based on.
    pub fn practical(&self, after: u64, band: usize) -> Option<(f32, u32)> {
        let replies = self.moves(after, band)?;
        let total: u32 = replies.iter().map(|m| m.count).sum();
        (total >= 12).then(|| (replies.iter().map(|m| m.count as f32 * (1.0 - m.score())).sum::<f32>() / total as f32, total))
    }

    /// Share of moves in `phase` that are (mistake or) blunder for this band, all clocks.
    pub fn slip_rate(&self, band: usize, phase: usize) -> Option<f32> {
        let mut c = [0u32; CATS];
        for p in &self.errors[band][phase] {
            for (a, b) in c.iter_mut().zip(p) {
                *a += b;
            }
        }
        let total: u32 = c.iter().sum();
        (total >= 200).then(|| (c[3] + c[4]) as f32 / total as f32)
    }

    /// All bands merged.
    pub fn all(&self, key: u64) -> Vec<BookMove> {
        let mut out: Vec<BookMove> = vec![];
        for b in 0..BANDS.len() {
            for m in self.moves(key, b).unwrap_or_default() {
                match out.iter_mut().find(|o| o.mv == m.mv) {
                    Some(o) => {
                        o.count += m.count;
                        o.half_points += m.half_points;
                    }
                    None => out.push(*m),
                }
            }
        }
        out.sort_by_key(|m| std::cmp::Reverse(m.count));
        out
    }

    #[allow(dead_code)] // used by the harvester
    pub fn write(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(b"FCB3")?;
        w.write_all(&self.games.to_le_bytes())?;
        for c in self.errors.iter().flatten().flatten().flatten() {
            w.write_all(&c.to_le_bytes())?;
        }
        w.write_all(&(self.names.len() as u32).to_le_bytes())?;
        for (k, n) in &self.names {
            w.write_all(&k.to_le_bytes())?;
            w.write_all(&(n.len() as u16).to_le_bytes())?;
            w.write_all(n.as_bytes())?;
        }
        w.write_all(&(self.games_kept.len() as u32).to_le_bytes())?;
        for g in &self.games_kept {
            w.write_all(&[g.id.len() as u8])?;
            w.write_all(g.id.as_bytes())?;
            w.write_all(&g.elo[0].to_le_bytes())?;
            w.write_all(&g.elo[1].to_le_bytes())?;
            w.write_all(&[g.winner, g.moves.len() as u8])?;
            for m in &g.moves {
                w.write_all(&m.to_le_bytes())?;
            }
        }
        w.write_all(&(self.positions.len() as u32).to_le_bytes())?;
        for ((k, b), moves) in &self.positions {
            w.write_all(&k.to_le_bytes())?;
            w.write_all(&[*b, moves.len() as u8])?;
            for m in moves {
                w.write_all(&m.mv.to_le_bytes())?;
                w.write_all(&m.count.to_le_bytes())?;
                w.write_all(&m.half_points.to_le_bytes())?;
            }
        }
        Ok(())
    }

    pub fn read(mut b: &[u8]) -> Option<Book> {
        fn take<'a>(b: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
            let (h, t) = (b.get(..n)?, b.get(n..)?);
            *b = t;
            Some(h)
        }
        let u8_ = |b: &mut &[u8]| Some(take(b, 1)?[0]);
        let u16_ = |b: &mut &[u8]| Some(u16::from_le_bytes(take(b, 2)?.try_into().ok()?));
        let u32_ = |b: &mut &[u8]| Some(u32::from_le_bytes(take(b, 4)?.try_into().ok()?));
        let u64_ = |b: &mut &[u8]| Some(u64::from_le_bytes(take(b, 8)?.try_into().ok()?));
        if take(&mut b, 4)? != b"FCB3" {
            return None;
        }
        let mut book = Book { games: u64_(&mut b)?, ..Default::default() };
        for c in book.errors.iter_mut().flatten().flatten().flatten() {
            *c = u32_(&mut b)?;
        }
        for _ in 0..u32_(&mut b)? {
            let k = u64_(&mut b)?;
            let n = u16_(&mut b)? as usize;
            book.names.insert(k, String::from_utf8_lossy(take(&mut b, n)?).into_owned());
        }
        for _ in 0..u32_(&mut b)? {
            let n = u8_(&mut b)? as usize;
            let id = String::from_utf8_lossy(take(&mut b, n)?).into_owned();
            let elo = [u16_(&mut b)?, u16_(&mut b)?];
            let (winner, n) = (u8_(&mut b)?, u8_(&mut b)?);
            let moves = (0..n).map(|_| u16_(&mut b)).collect::<Option<Vec<_>>>()?;
            book.games_kept.push(RealGame { id, elo, winner, moves });
        }
        for _ in 0..u32_(&mut b)? {
            let k = u64_(&mut b)?;
            let band = u8_(&mut b)?;
            let n = u8_(&mut b)?;
            let moves = (0..n)
                .map(|_| Some(BookMove { mv: u16_(&mut b)?, count: u32_(&mut b)?, half_points: u32_(&mut b)? }))
                .collect::<Option<Vec<_>>>()?;
            book.positions.insert((k, band), moves);
        }
        Some(book)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_and_book_round_trip() {
        let pos: Chess = "r3k2r/1P6/8/8/8/8/8/R3K2R w KQkq - 0 1"
            .parse::<shakmaty::fen::Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        for m in pos.legal_moves() {
            assert_eq!(decode(encode(m), &pos), Some(m), "{m:?}");
        }
        let mut b = Book { games: 7, ..Default::default() };
        b.errors[3][5][1][4] = 42;
        b.names.insert(1, "Italian Game".into());
        b.positions.insert((9, 2), vec![BookMove { mv: 5, count: 6, half_points: 7 }]);
        b.games_kept.push(RealGame { id: "AbCd1234".into(), elo: [1500, 1510], winner: 1, moves: vec![1, 2, 3] });
        let mut bytes = vec![];
        b.write(&mut bytes).unwrap();
        let r = Book::read(&bytes).unwrap();
        assert_eq!((r.games, r.errors[3][5][1][4], r.names[&1].as_str()), (7, 42, "Italian Game"));
        assert_eq!(r.moves(9, 2).unwrap()[0].half_points, 7);
        assert_eq!((r.games_kept[0].id.as_str(), r.games_kept[0].elo, r.games_kept[0].moves.len()), ("AbCd1234", [1500, 1510], 3));
    }

    #[test]
    fn practical_score_looks_at_how_people_answer() {
        let mut b = Book::default();
        // after our move, 1200s mostly answer with a reply that scores only 10% for them
        b.positions.insert((5, 3), vec![
            BookMove { mv: 1, count: 80, half_points: 16 },
            BookMove { mv: 2, count: 20, half_points: 30 },
        ]);
        let p = b.practical(5, 3).unwrap().0;
        assert!((p - (0.8 * 0.9 + 0.2 * 0.25)).abs() < 1e-4, "{p}");
        assert_eq!(b.practical(5, 4), None);
    }
}
