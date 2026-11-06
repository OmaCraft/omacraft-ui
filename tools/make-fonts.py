#!/usr/bin/env python3
"""Rebuild the derived fonts in assets/fonts from the pinned inputs in tools/input.

egui cannot switch OpenType features on at runtime, so:
- Inter Tabular = Inter with `tnum` frozen into the default digit glyphs.
- Instrument Sans ships as a variable font; egui gets static instances.

Needs fonttools==4.55.3 and opentype-feature-freezer==1.32.2. Run from the repo root:
    python3 tools/make-fonts.py
Input hashes are checked first, so a changed input fails instead of silently changing the output.
"""

import hashlib
import shutil
import subprocess
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parent.parent
INPUT = ROOT / "tools" / "input"
OUT = ROOT / "assets" / "fonts"

INPUTS = {
    "Inter-Regular.ttf": "40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82",
    "Inter-SemiBold.ttf": "78a843fade9d4612a5567302fb595b56976eb5fcebf4fea5a5912d638bafcde3",
    # google/fonts ofl/instrumentsans, fetched 2026-11-07.
    "InstrumentSans[wdth,wght].ttf": "b24f1812584816958afcf22e22d08e44318c5e51651e25d2438efdde389b33b1",
}


def check_inputs():
    for name, want in INPUTS.items():
        got = hashlib.sha256((INPUT / name).read_bytes()).hexdigest()
        if got != want:
            sys.exit(f"{name}: sha256 {got} != pinned {want}")


def set_names(font, family, style):
    name = font["name"]
    full = f"{family} {style}"
    ps = f"{family.replace(' ', '')}-{style}"
    for rec in list(name.names):
        if rec.nameID in (16, 17, 21, 22, 25):
            name.removeNames(nameID=rec.nameID)
    name.setName(family, 1, 3, 1, 0x409)
    name.setName(style, 2, 3, 1, 0x409)
    name.setName(full, 4, 3, 1, 0x409)
    name.setName(ps, 6, 3, 1, 0x409)


def clean(font):
    """Deterministic output: no build-time timestamp."""
    font["head"].modified = font["head"].created


def inter_tabular():
    for weight in ("Regular", "SemiBold"):
        src = INPUT / f"Inter-{weight}.ttf"
        tmp = OUT / f".InterTabular-{weight}.tmp.ttf"
        subprocess.run(
            [sys.executable, "-m", "opentype_feature_freezer.cli", "-f", "tnum", "-n", str(src), str(tmp)],
            check=True,
        )
        font = TTFont(tmp, recalcTimestamp=False)
        set_names(font, "Inter Tabular", weight)
        clean(font)
        font.save(OUT / f"InterTabular-{weight}.ttf", reorderTables=True)
        tmp.unlink()


def instrument_sans():
    for weight, wght in (("Regular", 400), ("SemiBold", 600)):
        font = TTFont(INPUT / "InstrumentSans[wdth,wght].ttf", recalcTimestamp=False)
        static = instancer.instantiateVariableFont(font, {"wght": wght, "wdth": 100}, updateFontNames=False)
        set_names(static, "Instrument Sans", weight)
        clean(static)
        static.save(OUT / f"InstrumentSans-{weight}.ttf", reorderTables=True)


def main():
    check_inputs()
    OUT.mkdir(parents=True, exist_ok=True)
    for weight in ("Regular", "SemiBold"):
        shutil.copyfile(INPUT / f"Inter-{weight}.ttf", OUT / f"Inter-{weight}.ttf")
    inter_tabular()
    instrument_sans()
    for path in sorted(OUT.glob("*.ttf")):
        print(hashlib.sha256(path.read_bytes()).hexdigest(), path.name)


if __name__ == "__main__":
    main()
