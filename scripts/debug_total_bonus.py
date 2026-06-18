# /// script
# requires-python = ">=3.12"
# dependencies = ["pillow"]
# ///
"""Headless calibration check for the total-score and bonus-score OCR regions.

Crops the stage TOTAL (big "X,XXX,XXXPt" number) and the BONUS badge
("+XXXXXX") for each of the three stages, preprocesses them, and runs the
embedded Tesseract the same way M2's recognize_single_number will, then prints
the read value. The region rectangles are read from config.json (the master);
override any of them on the command line for quick what-if checks.

Preprocessing matches scripts/region_tuner.py:
  total : white digits -> luminance threshold; whitelist digits + comma.
  bonus : light-blue digits preceded by a gold crown + "+" -> blue-selective
          mask (blue >= bmin AND blue-red >= margin) drops the crown/white;
          whitelist digits + "+", and the value is the digits after the last "+".

Usage (run from the repo root):

    uv run scripts/debug_total_bonus.py temp/failed_overlapped_samples/003_20260618_101738.png

    # override one stage's TOTAL or BONUS rect (x,y,w,h) for a quick test:
    uv run scripts/debug_total_bonus.py <png> --total2 0.29,0.388,0.40,0.035
    uv run scripts/debug_total_bonus.py <png> --bonus2 0.28,0.452,0.45,0.022

    # tweak preprocessing:
    uv run scripts/debug_total_bonus.py <png> --threshold 175 --bmin 150 --margin 30
"""

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

PROJECT_ROOT = Path(__file__).resolve().parent.parent
CONFIG = PROJECT_ROOT / "config.json"
TESS_EXE = PROJECT_ROOT / "target" / "release" / "tesseract" / "tesseract.exe"
TESS_DATA = PROJECT_ROOT / "target" / "release" / "tesseract" / "tessdata"

FALLBACK = {
    "total": [[0.25, 0.138, 0.50, 0.024], [0.25, 0.389, 0.50, 0.024], [0.25, 0.644, 0.50, 0.024]],
    "bonus": [[0.30, 0.197, 0.45, 0.022], [0.30, 0.448, 0.45, 0.022], [0.30, 0.703, 0.45, 0.022]],
}
# bonus_blue_min defaults to 190: at 150 the character icons' dimmer blue leaks digits.
PARAM_FALLBACK = {"threshold": 190, "bmin": 190, "margin": 30}


def _config():
    try:
        return json.loads(CONFIG.read_text(encoding="utf-8"))
    except Exception:
        return {}


def load_regions():
    """config.json is the master; fall back only when a region array is absent."""
    cfg = _config()

    def conv(key, fb):
        arr = cfg.get(key)
        if not isinstance(arr, list) or not arr:
            return [r[:] for r in fb]
        return [[r["x"], r["y"], r["width"], r["height"]] for r in arr]

    return {"total": conv("total_regions", FALLBACK["total"]),
            "bonus": conv("bonus_regions", FALLBACK["bonus"])}


def load_params():
    cfg = _config()
    return {
        "threshold": int(cfg.get("total_threshold", PARAM_FALLBACK["threshold"])),
        "bmin": int(cfg.get("bonus_blue_min", PARAM_FALLBACK["bmin"])),
        "margin": int(cfg.get("bonus_br_margin", PARAM_FALLBACK["margin"])),
    }


def parse_rect(s: str):
    parts = [float(p) for p in s.split(",")]
    if len(parts) != 4:
        raise argparse.ArgumentTypeError("rect must be x,y,w,h (4 comma-separated floats)")
    return parts


def to_pixels(rect, w, h):
    x, y, rw, rh = rect
    return (int(x * w), int(y * h), int((x + rw) * w), int((y + rh) * h))


def preprocess(crop, kind, threshold, bmin, margin):
    if kind == "bonus":
        b = crop.getchannel("B")
        r = crop.getchannel("R")
        diff = ImageChops.subtract(b, r)
        m_blue = b.point(lambda v: 255 if v >= bmin else 0)
        m_diff = diff.point(lambda v: 255 if v >= margin else 0)
        # text = blue glyph (white here); invert to black-on-white like Rust.
        return ImageChops.invert(ImageChops.darker(m_blue, m_diff))
    # Total: match Rust threshold_bright_pixels EXACTLY — a pixel is text only
    # when R>t AND G>t AND B>t (all-channels), NOT a luminance average. They
    # diverge near anti-aliased edges, which once caused the pipeline to read a
    # total differently from this tool (the 210 calibration was tuned on the
    # wrong, luminance, binarization). Output black text on white, as Rust feeds
    # Tesseract.
    r, g, b = crop.split()[:3]
    bright = lambda v: 255 if v > threshold else 0
    allch = ImageChops.darker(ImageChops.darker(r.point(bright), g.point(bright)), b.point(bright))
    return ImageChops.invert(allch)


def ocr(mask, kind):
    if not TESS_EXE.exists():
        return "(tesseract.exe missing)"
    whitelist = "0123456789," if kind == "total" else "0123456789+"
    with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as tf:
        tmp = Path(tf.name)
    try:
        mask.save(tmp)
        cmd = [str(TESS_EXE), str(tmp), "stdout", "--tessdata-dir", str(TESS_DATA),
               "-l", "eng", "--psm", "7", "-c", f"tessedit_char_whitelist={whitelist}"]
        return subprocess.run(cmd, capture_output=True, text=True).stdout.strip()
    finally:
        tmp.unlink(missing_ok=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p0 = load_params()  # defaults come from config.json (the master)
    ap.add_argument("screenshot")
    ap.add_argument("--threshold", type=int, default=p0["threshold"], help="total luminance cutoff")
    ap.add_argument("--bmin", type=int, default=p0["bmin"], help="bonus blue-channel minimum")
    ap.add_argument("--margin", type=int, default=p0["margin"], help="bonus (blue-red) minimum")
    for i in (1, 2, 3):
        ap.add_argument(f"--total{i}", type=parse_rect, default=None, help=f"override stage {i} total rect x,y,w,h")
        ap.add_argument(f"--bonus{i}", type=parse_rect, default=None, help=f"override stage {i} bonus rect x,y,w,h")
    args = ap.parse_args()

    src = Path(args.screenshot)
    if not src.exists():
        sys.exit(f"screenshot not found: {src}")
    if not TESS_EXE.exists():
        print(f"WARNING: {TESS_EXE} not found; build release first.", file=sys.stderr)

    regions = load_regions()
    totals = [getattr(args, f"total{i}") or regions["total"][i - 1] for i in (1, 2, 3)]
    bonuses = [getattr(args, f"bonus{i}") or regions["bonus"][i - 1] for i in (1, 2, 3)]

    debug_dir = PROJECT_ROOT / "debug"
    debug_dir.mkdir(exist_ok=True)
    img = Image.open(src).convert("RGB")
    w, h = img.size
    overlay = img.copy()
    draw = ImageDraw.Draw(overlay)
    print(f"image: {w}x{h}  threshold: {args.threshold}  bmin: {args.bmin}  margin: {args.margin}\n")

    def run(kind, rect, stage, color):
        box = to_pixels(rect, w, h)
        mask = preprocess(img.crop(box), kind, args.threshold, args.bmin, args.margin)
        mask.save(debug_dir / f"{kind}{stage}.png")
        draw.rectangle(box, outline=color, width=2)
        text = ocr(mask, kind)
        src_txt = text.split("+")[-1] if kind == "bonus" else text
        digits = "".join(c for c in src_txt if c.isdigit())
        print(f"  stage {stage} {kind:5s} rect={[round(v, 4) for v in rect]} box={box}")
        print(f"           OCR={text!r:18s} -> {digits or '(none)'}")

    for stage in (1, 2, 3):
        print(f"=== STAGE {stage} ===")
        run("total", totals[stage - 1], stage, (255, 80, 80))
        run("bonus", bonuses[stage - 1], stage, (80, 200, 80))
        print()

    overlay.save(debug_dir / "total_bonus_overlay.png")
    print(f"overlay + crops written to {debug_dir}")


if __name__ == "__main__":
    main()
