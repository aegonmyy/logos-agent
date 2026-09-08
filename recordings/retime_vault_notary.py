#!/usr/bin/env python3
"""Re-time the vault+notary cast (58 min of real proving) down to a narratable
~4:25, the same treatment logos-agent-real-proof.cast got, with a second pass
that freezes the screen just before each narration beat so the beat lands
exactly when the narration reaches it (see recordings/vault-notary-narration.md).

Pass 1 assigns every event a dwell from what it shows (banners and results
hold, progress bars and tx dumps collapse). Pass 2 pins four beats to target
times by adding a hold to the event just before each beat: the screen freezes
on what is showing (e.g. the anchor lines) while the narration carries, then
the beat prints on cue. The header idle cap is set above the largest hold so
renderers honor the pacing instead of clamping it.
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
    ("paid_multi_agent_task", 250),
    ("test result", 300),
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

raw = [dwell(o) for o in events]

# locate beat events, in order
pins = []
search_from = 0
for sub, target in BEATS:
    for i in range(search_from, len(events)):
        if sub in events[i].lower():
            pins.append((i, target))
            search_from = i + 1
            break
assert len(pins) == len(BEATS), f"missing beats: found {len(pins)} of {len(BEATS)}"

# pass 2: hold on the event before each beat until the beat lands on target.
# t is the start time of event i; a hold lengthens event i's dwell (the time
# AFTER it prints) so the next event, the beat, starts exactly on target.
held = list(raw)
starts = [0.0] * len(events)
t = 0.0
pin_iter = iter(pins)
next_pin = next(pin_iter, None)
for i in range(len(events)):
    d = held[i]
    if next_pin and i + 1 == next_pin[0]:
        deficit = next_pin[1] - (t + d)
        if deficit > 0:
            held[i] += deficit
            d = held[i]
        next_pin = next(pin_iter, None)
    starts[i] = t
    t += d

with open(DST, "w") as f:
    header["idle_time_limit"] = 300.0
    f.write(json.dumps(header) + "\n")
    tt = 0.0
    for out, d in zip(events, held):
        f.write(json.dumps([round(tt, 3), "o", out]) + "\n")
        tt += d

print(f"wrote {DST}: {len(events)} events, {tt:.1f}s = {tt/60:.1f} min")
print("\nBEATS land at:")
for i, target in pins:
    print(f"  {starts[i]:7.1f}s  {events[i].replace(chr(13), ' ').replace(chr(10), ' ').strip()[:70]}")
for i, out in enumerate(events):
    low = out.lower()
    if any(k in low for k in ("paid_multi_agent_task", "test result", "demo complete")):
        print(f"  tail {starts[i]:6.1f}s  {out.replace(chr(13), ' ').replace(chr(10), ' ').strip()[:70]}")
