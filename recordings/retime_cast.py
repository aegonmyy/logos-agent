#!/usr/bin/env python3
"""Re-time an asciinema v2 .cast so the meaningful lines stay on screen long
enough to narrate over, and the 9h of compile/proof dead air disappears.

Strategy: keep every event in order, but assign each event a dwell based on what
it shows. Banners and results hold ~4s; ordinary output ~1.1s; progress-bar
churn (the [====> ] 1017/1020 lines) collapses to 0.05s so the build flicker is
gone but the line still renders. Idle gaps are dropped entirely (we drive purely
off per-event dwell, not the original wall clock).

The output is a valid asciinema v2 cast that any renderer (agg) plays back at
narration pace. We also print a chapter list (event index + elapsed seconds +
first line) for the scenario banners so the narrator knows where to talk.
"""
import json, sys, re

SRC = "/home/ubuntu/logos-agent/recordings/logos-agent-real-proof.cast"
DST = "/home/ubuntu/logos-agent/recordings/logos-agent-narratable.cast"

PROG = re.compile(r"\[1m\[(9[0-9])?m?\s*(Building|Compiling|Finished)\b")
PROG2 = re.compile(r"\[\s*=+>\s*\]\s*\d+/\d+:")

def dwell(out: str) -> float:
    s = out
    # progress-bar / build-status churn: collapse but keep
    if PROG.search(s) or PROG2.search(s):
        return 0.04
    low = s.lower()
    # scenario banners and the demo title banner: hold long
    if "=====" in s and ("scenario" in low or "logos autonomous agent" in low or "demo" in low):
        return 4.5
    # results and anchors
    if any(k in low for k in ("=ok", "approved=", "final_balance", "tx=0x", "block=", "public_anchor", "testnet", "1779", "pass", "fail", "demo complete")):
        return 3.5
    # the proof-mode command line itself
    if "risc0_dev_mode=0" in low:
        return 4.0
    # a full visible line (has a newline) gets a beat
    if "\n" in s or "\r" in s:
        return 1.1
    # mid-line fragments (typing, escape sequences): quick
    return 0.12

events = []
with open(SRC) as f:
    header = json.loads(next(f))
    for line in f:
        try:
            t, typ, out = json.loads(line)
            if typ == "o":
                events.append(out)
        except Exception:
            continue

# emit retimed cast
with open(DST, "w") as f:
    # Keep the idle cap ABOVE the max per-event dwell (4.5s) so the renderer
    # honors the narration pacing instead of collapsing it to the cap.
    header["idle_time_limit"] = 10.0
    header["timestamp"] = 1787005070
    f.write(json.dumps(header) + "\n")
    t = 0.0
    chapters = []
    for i, out in enumerate(events):
        low = out.lower()
        if ("=====" in out and ("scenario" in low or "logos autonomous agent" in low)) or "risc0_dev_mode=0" in low or "demo complete" in low:
            chapters.append((t, out.replace("\r"," ").replace("\n"," ").strip()[:80]))
        d = dwell(out)
        f.write(json.dumps([round(t,3), "o", out]) + "\n")
        t += d

print(f"wrote {DST}: {len(events)} events, {t:.1f}s = {t/60:.1f} min")
print("\nCHAPTERS (elapsed sec -> first line) — narrate here:")
for tt, line in chapters:
    print(f"  {tt:6.1f}s  {line}")
