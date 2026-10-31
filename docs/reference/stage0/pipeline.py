"""Audioplay Cutter, Stage 0 mini test.
Fountain + per-role transcript JSON + room transcript JSON (+ audio for energy) -> takes, notes, picks.
"""
import json, re, sys
import numpy as np
from rapidfuzz import fuzz

SRC = "src"
ROLE = "PAUL"
FPS = 100  # envelope frames per second

# ---------------- fountain ----------------
SCENE_RE = re.compile(r"^(INT|EXT|EST|INT\./EXT|I/E)[\. ]", re.I)

def parse_fountain(path):
    txt = open(path, encoding="utf-8").read().replace("\r", "")
    if "===" in txt:
        txt = txt.split("===", 1)[1]  # drop title page
    blocks = re.split(r"\n\s*\n", txt)
    scenes, units = [], []
    cur = None
    for b in blocks:
        lines = [l.rstrip() for l in b.strip("\n").split("\n") if l.strip()]
        if not lines:
            continue
        first = lines[0].strip()
        if SCENE_RE.match(first):
            cur = {"id": f"S{len(scenes)+1}", "heading": first, "units": []}
            scenes.append(cur)
            lines = lines[1:]
            if not lines:
                continue
            first = lines[0].strip()
        if cur is None:
            continue
        cue = re.sub(r"\s*\(.*?\)\s*$", "", first).strip()
        is_cue = (len(lines) > 1 and first == first.upper() and re.search(r"[A-Z]", first)
                  and not first.startswith(("!", ">", "--")) and len(cue) < 30)
        if is_cue:
            char = cue.lstrip("@")
            paren = None
            buf = []
            def flush():
                if buf:
                    add(cur, units, "dialogue", char, " ".join(buf), paren)
            for l in lines[1:]:
                s = l.strip()
                if s.startswith("(") and s.endswith(")"):
                    flush(); buf.clear(); paren = s.strip("()")
                elif s in ("…", "..."):
                    flush(); buf.clear()
                else:
                    buf.append(s)
            flush()
        else:
            text = " ".join(l.strip().lstrip("!>-").strip() for l in lines)
            if text:
                add(cur, units, "action", None, text, None)
    return scenes, units

def add(scene, units, kind, char, text, paren):
    u = {"id": f"{scene['id']}.{len(scene['units'])+1}", "scene": scene["id"], "kind": kind,
         "char": char, "text": text, "paren": paren}
    u["norm"] = norm_words(text)
    scene["units"].append(u)
    units.append(u)

# ---------------- text normalization ----------------
def norm_word(w):
    w = w.lower().replace("’", "'")
    w = re.sub(r"[^a-z0-9']", "", w).replace("'", "")
    w = re.sub(r"(.)\1+", r"\1", w)  # collapse repeats: whoooaaah -> whoah
    return w

def norm_words(text):
    text = re.sub(r"[_*]", "", text)
    text = re.sub(r"[\-–—…]+", " ", text)
    return [w for w in (norm_word(x) for x in text.split()) if w]

# ---------------- transcripts ----------------
def tc(s):
    h, m, rest = s.split(":"); sec, ms = rest.split(",")
    return int(h) * 3600 + int(m) * 60 + int(sec) + int(ms) / 1000

def load_stream(path):
    segs = json.load(open(path))["lines"]
    words, out_segs = [], []
    for si, s in enumerate(segs):
        st, en = tc(s["startTime"]), tc(s["endTime"])
        raw = [w for w in re.sub(r"[\-–—…]+", " ", s["text"]).split() if norm_word(w)]
        out_segs.append({"i": si, "start": st, "end": en, "text": s["text"], "spk": s["speakerDesignation"], "w0": len(words)})
        if not raw:
            out_segs[-1]["w1"] = len(words); continue
        lens = np.array([len(w) + 1 for w in raw], float)
        edges = st + np.concatenate([[0], np.cumsum(lens)]) / lens.sum() * (en - st)
        for k, w in enumerate(raw):
            words.append({"w": norm_word(w), "raw": w, "t0": edges[k], "t1": edges[k + 1], "seg": si})
        out_segs[-1]["w1"] = len(words)
    return words, out_segs

# ---------------- alignment ----------------
def candidates(units, words, max_gap=1.6):
    cands = []
    W = len(words)
    seg_start = [k == 0 or words[k - 1]["seg"] != words[k]["seg"] for k in range(W)]
    seg_end = [k == W - 1 or words[k + 1]["seg"] != words[k]["seg"] for k in range(W)]
    for u in units:
        n = len(u["norm"])
        if n == 0:
            continue
        target = " ".join(u["norm"])
        thr = 88 if n <= 2 else (80 if n <= 4 else 72)
        for i in range(W):
            # quick prefilter: first or second word must be close to something in the line head
            for L in range(max(1, n - 3), n + 4):
                j = i + L
                if j > W:
                    break
                if any(words[k + 1]["t0"] - words[k]["t1"] > max_gap for k in range(i, j - 1)):
                    break
                if n <= 2:
                    whole = seg_start[i] and seg_end[j - 1]
                    lead = seg_start[i] and len(target) >= 4
                    if not (whole or lead):
                        continue
                cand = " ".join(w["w"] for w in words[i:j])
                sc = fuzz.ratio(target, cand)
                if sc >= thr:
                    pr = sc * (1 + 0.04 * min(n, 10))
                    cands.append((pr, sc, u["id"], i, j))
    return cands

def greedy(cands):
    cands.sort(key=lambda c: -c[0])
    used = set(); hits = []
    for pr, sc, uid, i, j in cands:
        if any(k in used for k in range(i, j)):
            continue
        used.update(range(i, j))
        hits.append({"unit": uid, "score": round(sc), "w0": i, "w1": j})
    return hits, used

# ---------------- audio energy ----------------
def envelope(raw_path, sr=8000):
    x = np.fromfile(raw_path, np.float32)
    hop = sr // FPS
    x = x[: len(x) // hop * hop].reshape(-1, hop)
    return np.sqrt((x ** 2).mean(1))

def refine(env, t0, t1, thr):
    """Snap clip edges to speech onset/offset in the role track."""
    a, b = int(t0 * FPS), int(t1 * FPS)
    n = len(env)
    # start: if already in speech, walk back to silence; else walk forward to onset
    s = a
    if env[min(s, n - 1)] > thr:
        lim = max(0, a - 50)
        while s > lim and env[s - 1] > thr * 0.5:
            s -= 1
    else:
        lim = min(n - 1, b)
        while s < lim and env[s] <= thr:
            s += 1
    e = b
    if env[min(e, n - 1)] > thr:
        lim = min(n - 1, b + 60)
        while e < lim and env[e] > thr * 0.5:
            e += 1
    else:
        lim = s + 10
        while e > lim and env[e - 1] <= thr:
            e -= 1
    return max(0, s / FPS - 0.08), e / FPS + 0.15

def rms_db(env, t0, t1):
    seg = env[int(t0 * FPS): int(t1 * FPS)]
    if len(seg) == 0:
        return -99
    v = np.sqrt((seg ** 2).mean())
    return float(20 * np.log10(v + 1e-9))

# ---------------- notes ----------------
STRONG = ["that was the one", "the one i liked", "right one", "we have the", "perfect", "definitely"]
WEAK = ["very good", "i like that", "i liked", "i like the", "nice", "awesome", "good"]
TAGS = {
    "quieter": ["quieter", "to yourself", "softer"], "louder/energy": ["oomph", "spice", "crazy", "excitement", "energ", "high"],
    "emphasis": ["emphasi", "emphasize"], "annoyed": ["annoyed", "annoying"], "excited": ["excited", "proud", "present"],
    "re-take": ["one more", "again", "a couple times", "from the top", "same thing"], "technical": ["pop", "malfunction", "hand in front", "couldn't hear"],
}

def summarize(texts):
    full = " ".join(texts).strip()
    low = full.lower()
    tags = [t for t, kws in TAGS.items() if any(k in low for k in kws)]
    sents = [s.strip() for s in re.split(r"(?<=[.?!])\s+", full) if len(s.split()) >= 3]
    key = ["give", "more", "like", "emphas", "quieter", "annoy", "excit", "one", "present", "not", "pop", "try"]
    best = max(sents, key=lambda s: sum(k in s.lower() for k in key), default=full)
    if len(best) > 140:
        best = best[:137].rsplit(" ", 1)[0] + "..."
    return best, tags

def pick_score(texts):
    low = " ".join(texts[:4]).lower()
    if any(k in low for k in STRONG):
        return 2
    if any(k in low for k in WEAK):
        return 1
    return 0

# ---------------- main ----------------
def main():
    scenes, units = parse_fountain(f"{SRC}/script.fountain")
    umap = {u["id"]: u for u in units}
    speakable = [u for u in units if u["kind"] == "dialogue"]
    actionish = [u for u in units if u["kind"] == "action"]

    # role track: align against all dialogue (other characters absorb bleed)
    pw, psegs = load_stream(f"{SRC}/paul.json")
    penv = envelope(f"{SRC}/paul.raw")
    thr = np.percentile(penv, 60) * 3
    pass1, _ = greedy(candidates([u for u in speakable if len(u["norm"]) >= 4], pw))
    active = {umap[h["unit"]]["scene"] for h in pass1 if umap[h["unit"]]["char"] == ROLE}
    order = [s["id"] for s in scenes]
    idx = {s: i for i, s in enumerate(order)}
    near = {order[j] for s in active for j in range(max(0, idx[s] - 1), min(len(order), idx[s] + 2))}
    speakable = [u for u in speakable if u["scene"] in near]
    actionish = [u for u in actionish if u["scene"] in near]
    phits, pused = greedy(candidates(speakable, pw))

    takes = []
    for h in phits:
        u = umap[h["unit"]]
        t0, t1 = pw[h["w0"]]["t0"], pw[h["w1"] - 1]["t1"]
        # use segment bounds when the hit covers whole segments
        s_first, s_last = psegs[pw[h["w0"]]["seg"]], psegs[pw[h["w1"] - 1]["seg"]]
        if h["w0"] == s_first["w0"]: t0 = s_first["start"]
        if h["w1"] == s_last["w1"]: t1 = s_last["end"]
        r0, r1 = refine(penv, t0, t1, thr)
        db = rms_db(penv, t0, t1)
        h.update({"t0": round(r0, 3), "t1": round(r1, 3), "db": round(db, 1),
                  "heard": " ".join(w["raw"] for w in pw[h["w0"]:h["w1"]]), "flub": h["score"] < 85})
        if u["char"] == ROLE:
            h["bleed"] = db < -38
            takes.append(h)
    takes.sort(key=lambda h: h["t0"])
    for a, b in zip(takes, takes[1:]):  # no overlaps between neighbouring clips
        if a["t1"] > b["t0"]:
            mid = round((pw[a["w1"] - 1]["t1"] + pw[b["w0"]]["t0"]) / 2, 3)
            a["t1"], b["t0"] = min(a["t1"], mid), max(b["t0"], mid)

    # room track: classify cue reading vs direction
    rw, rsegs = load_stream(f"{SRC}/room.json")
    renv = envelope(f"{SRC}/room.raw")
    rhits, rused = greedy(candidates(speakable + actionish, rw))
    cues = []
    for h in rhits:
        u = umap[h["unit"]]
        t0, t1 = rw[h["w0"]]["t0"], rw[h["w1"] - 1]["t1"]
        sf, sl = rsegs[rw[h["w0"]]["seg"]], rsegs[rw[h["w1"] - 1]["seg"]]
        if h["w0"] == sf["w0"]: t0 = sf["start"]
        if h["w1"] == sl["w1"]: t1 = sl["end"]
        h.update({"t0": round(t0 - 0.05, 3), "t1": round(t1 + 0.15, 3), "db": round(rms_db(renv, t0, t1), 1),
                  "heard": " ".join(w["raw"] for w in rw[h["w0"]:h["w1"]]), "char": u["char"], "kind": u["kind"]})
        cues.append(h)
    notes = []
    for s in rsegs:
        n = s["w1"] - s["w0"]
        if n == 0:
            continue
        covered = sum(1 for k in range(s["w0"], s["w1"]) if k in rused)
        if covered / n >= 0.6:
            continue
        # also drop segments that are the actor talking (overlap with a matched take)
        notes.append({"t0": s["start"], "t1": s["end"], "text": s["text"], "spk": s["spk"]})

    # link notes to takes: block between take k and k+1
    real = [t for t in takes if not t["bleed"]]
    for k, t in enumerate(real):
        t["take_index_global"] = k
    def block(a, b):
        return [n for n in notes if n["t0"] >= a - 0.3 and n["t0"] < b - 0.3]
    for k, t in enumerate(real):
        prev_end = real[k - 1]["t1"] if k > 0 else 0
        nxt_start = real[k + 1]["t0"] if k + 1 < len(real) else 1e9
        before = block(max(prev_end, t["t0"] - 30), t["t0"])
        after = block(t["t1"], min(nxt_start, t["t1"] + 8))
        during = [n for n in notes if n["t0"] >= t["t0"] + 0.3 and n["t0"] < t["t1"] - 0.3]
        t["note_before"], t["note_after"] = [n["text"] for n in before], [n["text"] for n in after]
        t["note_during"] = [n["text"] for n in during]
        t["pick"] = pick_score(t["note_after"])
        t["summary_after"], t["tags_after"] = summarize(t["note_after"]) if after else ("", [])
        t["summary_before"], t["tags_before"] = summarize(t["note_before"]) if before else ("", [])

    # per unit: take list + default
    by_unit = {}
    for t in real:
        by_unit.setdefault(t["unit"], []).append(t)
    for uid, lst in by_unit.items():
        lst.sort(key=lambda t: t["t0"])
        for i, t in enumerate(lst):
            t["take"] = i + 1
        best = max(lst, key=lambda t: (t["pick"], t["t0"]))
        for t in lst:
            t["default"] = t is best

    # unmatched loud role speech (ad-libs, chatter)
    unmatched = []
    for s in psegs:
        n = s["w1"] - s["w0"]
        if n == 0:
            continue
        cov = sum(1 for k in range(s["w0"], s["w1"]) if k in pused)
        db = rms_db(penv, s["start"], s["end"])
        if cov / n < 0.5 and db > -38:
            unmatched.append({"t0": s["start"], "t1": s["end"], "text": s["text"], "db": round(db, 1)})

    out = {"role": ROLE, "scenes": [{"id": s["id"], "heading": s["heading"], "units": [
        {k: u[k] for k in ("id", "kind", "char", "text", "paren")} for u in s["units"]]} for s in scenes],
        "takes": real, "bleed": [t for t in takes if t["bleed"]],
        "cues": [c for c in cues if c["kind"] == "dialogue"], "notes": notes, "unmatched": unmatched,
        "role_lines_total": sum(1 for u in units if u["char"] == ROLE)}
    json.dump(out, open("out/project.json", "w"), indent=1)
    report(out, umap)

def fmt(t):
    return f"{int(t//60):02d}:{t%60:05.2f}"

def report(out, umap):
    L = []
    L.append(f"# Match report: {out['role']}\n")
    hit_units = {t["unit"] for t in out["takes"]}
    L.append(f"- Takes found: {len(out['takes'])} on {len(hit_units)} script lines (role has {out['role_lines_total']} lines in the full episode)")
    L.append(f"- Dropped as bleed (quiet): {len(out['bleed'])}")
    L.append(f"- Room: {len(out['cues'])} cue readings, {len(out['notes'])} note segments")
    L.append(f"- Unmatched loud role speech: {len(out['unmatched'])}\n")
    for sc in out["scenes"]:
        rows = [u for u in sc["units"] if u["char"] == out["role"] and u["id"] in hit_units]
        if not rows:
            continue
        L.append(f"## {sc['id']} {sc['heading']}\n")
        for u in sc["units"]:
            if u["char"] != out["role"]:
                continue
            ts = [t for t in out["takes"] if t["unit"] == u["id"]]
            L.append(f"**{u['id']}** {u['text']}  ")
            if not ts:
                L.append("  - (no takes)\n"); continue
            for t in ts:
                flag = " DEFAULT" if t["default"] else ""
                pk = {2: " PICK", 1: " liked", 0: ""}[t["pick"]]
                L.append(f"  - T{t['take']} {fmt(t['t0'])}-{fmt(t['t1'])} score {t['score']}{flag}{pk} | heard: {t['heard']}")
                if t["summary_after"]:
                    L.append(f"    - after: {t['summary_after']} {t['tags_after']}")
            L.append("")
    L.append("## Unmatched loud role speech\n")
    for u in out["unmatched"]:
        L.append(f"- {fmt(u['t0'])} {u['text']}")
    open("out/report.md", "w").write("\n".join(L))

if __name__ == "__main__":
    main()
