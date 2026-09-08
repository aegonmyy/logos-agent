#!/usr/bin/env python3
"""Re-time the vault+notary cast (58 min of real proving) down to a narratable
~4:20, the same treatment logos-agent-real-proof.cast got, with a second pass
that stretches the quiet stretches so each on-screen beat lands when the
narration reaches it (see recordings/vault-notary-narration.md).

Pass 1 assigns every event a dwell from what it shows (banners and results
hold, progress bars and tx dumps collapse). Pass 2 pins the six narration
beats to target times and rescales the dwell between consecutive beats.
"""
import json, re

SRC = "/home/ubuntu/logos-agent/recordings/vault-notary-real-proof.cast"
DST = "/home/ubuntu/logos-agent/recordings/vault-notary-narratable.cast"

PROG = re.compile(r"\[1m\[(9[0-9])?m?\s*(Building|Compiling|Finished)\b")
SYNCBAR = re.compile(r"[█░]{6,}")
GRABBAG = re.compile(r"(Stored persistent accounts|Stored statistics|Generated new account|With npk|With vpk|With pk|Falling back to slow)")

# (substring, target start time in seconds) in narration order
BEATS = [
    ("use_case=personal_file_vault", 50),
    ("included in block 14", 130),
    ("invalid privacy", 180),
    ("included in block 17", 215),
    ("test result", 240),
    ("demo complete", 255),
]

def dwell(out: str) -> float:
    if PROG.search(out):
        return 0.04
    low = out.lower()
    if SYNCBAR.search(out):
        return 0.04
    if "=====" in out and ("demo" in low or "logos autonomous agent" in low):
        return 5.0
    if "risc0_dev_mode=0" in low or "real groth16" in low:
        return 4.5
    if any(k in low for k in ("use_case=", "included in block", "test result", "demo complete",
                              "paid_multi_agent_task", "three_use_cases_local=complete")):
        return 4.5
    if "indexer_core" in low or "invalid privacy" in low:
        return 0.5
    if GRABBAG.search(out):
        return 0.06
    if "privacypreserving" in low.replace(" ", "") or "public_actions" in low:
        return 0.25
    if "\n" in out or "\r" in out:
        return 1.0
    return 0.12

events = []
with open(SRC) as f:
    header = json.loads(next(f))
    for line in f:
        try:
            _t, typ, out = json.loads(line)
            if typ == "o":
                events.append(out)
        except Exception:
            continue

# pass 1: raw dwell
raw = [dwell(o) for o in events]

# locate beat events, in order
pins = []  # (event index, target time)
search_from = 0
for sub, target in BEATS:
    for i in range(search_from, len(events)):
        if sub in events[i].lower():
            pins.append((i, target))
            search_from = i + 1
            break
assert len(pins) == len(BEATS), f"missing beats: found {len(pins)} of {len(BEATS)}"

# pass 2: rescale dwell between consecutive pins so each pin lands on target
scaled = list(raw)
prev_i, prev_t = 0, 0.0
for i, target in pins + [(len(events), None)]:
    window = sum(raw[prev_i:i])
    avail = (target - prev_t) if target is not None else window
    if window > 0 and avail > 0:
        f = avail / window
        for j in range(prev_i, i):
            scaled[j] = raw[j] * f if raw[j] > 0.3 else raw[j]  # keep churn fast
    if target is not None:
        prev_i, prev_t = i, target

with open(DST, "w") as f:
    header["idle_time_limit"] = 10.0
    f.write(json.dumps(header) + "\n")
    t = 0.0
    chapters = []
    for out, d in zip(events, scaled):
        low = out.lower()
        if any(k in low for k in ("use_case=", "included in block", "demo complete",
                                  "test result", "risc0_dev_mode=0", "invalid privacy")):
            chapters.append((t, out.replace("\r", " ").replace("\n", " ").strip()[:100]))
        f.write(json.dumps([round(t, 3), "o", out]) + "\n")
        t += d

print(f"wrote {DST}: {len(events)} events, {t:.1f}s = {t/60:.1f} min")
print("\nBEATS (pinned):", [(i, tt) for (i, tt) in pins])
print("\nCHAPTERS (elapsed sec -> line):")
for tt, line in chapters:
    print(f"  {tt:7.1f}s  {line}")
