# Fun Chess

Chess vs bots that play like people, from ELO 1 ("Potato") to "Magnus".
Stockfish ranks the candidate moves; the bot then picks like a human would: it loves
checks and attacks, defends pieces you attack, punishes pieces you hang, gets
defensive when you go for its king, has lapses (yes, Magnus too) and hurries on the clock.

- Bots learned from millions of real Lichess games: in common positions they play what real
  players *of their rating* play (including the traps they fall for and set), and afterwards they
  make best/good/inaccurate/mistaken/blundering moves at the real rate for their rating and clock.
- Practice mode: drill an opening or a trap line; the bot follows it, then answers like humans do.
- "Book" grades show how many players make your move; "Trap!" warns when most players go wrong; the
  opening name is shown as you play.
- Your moves are graded instantly: Brilliant !!, Great !, Best, Excellent, Good, Inaccuracy ?!, Mistake ?, Blunder ??
- Clocks: none, 1+0, 3+2, 5+0, 10+0
- Overlay mode: borderless, transparent, always on top, never steals focus. Clicks and scrolling
  pass through everywhere except your pieces / move targets / the control strip, so you can keep
  coding or scrolling underneath. Drag the dotted grip to move it.

## Download
Grab the zip for your OS from Releases, unzip, run `funchess` / `funchess.exe`
(keep `stockfish` next to it).

## Build
    cargo run --release            # needs a `stockfish` binary next to Cargo.toml or on PATH
    ./release.sh                   # Linux + Windows packages in dist/

    # rebuild the human statistics (assets/book.bin) from a Lichess monthly dump:
    curl -s https://database.lichess.org/standard/lichess_db_standard_rated_2026-08.pgn.zst \
      | zstdcat | cargo run --release --bin harvest -- 6000000 assets/book.bin data/openings/*.tsv

Controls: wheel = zoom (scroll over the panel = ELO), right-drag = pan, R = reset view.

Game statistics come from the Lichess open database (CC0); opening names from
lichess-org/chess-openings (CC0, in data/openings). Stockfish is GPLv3 and is distributed unmodified alongside this program; see the bundled
Stockfish-COPYING.txt / Stockfish-SOURCE.txt. `vendor/miniquad` is miniquad 0.4.11 (MIT/Apache)
with a small patch so transparent windows get a 32-bit visual on X11.
