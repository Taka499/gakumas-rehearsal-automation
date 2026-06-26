"""Interactive browser tool to calibrate the OCR regions and the review crop.

Starts a tiny local web server. Open the printed URL, pick a sample
screenshot, then drag/resize the six OCR rectangles (3 stage TOTALs in red,
3 BONUS badges in green). Each adjustment runs the embedded Tesseract
server-side exactly the way recognize_single_number does (single line
--psm 7, digit whitelist) and shows the read value plus a preview of the
thresholded crop Tesseract actually sees. Copy the resulting JSON straight
into config.json's total_regions / bonus_regions.

It also tunes the REVIEW CROP shown inline in the GUI's review window: the
three purple rectangles are derived live from score_regions plus a single
shared ReviewCropAdjust (top/bottom extend, left/right inset). Drag any one
(or use the four sliders) to frame each stage's character portraits + printed
scores; all three move together because the adjust is shared. A live thumbnail
under each shows exactly what the review window will render. Copy the
review_crop_adjust JSON into config.json.

Dependencies (pillow) are managed by the project's pyproject.toml/uv.lock; uv
syncs the .venv automatically. Run from the repo root:

    uv run python scripts/region_tuner.py
    # then open http://127.0.0.1:8777 in a browser

Nothing leaves your machine; it binds to localhost only.
"""

import base64
import io
import json
import socketserver
import subprocess
import sys
import tempfile
from http.server import BaseHTTPRequestHandler
from pathlib import Path
from urllib.parse import urlparse, parse_qs

from PIL import Image, ImageChops

PROJECT_ROOT = Path(__file__).resolve().parent.parent
SAMPLE_DIR = PROJECT_ROOT / "temp" / "failed_overlapped_samples"
PORT = 8777

TESS_EXE = PROJECT_ROOT / "target" / "release" / "tesseract" / "tesseract.exe"
TESS_DATA = PROJECT_ROOT / "target" / "release" / "tesseract" / "tessdata"

CONFIG = PROJECT_ROOT / "config.json"

# Used only if config.json is missing the regions (normalized x,y,w,h, stage 1/2/3).
FALLBACK = {
    "total": [[0.25, 0.138, 0.50, 0.024], [0.25, 0.389, 0.50, 0.024], [0.25, 0.644, 0.50, 0.024]],
    "bonus": [[0.30, 0.197, 0.45, 0.022], [0.30, 0.448, 0.45, 0.022], [0.30, 0.703, 0.45, 0.022]],
}
# score_regions fallback — must mirror default_score_regions() in
# src/automation/config.rs (the review crop is derived from these).
SCORE_FALLBACK = [[0.0, 0.179, 1.0, 0.022], [0.0, 0.430, 1.0, 0.022], [0.0, 0.685, 1.0, 0.022]]
# ReviewCropAdjust fallback — mirrors ReviewCropAdjust::default() in config.rs.
REVIEW_ADJUST_FALLBACK = {"top_extend": 0.05, "bottom_extend": 0.0, "left_inset": 0.0, "right_inset": 0.22}
# Preprocessing fallbacks. bonus_blue_min defaults to 190 (not 150): the character
# icons contain a dimmer blue that, at 150, leaks extra digits into the bonus read.
PARAM_FALLBACK = {"threshold": 190, "bmin": 190, "margin": 30}


def load_regions():
    """config.json is the master. Read total_regions/bonus_regions from it,
    falling back to FALLBACK only when absent. Objects {x,y,width,height} are
    converted to the [x,y,w,h] lists the UI uses."""
    try:
        cfg = json.loads(CONFIG.read_text(encoding="utf-8"))
    except Exception:
        return {k: [r[:] for r in v] for k, v in FALLBACK.items()}

    def conv(key, fb):
        arr = cfg.get(key)
        if not isinstance(arr, list) or not arr:
            return [r[:] for r in fb]
        return [[r["x"], r["y"], r["width"], r["height"]] for r in arr]

    return {"total": conv("total_regions", FALLBACK["total"]),
            "bonus": conv("bonus_regions", FALLBACK["bonus"]),
            "score": conv("score_regions", SCORE_FALLBACK)}


def load_review_adjust():
    """ReviewCropAdjust from config.json (the master), falling back to
    REVIEW_ADJUST_FALLBACK when absent."""
    try:
        cfg = json.loads(CONFIG.read_text(encoding="utf-8"))
        a = cfg.get("review_crop_adjust") or {}
    except Exception:
        a = {}
    return {k: float(a.get(k, v)) for k, v in REVIEW_ADJUST_FALLBACK.items()}


def load_params():
    """Preprocessing knobs from config.json (the master): total_threshold,
    bonus_blue_min, bonus_br_margin. Falls back to PARAM_FALLBACK when absent."""
    try:
        cfg = json.loads(CONFIG.read_text(encoding="utf-8"))
    except Exception:
        return dict(PARAM_FALLBACK)
    return {
        "threshold": int(cfg.get("total_threshold", PARAM_FALLBACK["threshold"])),
        "bmin": int(cfg.get("bonus_blue_min", PARAM_FALLBACK["bmin"])),
        "margin": int(cfg.get("bonus_br_margin", PARAM_FALLBACK["margin"])),
    }


def list_samples():
    out = []
    if SAMPLE_DIR.is_dir():
        out += [str(p.relative_to(PROJECT_ROOT)).replace("\\", "/") for p in sorted(SAMPLE_DIR.glob("*.png"))]
    return out


def preprocess(crop, kind, params):
    """Binarize a crop for OCR.

    total: white digits on a dark badge -> plain luminance threshold.
    bonus: the value is LIGHT BLUE and is preceded by a GOLD crown icon and a
    '+'. A blue-selective mask (blue channel high AND blue clearly greater than
    red) keeps the light-blue glyphs while dropping the gold crown and any white,
    so the crop width / which-of-three-columns the badge sits in no longer matters.
    """
    if kind == "bonus":
        b = crop.getchannel("B")
        r = crop.getchannel("R")
        diff = ImageChops.subtract(b, r)  # max(b - r, 0)
        bmin = params.get("bmin", 190)
        margin = params.get("margin", 30)
        m_blue = b.point(lambda v: 255 if v >= bmin else 0)
        m_diff = diff.point(lambda v: 255 if v >= margin else 0)
        return ImageChops.darker(m_blue, m_diff)  # logical AND of the two masks
    thr = params.get("threshold", 190)
    return crop.convert("L").point(lambda p: 255 if p >= thr else 0)


def run_ocr(path: Path, rect, kind: str, params: dict):
    img = Image.open(path).convert("RGB")
    w, h = img.size
    x, y, rw, rh = rect
    box = (int(x * w), int(y * h), int((x + rw) * w), int((y + rh) * h))
    mask = preprocess(img.crop(box), kind, params)

    buf = io.BytesIO()
    mask.save(buf, format="PNG")
    preview = "data:image/png;base64," + base64.b64encode(buf.getvalue()).decode()

    # Allow '+' for bonus so the crown noise (whatever it reads as) lands before
    # the '+' and is discarded; the real value is the digits after the last '+'.
    whitelist = "0123456789," if kind == "total" else "0123456789+"
    text = ""
    if TESS_EXE.exists():
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as tf:
            tmp = Path(tf.name)
        try:
            mask.save(tmp)
            cmd = [str(TESS_EXE), str(tmp), "stdout", "--tessdata-dir", str(TESS_DATA),
                   "-l", "eng", "--psm", "7", "-c", f"tessedit_char_whitelist={whitelist}"]
            res = subprocess.run(cmd, capture_output=True, text=True)
            text = res.stdout.strip()
        finally:
            tmp.unlink(missing_ok=True)
    else:
        text = "(tesseract.exe missing)"

    src = text.split("+")[-1] if kind == "bonus" else text
    digits = "".join(c for c in src if c.isdigit())
    return {"text": text, "digits": digits, "box": box, "preview": preview}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _send(self, code, body, ctype="application/json"):
        if isinstance(body, (dict, list)):
            body = json.dumps(body).encode()
        elif isinstance(body, str):
            body = body.encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        u = urlparse(self.path)
        if u.path == "/":
            self._send(200, HTML, "text/html; charset=utf-8")
        elif u.path == "/samples":
            self._send(200, {"samples": list_samples(), "defaults": load_regions(),
                             "params": load_params(), "review_adjust": load_review_adjust()})
        elif u.path == "/image":
            q = parse_qs(u.query)
            rel = q.get("path", [""])[0]
            p = (PROJECT_ROOT / rel).resolve()
            if not p.exists():
                self._send(404, {"error": "not found"})
                return
            data = p.read_bytes()
            self._send(200, data, "image/png")
        else:
            self._send(404, {"error": "unknown"})

    def do_POST(self):
        if urlparse(self.path).path != "/ocr":
            self._send(404, {"error": "unknown"})
            return
        n = int(self.headers.get("Content-Length", 0))
        req = json.loads(self.rfile.read(n) or b"{}")
        p = (PROJECT_ROOT / req["path"]).resolve()
        if not p.exists():
            self._send(404, {"error": "image not found"})
            return
        params = {
            "threshold": int(req.get("threshold", 190)),
            "bmin": int(req.get("bmin", 190)),
            "margin": int(req.get("margin", 30)),
        }
        try:
            self._send(200, run_ocr(p, req["rect"], req.get("kind", "total"), params))
        except Exception as e:  # noqa: BLE001
            self._send(500, {"error": str(e)})


HTML = r"""<!doctype html>
<html><head><meta charset="utf-8"><title>OCR Region Tuner</title>
<style>
  :root { --bg:#1e1f22; --panel:#2b2d31; --txt:#e3e5e8; --muted:#9aa0a6; --red:#ff5555; --green:#4ec97a; --purple:#b083f0; }
  * { box-sizing:border-box; }
  body { margin:0; font:13px/1.4 system-ui,sans-serif; background:var(--bg); color:var(--txt); display:flex; }
  #stage { padding:12px; }
  #wrap { position:relative; display:inline-block; user-select:none; }
  #wrap img { display:block; }
  .rgn { position:absolute; border:2px solid; cursor:move; }
  .rgn.total { border-color:var(--red); background:rgba(255,85,85,.10); }
  .rgn.bonus { border-color:var(--green); background:rgba(78,201,122,.10); }
  .rgn.review { border-color:var(--purple); border-style:dashed; background:rgba(176,131,240,.08); }
  .rgn.active { box-shadow:0 0 0 2px #fff inset; }
  .rgn .lbl { position:absolute; top:-16px; left:-2px; font-size:10px; padding:0 3px; color:#fff; white-space:nowrap; }
  .rgn.total .lbl { background:var(--red); } .rgn.bonus .lbl { background:var(--green); } .rgn.review .lbl { background:var(--purple); }
  .crops { display:flex; gap:6px; margin:6px 0; }
  .crops canvas { background:#000; border:1px solid #444; flex:1; min-width:0; height:auto; }
  .srow { display:flex; align-items:center; gap:8px; margin:6px 0; }
  .srow span.lab { width:104px; color:var(--purple); }
  .srow input[type=range] { flex:1; }
  .srow b { width:52px; text-align:right; font-family:monospace; }
  .rgn .h { position:absolute; right:-5px; bottom:-5px; width:11px; height:11px; background:#fff; border-radius:2px; cursor:nwse-resize; }
  #side { width:474px; min-width:474px; height:100vh; overflow:auto; background:var(--panel); padding:12px; }
  h3 { margin:6px 0; font-size:13px; }
  select,input[type=text]{ width:100%; background:#1e1f22; color:var(--txt); border:1px solid #444; border-radius:4px; padding:5px; }
  .row { display:flex; flex-wrap:wrap; align-items:center; gap:6px; padding:6px; border-radius:6px; margin-bottom:6px; background:#1e1f22; cursor:pointer; }
  .row.active { outline:1px solid #fff; }
  .row .tag { width:58px; font-weight:600; }
  .row.total .tag { color:var(--red); } .row.bonus .tag { color:var(--green); }
  .row .f { display:flex; align-items:center; gap:3px; }
  .row .f i { color:var(--muted); font-style:normal; font-size:10px; }
  .row .f input { width:62px; background:#111; color:var(--txt); border:1px solid #444; border-radius:3px; padding:2px 4px; font-family:monospace; cursor:text; }
  .read { font-family:monospace; min-width:96px; }
  .read b { color:#ffd866; }
  .prev { height:22px; background:#000; border:1px solid #444; image-rendering:pixelated; }
  .muted { color:var(--muted); }
  #thrwrap { display:flex; align-items:center; gap:8px; margin:8px 0; }
  #out { width:100%; height:150px; background:#111; color:#9ee; border:1px solid #444; border-radius:4px; font-family:monospace; font-size:11px; }
  button { background:#3a7afe; color:#fff; border:0; border-radius:5px; padding:6px 10px; cursor:pointer; }
  button.sec { background:#444; }
</style></head>
<body>
<div id="stage"><div id="wrap"><img id="img" src="" alt=""><div id="rects"></div></div></div>
<div id="side">
  <h3>Screenshot</h3>
  <select id="sample"></select>
  <input type="text" id="custom" placeholder="...or custom path relative to repo root, then Enter">
  <div id="thrwrap"><span style="width:120px">total threshold</span><input type="range" id="thr" min="100" max="255" value="190" style="flex:1"><b id="thrval">190</b></div>
  <div id="thrwrap"><span style="width:120px" class="muted">bonus blue-min</span><input type="range" id="bmin" min="100" max="255" value="190" style="flex:1"><b id="bminval">190</b></div>
  <div id="thrwrap"><span style="width:120px" class="muted">bonus B−R margin</span><input type="range" id="bmargin" min="0" max="120" value="30" style="flex:1"><b id="bmarginval">30</b></div>
  <div style="margin-bottom:8px"><button id="ocrall">OCR all</button> <button class="sec" id="reset">Reset rects</button></div>
  <h3>OCR regions <span class="muted">(drag to move, corner to resize)</span></h3>
  <div id="rows"></div>
  <h3>Review crop <span class="muted">(purple — portraits + scores shown in the GUI)</span></h3>
  <div class="srow"><span class="lab">top extend</span><input type="range" id="rtop" min="0" max="0.20" step="0.005" value="0.05"><b id="rtopval">0.050</b></div>
  <div class="srow"><span class="lab">bottom extend</span><input type="range" id="rbot" min="0" max="0.10" step="0.005" value="0.00"><b id="rbotval">0.000</b></div>
  <div class="srow"><span class="lab">left inset</span><input type="range" id="rleft" min="-0.10" max="0.40" step="0.005" value="0.00"><b id="rleftval">0.000</b></div>
  <div class="srow"><span class="lab">right inset</span><input type="range" id="rright" min="-0.10" max="0.50" step="0.005" value="0.22"><b id="rrightval">0.220</b></div>
  <div class="crops"><canvas id="crop0"></canvas><canvas id="crop1"></canvas><canvas id="crop2"></canvas></div>
  <h3>config.json snippet</h3>
  <textarea id="out" readonly></textarea>
  <div style="margin-top:6px"><button id="copy">Copy JSON</button></div>
</div>
<script>
const KINDS=["total","bonus"];
const img=document.getElementById("img"), rectsEl=document.getElementById("rects"), rowsEl=document.getElementById("rows");
let state={total:[],bonus:[]}, active=null, path="", thr=190, bmin=150, bmargin=30;
let rectDivs={}, inputs={}, reads={}, prevs={}, rows={}, timers={};
// Review crop: 3 score_regions (reference) + one shared adjust → 3 derived crops.
let score=[], adj={top_extend:0.05,bottom_extend:0.0,left_inset:0.0,right_inset:0.22}, reviewDivs=[];
const key=(k,i)=>k+i;
const fmt=v=>Number(v).toFixed(4);
const fmt3=v=>Number(v).toFixed(3);
const clamp=v=>Math.max(0,Math.min(1,v));

// Derive stage i's review crop [x,y,w,h] from its score region + the shared
// adjust, clamped into [0,1] — mirrors review_crop_rect() in config.rs.
function reviewRect(i){
  const s=score[i]; if(!s) return [0,0,0,0];
  const x0=clamp(s[0]+adj.left_inset), y0=clamp(s[1]-adj.top_extend);
  const x1=clamp(s[0]+s[2]-adj.right_inset), y1=clamp(s[1]+s[3]+adj.bottom_extend);
  return [x0,y0,Math.max(0,x1-x0),Math.max(0,y1-y0)];
}
function applyAdjust(a){
  adj={top_extend:+a.top_extend,bottom_extend:+a.bottom_extend,left_inset:+a.left_inset,right_inset:+a.right_inset};
  const set=(id,v)=>{document.getElementById(id).value=v; document.getElementById(id+"val").textContent=fmt3(v);};
  set("rtop",adj.top_extend); set("rbot",adj.bottom_extend); set("rleft",adj.left_inset); set("rright",adj.right_inset);
}

async function boot(){
  const r=await (await fetch("/samples")).json();
  const sel=document.getElementById("sample"); sel.innerHTML="";
  r.samples.forEach(s=>{const o=document.createElement("option");o.value=s;o.textContent=s;sel.appendChild(o);});
  state=JSON.parse(JSON.stringify(r.defaults));
  score=JSON.parse(JSON.stringify(r.defaults.score||[]));
  if(r.params) applyParams(r.params);
  if(r.review_adjust) applyAdjust(r.review_adjust);
  sel.onchange=()=>load(sel.value);
  document.getElementById("custom").onkeydown=e=>{if(e.key==="Enter")load(e.target.value.trim());};
  if(r.samples.length) load(r.samples[0]);
}
function applyParams(p){
  thr=p.threshold; bmin=p.bmin; bmargin=p.margin;
  const set=(id,v)=>{document.getElementById(id).value=v; document.getElementById(id+"val").textContent=v;};
  set("thr",thr); set("bmin",bmin); set("bmargin",bmargin);
}
function load(p){ path=p; img.onload=()=>{build();layout();ocrAll();}; img.src="/image?path="+encodeURIComponent(p)+"&t="+Date.now(); }

// Build rect overlays + sidebar rows ONCE per image; later changes only reposition.
function build(){
  rectsEl.innerHTML=""; rowsEl.innerHTML=""; rectDivs={}; inputs={}; reads={}; prevs={}; rows={};
  KINDS.forEach(kind=>state[kind].forEach((r,i)=>{
    const d=document.createElement("div"); d.className="rgn "+kind;
    d.innerHTML='<span class="lbl">'+kind[0].toUpperCase()+(i+1)+'</span><span class="h"></span>';
    rectsEl.appendChild(d); rectDivs[key(kind,i)]=d; bindDrag(d,kind,i);

    const row=document.createElement("div"); row.className="row "+kind;
    row.onclick=()=>{ active={kind,idx:i}; layout(); };
    const tag=document.createElement("span"); tag.className="tag"; tag.textContent=kind+" "+(i+1); row.appendChild(tag);
    inputs[key(kind,i)]=[];
    ["x","y","w","h"].forEach((lab,k)=>{
      const f=document.createElement("span"); f.className="f";
      const t=document.createElement("i"); t.textContent=lab; f.appendChild(t);
      const inp=document.createElement("input");
      inp.type="number"; inp.step="0.001"; inp.min="0"; inp.max="1"; inp.value=fmt(r[k]);
      inp.onclick=e=>e.stopPropagation();              // don't let the row's click steal focus
      inp.oninput=()=>{ const v=parseFloat(inp.value); if(isNaN(v))return;
        state[kind][i][k]=clamp(v); active={kind,idx:i}; layout(); ocrDebounced(kind,i); };
      f.appendChild(inp); row.appendChild(f); inputs[key(kind,i)].push(inp);
    });
    const read=document.createElement("span"); read.className="read"; read.innerHTML="<span class='muted'>…</span>";
    row.appendChild(read); reads[key(kind,i)]=read;
    const pv=document.createElement("img"); pv.className="prev"; row.appendChild(pv); prevs[key(kind,i)]=pv;
    rowsEl.appendChild(row); rows[key(kind,i)]=row;
  }));
  // Review-crop overlays (derived, purple). Dragging any one edits the shared
  // adjust, so all three move together.
  reviewDivs=[];
  for(let i=0;i<score.length;i++){
    const d=document.createElement("div"); d.className="rgn review";
    d.innerHTML='<span class="lbl">R'+(i+1)+'</span><span class="h"></span>';
    rectsEl.appendChild(d); reviewDivs[i]=d; bindReviewDrag(d,i);
  }
}

// Reposition rects from state and refresh active highlight + JSON output. No DOM rebuild.
function layout(){
  const W=img.clientWidth, H=img.clientHeight;
  KINDS.forEach(kind=>state[kind].forEach((r,i)=>{
    const d=rectDivs[key(kind,i)]; if(!d) return;
    d.style.left=(r[0]*W)+"px"; d.style.top=(r[1]*H)+"px"; d.style.width=(r[2]*W)+"px"; d.style.height=(r[3]*H)+"px";
    const on=!!(active&&active.kind===kind&&active.idx===i);
    d.classList.toggle("active",on); rows[key(kind,i)].classList.toggle("active",on);
  }));
  reviewDivs.forEach((d,i)=>{
    const r=reviewRect(i);
    d.style.left=(r[0]*W)+"px"; d.style.top=(r[1]*H)+"px"; d.style.width=(r[2]*W)+"px"; d.style.height=(r[3]*H)+"px";
  });
  drawCrops();
  updateOut();
}

// Render each derived review crop into its thumbnail canvas, exactly the sub-
// region the GUI's review window will draw, at the crop's native aspect.
function drawCrops(){
  const nW=img.naturalWidth, nH=img.naturalHeight; if(!nW) return;
  for(let i=0;i<3;i++){
    const cv=document.getElementById("crop"+i); if(!cv) continue;
    const r=score[i]?reviewRect(i):[0,0,0,0];
    const cw=cv.clientWidth||140; const aspect=(r[2]>0)?(r[3]*nH)/(r[2]*nW):0.2;
    cv.width=cw; cv.height=Math.max(8,Math.round(cw*aspect));
    const ctx=cv.getContext("2d"); ctx.clearRect(0,0,cv.width,cv.height);
    if(r[2]<=0||r[3]<=0) continue;
    ctx.drawImage(img, r[0]*nW, r[1]*nH, r[2]*nW, r[3]*nH, 0,0, cv.width, cv.height);
  }
}
// Push state values back into the numeric inputs (used during drag; skips the field being typed in).
function syncInputs(kind,i){ const ins=inputs[key(kind,i)]; if(!ins) return;
  state[kind][i].forEach((v,k)=>{ if(document.activeElement!==ins[k]) ins[k].value=fmt(v); }); }

function bindDrag(d,kind,i){
  d.onmousedown=e=>{ if(e.target.classList.contains("h")) return; e.preventDefault(); active={kind,idx:i};
    const W=img.clientWidth,H=img.clientHeight, sx=e.clientX,sy=e.clientY, r=state[kind][i].slice();
    const mv=ev=>{ state[kind][i][0]=Math.max(0,Math.min(1-r[2], r[0]+(ev.clientX-sx)/W));
      state[kind][i][1]=Math.max(0,Math.min(1-r[3], r[1]+(ev.clientY-sy)/H)); layout(); syncInputs(kind,i); };
    const up=()=>{document.removeEventListener("mousemove",mv);document.removeEventListener("mouseup",up);ocr(kind,i);};
    document.addEventListener("mousemove",mv);document.addEventListener("mouseup",up);
  };
  d.querySelector(".h").onmousedown=e=>{ e.preventDefault();e.stopPropagation(); active={kind,idx:i};
    const W=img.clientWidth,H=img.clientHeight, sx=e.clientX,sy=e.clientY, r=state[kind][i].slice();
    const mv=ev=>{ state[kind][i][2]=Math.max(0.02,Math.min(1-r[0], r[2]+(ev.clientX-sx)/W));
      state[kind][i][3]=Math.max(0.008,Math.min(1-r[1], r[3]+(ev.clientY-sy)/H)); layout(); syncInputs(kind,i); };
    const up=()=>{document.removeEventListener("mousemove",mv);document.removeEventListener("mouseup",up);ocr(kind,i);};
    document.addEventListener("mousemove",mv);document.addEventListener("mouseup",up);
  };
}

// Dragging review crop i edits the SHARED adjust (so all three move):
//   move  → translate: left+=dx, right-=dx, top-=dy, bottom+=dy (size kept)
//   corner→ resize the far edges: right-=dx, bottom+=dy
function bindReviewDrag(d,i){
  d.onmousedown=e=>{ if(e.target.classList.contains("h")) return; e.preventDefault();
    const W=img.clientWidth,H=img.clientHeight, sx=e.clientX,sy=e.clientY, a0={...adj};
    const mv=ev=>{ const dx=(ev.clientX-sx)/W, dy=(ev.clientY-sy)/H;
      adj.left_inset=a0.left_inset+dx; adj.right_inset=a0.right_inset-dx;
      adj.top_extend=a0.top_extend-dy; adj.bottom_extend=a0.bottom_extend+dy;
      applyAdjust(adj); layout(); };
    const up=()=>{document.removeEventListener("mousemove",mv);document.removeEventListener("mouseup",up);};
    document.addEventListener("mousemove",mv);document.addEventListener("mouseup",up);
  };
  d.querySelector(".h").onmousedown=e=>{ e.preventDefault();e.stopPropagation();
    const W=img.clientWidth,H=img.clientHeight, sx=e.clientX,sy=e.clientY, a0={...adj};
    const mv=ev=>{ const dx=(ev.clientX-sx)/W, dy=(ev.clientY-sy)/H;
      adj.right_inset=a0.right_inset-dx; adj.bottom_extend=a0.bottom_extend+dy;
      applyAdjust(adj); layout(); };
    const up=()=>{document.removeEventListener("mousemove",mv);document.removeEventListener("mouseup",up);};
    document.addEventListener("mousemove",mv);document.addEventListener("mouseup",up);
  };
}
function bindAdjSlider(id,key){
  const el=document.getElementById(id);
  el.oninput=e=>{ adj[key]=+e.target.value; document.getElementById(id+"val").textContent=fmt3(adj[key]); layout(); };
}
bindAdjSlider("rtop","top_extend"); bindAdjSlider("rbot","bottom_extend");
bindAdjSlider("rleft","left_inset"); bindAdjSlider("rright","right_inset");

function ocrDebounced(kind,i){ clearTimeout(timers[key(kind,i)]); timers[key(kind,i)]=setTimeout(()=>ocr(kind,i),200); }
async function ocr(kind,i){
  if(!path) return;
  const res=await (await fetch("/ocr",{method:"POST",headers:{"Content-Type":"application/json"},
    body:JSON.stringify({path,kind,threshold:thr,bmin,margin:bmargin,rect:state[kind][i]})})).json();
  const read=reads[key(kind,i)]; if(read) read.innerHTML="<b>"+(res.digits||"—")+"</b> <span class='muted'>"+(res.text||"")+"</span>";
  const pv=prevs[key(kind,i)]; if(pv&&res.preview) pv.src=res.preview;
}
function ocrAll(){ KINDS.forEach(k=>state[k].forEach((_,i)=>ocr(k,i))); }
function ocrKind(k){ state[k].forEach((_,i)=>ocr(k,i)); }
function updateOut(){
  const j={total_regions:state.total.map(r=>({x:+fmt(r[0]),y:+fmt(r[1]),width:+fmt(r[2]),height:+fmt(r[3])})),
           bonus_regions:state.bonus.map(r=>({x:+fmt(r[0]),y:+fmt(r[1]),width:+fmt(r[2]),height:+fmt(r[3])})),
           review_crop_adjust:{top_extend:+fmt3(adj.top_extend),bottom_extend:+fmt3(adj.bottom_extend),
                               left_inset:+fmt3(adj.left_inset),right_inset:+fmt3(adj.right_inset)}};
  document.getElementById("out").value=JSON.stringify(j,null,2);
}
document.getElementById("thr").oninput=e=>{thr=+e.target.value;document.getElementById("thrval").textContent=thr;};
document.getElementById("thr").onchange=()=>ocrKind("total");
document.getElementById("bmin").oninput=e=>{bmin=+e.target.value;document.getElementById("bminval").textContent=bmin;};
document.getElementById("bmin").onchange=()=>ocrKind("bonus");
document.getElementById("bmargin").oninput=e=>{bmargin=+e.target.value;document.getElementById("bmarginval").textContent=bmargin;};
document.getElementById("bmargin").onchange=()=>ocrKind("bonus");
document.getElementById("ocrall").onclick=ocrAll;
document.getElementById("reset").onclick=async()=>{const r=await (await fetch("/samples")).json();
  state=JSON.parse(JSON.stringify(r.defaults)); score=JSON.parse(JSON.stringify(r.defaults.score||[]));
  if(r.params) applyParams(r.params); if(r.review_adjust) applyAdjust(r.review_adjust);
  build(); layout(); ocrAll();};
document.getElementById("copy").onclick=()=>navigator.clipboard.writeText(document.getElementById("out").value);
window.addEventListener("resize", layout);
boot();
</script>
</body></html>"""


def main():
    if not TESS_EXE.exists():
        print(f"WARNING: {TESS_EXE} not found; build release first or OCR will be disabled.", file=sys.stderr)
    socketserver.TCPServer.allow_reuse_address = True
    with socketserver.TCPServer(("127.0.0.1", PORT), Handler) as httpd:
        print(f"Region tuner running at http://127.0.0.1:{PORT}  (Ctrl+C to stop)")
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print("\nstopped.")


if __name__ == "__main__":
    main()
