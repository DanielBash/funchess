#!/usr/bin/env python3
"""One-off asset prep (needs PIL + GNOME's librsvg via PyGObject).

- Staunton boards are drawn squashed for a tilted 3D view (8 rows in 936px plus an
  edge strip): crop the strip and stretch back to a square.
- Material Symbols icons: render white 96px PNGs for the move-grade badges.
"""
import pathlib
import gi
from PIL import Image

gi.require_version("Rsvg", "2.0")
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf  # noqa: E402

root = pathlib.Path(__file__).resolve().parent.parent / "assets"

for b in (root / "boards").glob("*.png"):
    im = Image.open(b).convert("RGB")
    if im.size != (1024, 1024):
        im.crop((0, 0, 1024, 936)).resize((1024, 1024), Image.LANCZOS).save(b, optimize=True)

for svg in (root / "grades").glob("*.svg"):
    text = svg.read_text().replace("<svg ", '<svg fill="white" ', 1)
    tmp = svg.with_suffix(".white.svg")
    tmp.write_text(text)
    GdkPixbuf.Pixbuf.new_from_file_at_size(str(tmp), 96, 96).savev(str(svg.with_suffix(".png")), "png", [], [])
    tmp.unlink()
