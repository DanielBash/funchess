#!/usr/bin/env bash
# Builds Linux + Windows zips into dist/, each bundled with Stockfish (GPLv3).
# Needs: rustup target x86_64-pc-windows-gnullvm, and llvm-mingw (https://github.com/mstorsjo/llvm-mingw)
set -euo pipefail
cd "$(dirname "$0")"
LLVM_MINGW=${LLVM_MINGW:-$(ls -d ~/.local/share/llvm-mingw-dl/llvm-mingw-* | head -1)}
SF=sf_19
WIN=x86_64-pc-windows-gnullvm

cargo build --release
CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER="$LLVM_MINGW/bin/x86_64-w64-mingw32-clang" \
  cargo build --release --target $WIN

rm -rf dist && mkdir -p dist/dl dist/funchess-linux dist/funchess-windows
curl -sL -o dist/dl/sf-linux.tgz "https://github.com/official-stockfish/Stockfish/releases/download/$SF/stockfish-linux-x86-64-universal.tar.gz"
curl -sL -o dist/dl/sf-win.zip "https://github.com/official-stockfish/Stockfish/releases/download/$SF/stockfish-windows-x86-64-universal.zip"
tar xzf dist/dl/sf-linux.tgz -C dist/dl
unzip -q dist/dl/sf-win.zip -d dist/dl/win

cp target/release/funchess dist/funchess-linux/
cp dist/dl/stockfish/stockfish-linux-x86-64-universal dist/funchess-linux/stockfish
cp target/$WIN/release/funchess.exe dist/funchess-windows/
cp dist/dl/win/stockfish/stockfish-windows-x86-64-universal.exe dist/funchess-windows/stockfish.exe
for d in dist/funchess-linux dist/funchess-windows; do
  cp dist/dl/stockfish/Copying.txt "$d/Stockfish-COPYING.txt"
  echo "Stockfish $SF (GPLv3), source: https://github.com/official-stockfish/Stockfish/tree/$SF" > "$d/Stockfish-SOURCE.txt"
done
# llvm-mingw's C++ runtime, if the exe links it dynamically
for dll in libunwind.dll libc++.dll; do
  if "$LLVM_MINGW/bin/llvm-objdump" -p dist/funchess-windows/funchess.exe | grep -qi "$dll"; then
    cp "$LLVM_MINGW/x86_64-w64-mingw32/bin/$dll" dist/funchess-windows/
  fi
done
(cd dist && zip -qr funchess-windows-x64.zip funchess-windows && tar czf funchess-linux-x64.tar.gz funchess-linux)
rm -rf dist/dl
ls -lh dist/*.zip dist/*.tar.gz
