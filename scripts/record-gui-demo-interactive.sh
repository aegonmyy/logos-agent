#!/bin/bash
# record-gui-demo-interactive.sh - interactive VNC recording of the Basecamp GUI.
#
# Brings up the full stack (Xvfb + x11vnc + noVNC web + nwaku + the agent driver
# + logos-standalone-app with the agent_owner plugin) and records the screen
# while YOU drive the Basecamp owner app over VNC/noVNC. The agent driver waits
# for your GUI decisions (it does not drive the GUI) and verifies on-chain
# settlement after each stage, so the recording shows real human interaction
# with a real cursor, settling on a real sequencer.
#
# After you finish, the script overlays timed stage captions + a final
# settlement summary onto the raw capture (see build-overlay.py) so the video
# is legible without narration: each stage announces itself, and the on-chain
# balance progression is shown at the end.
#
# Connect (from your laptop, in a browser):
#   http://104.207.89.61:6080/vnc.html?autoconnect=1
#   password: logos2026
#
# Or via VNC client over an SSH tunnel:
#   ssh -L 5900:localhost:5900 ubuntu@104.207.89.61
#   then connect a VNC viewer to localhost:5900 (password logos2026)
#
# Flow:
#   1. Xvfb + x11vnc + noVNC + nwaku + ffmpeg capture start
#   2. Driver test starts (creates agent, funds it 100 tokens, writes
#      agent-ids.env, waits for logoscore-ready.flag)
#   3. Script starts logos-standalone-app (agent_owner plugin), touches the
#      ready flag -> driver proposes the first (over-limit) spend
#   4. YOU see the pending request in the Basecamp app over VNC and click
#      Poll requests -> Approve. Driver verifies balance 100->50.
#   5. Driver proposes a second over-limit spend. YOU click Poll -> Deny.
#      Driver verifies balance stays 50.
#   6. Driver signals reconfigure. YOU type 45 in the limit field -> Set.
#      Driver verifies the per-tx limit is now 45.
#   7. Driver proposes a 40-token spend (under the new limit): it executes
#      autonomously. Driver verifies balance 50->10. RECORDING_COMPLETE.
#   8. Script stops capture, overlays captions, writes basecamp-gui-demo.mp4.
#
# Watch the driver.log lines on stdout (or $VIDEO/driver.log) for the
# READY:approve / READY:deny / READY:reconfigure cues that tell you when to
# act. Take your time; the driver waits up to 5 minutes per stage.

set -euo pipefail

export PATH="$HOME/.risc0/bin:$HOME/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu/bin:$HOME/.cargo/bin:$PATH"

REPO=/home/ubuntu/logos-agent
WORK=/home/ubuntu/recording/work
VIDEO=/home/ubuntu/recording/video
DISPLAY=:99
SCREEN=1920x1080x24
VNC_PASS=logos2026
VNC_PORT=5900
NOVNC_PORT=6080

LOGOSCORE="$WORK/logoscore/bin/logos-standalone-app"
INSTALL_ROOT="$WORK/combined-install"
BUNDLES=/home/ubuntu/recording/bundles
FFI_LIB="$BUNDLES/logos_agent.so"
LC="$WORK/logos-config"
FONT=/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf

mkdir -p "$WORK" "$VIDEO" "$LC"
rm -f "$WORK/agent-ids.env" "$WORK/logoscore-ready.flag"
rm -f "$WORK/record-state.json" "$REPO/record-state.json"
rm -f "$VIDEO/events.tsv"

mkdir -p "$HOME/.vnc"
x11vnc -storepasswd "$VNC_PASS" "$HOME/.vnc/passwd" >/dev/null 2>&1

# --- Cleanup ---
cleanup() {
  echo "=== Cleaning up ==="
  kill ${FFMPEG_PID:-} ${LOGOSCORE_PID:-} ${DRIVER_PID:-} ${XVFB_PID:-} \
       ${X11VNC_PID:-} ${WS_PID:-} 2>/dev/null || true
  pkill -f "logos-standalone-app" 2>/dev/null || true
  pkill -f "ui-host" 2>/dev/null || true
  pkill websockify 2>/dev/null || true
  pkill x11vnc 2>/dev/null || true
  docker rm -f nwaku 2>/dev/null || true
  pkill Xvfb 2>/dev/null || true
  docker rm -f $(docker ps -q --filter "ancestor=ghcr.io/logos-blockchain/logos-blockchain:0.2.1-lssa") 2>/dev/null || true
}
trap cleanup EXIT

echo "=== 1. Start Xvfb + x11vnc + noVNC ==="
pkill Xvfb 2>/dev/null || true; pkill x11vnc 2>/dev/null || true; pkill websockify 2>/dev/null || true
sleep 1
Xvfb "$DISPLAY" -screen 0 "$SCREEN" -ac +extension GLX +render -noreset &
XVFB_PID=$!
sleep 2
export DISPLAY="$DISPLAY"

x11vnc -display "$DISPLAY" -rfbauth "$HOME/.vnc/passwd" -forever -shared \
  -noxdamage -threads -rfbport $VNC_PORT -bg -o "$VIDEO/x11vnc.log" 2>/dev/null
sleep 1
websockify --web=/usr/share/novnc $NOVNC_PORT localhost:$VNC_PORT > "$VIDEO/websockify.log" 2>&1 &
WS_PID=$!
sleep 1

PUBLIC_IP=$(curl -s --max-time 3 ifconfig.me 2>/dev/null || echo "104.207.89.61")
echo ""
echo "============================================================"
echo "  Open this URL in your browser to drive the Basecamp app:"
echo "  http://$PUBLIC_IP:$NOVNC_PORT/vnc.html?autoconnect=1"
echo "  VNC password: $VNC_PASS"
echo "============================================================"
echo ""

echo "=== 2. Start nwaku ==="
docker rm -f nwaku 2>/dev/null || true
docker run -d --name nwaku -p 8645:8645 \
  wakuorg/nwaku:v0.38.0 \
  --rest=true --rest-address=0.0.0.0 --rest-port=8645 \
  --relay=true --cluster-id=2
sleep 3
echo "nwaku started"

echo "=== 3. Start ffmpeg capture ==="
date +%s.%N > "$VIDEO/capture-start"
ffmpeg -y -f x11grab -video_size 1920x1080 -framerate 30 \
  -i "$DISPLAY" -c:v libx264 -preset fast -crf 18 \
  "$VIDEO/basecamp-gui-raw.mp4" \
  -loglevel warning &
FFMPEG_PID=$!
sleep 1

echo "=== 4. Start agent driver ==="
cd "$REPO"
RISC0_DEV_MODE=1 AGENT_MESSAGING_URL="http://127.0.0.1:8645" \
  cargo test --test record_gui_driver -- --ignored --nocapture --test-threads=1 \
  > "$VIDEO/driver.log" 2>&1 &
DRIVER_PID=$!

echo "waiting for agent driver to provide account IDs..."
for i in $(seq 1 180); do
  if [ -f "$WORK/agent-ids.env" ]; then
    source "$WORK/agent-ids.env"
    break
  fi
  sleep 1
done
if [ -z "${LOGOS_AGENT_ACCOUNT_ID:-}" ]; then
  echo "ERROR: agent driver did not provide account IDs"
  cat "$VIDEO/driver.log" | tail -20
  exit 1
fi
echo "agent account: $LOGOS_AGENT_ACCOUNT_ID"
echo "owner: $LOGOS_AGENT_OWNER_ID"

echo "=== 5. Start logos-standalone-app ==="
export LOGOS_AGENT_FFI_PATH="$FFI_LIB"
export AGENT_MESSAGING_URL="http://127.0.0.1:8645"
export LOGOS_AGENT_ACCOUNT_ID
export LOGOS_AGENT_OWNER_ID
export QT_LOGGING_RULES="logos.viewhost.debug=true"

"$LOGOSCORE" \
  "$INSTALL_ROOT/plugins/agent_owner" \
  --modules-dir "$INSTALL_ROOT/modules" \
  --title "Autonomous Agent" \
  --width 1920 --height 1080 \
  --user-dir "$LC" \
  > "$VIDEO/logoscore.log" 2>&1 &
LOGOSCORE_PID=$!
echo "logos-standalone-app PID: $LOGOSCORE_PID"

sleep 5
touch "$WORK/logoscore-ready.flag"
echo "logoscore ready flag set - the Basecamp app should now be visible in your browser."

echo ""
echo "============================================================"
echo "  DRIVE THE APP NOW. Watch the cues below; the driver waits"
echo "  up to 5 min per stage for your click."
echo "    READY:approve   -> click Poll requests, then Approve"
echo "    READY:deny      -> click Poll requests, then Deny"
echo "    READY:reconfigure -> type 45 in the limit field, then Set"
echo "  The 4th stage (autonomous spend) needs no action."
echo "============================================================"
echo ""

# Stream the driver cues to stdout AND timestamp each key line into
# events.tsv as it appears, so the overlay phase knows when each stage
# started/ended in capture time.
now_t() {
  local start
  start=$(cat "$VIDEO/capture-start")
  python3 -c "print(round($(date +%s.%N) - $start, 3))"
}

# Map each cue to a stage caption. The first time we see a cue, record its
# timestamp; the stage ends when the next cue (or RECORDING_COMPLETE) appears.
declare -A CUE_SEEN
record_cue() {
  local cue="$1" t="$2"
  case "$cue" in
    READY:approve)    cap="1/4  Approve an over-limit 50-token spend (limit is 30)";;
    READY:deny)       cap="2/4  Deny an over-limit spend (no funds move)";;
    READY:reconfigure) cap="3/4  Raise the per-tx limit 30 -> 45";;
    AUTONOMOUS:*)     cap="4/4  Autonomous 40-token spend under the new limit";;
    *) return;;
  esac
  printf 'stage_start\t%s\t%s\n' "$t" "$cap" >> "$VIDEO/events.tsv"
}

tail -f "$VIDEO/driver.log" | while IFS= read -r line; do
  echo "$line"
  case "$line" in
    *READY:approve*|*READY:deny*|*READY:reconfigure*|*AUTONOMOUS:*)
      cue=$(echo "$line" | grep -oE "READY:(approve|deny|reconfigure)|AUTONOMOUS:" | head -1)
      [ -z "${CUE_SEEN[$cue]:-}" ] && { CUE_SEEN[$cue]=1; record_cue "$cue" "$(now_t)"; }
      ;;
    *RECORDING_COMPLETE*)
      printf 'stage_end\t%s\n' "$(now_t)" >> "$VIDEO/events.tsv"
      ;;
  esac
done &
TAIL_PID=$!

# Wait for the driver to finish (max 25 min for a human-paced run).
for i in $(seq 1 1500); do
  grep -q "RECORDING_COMPLETE" "$VIDEO/driver.log" 2>/dev/null && break
  if ! kill -0 $DRIVER_PID 2>/dev/null; then
    echo "driver process exited"
    break
  fi
  sleep 1
done
kill $TAIL_PID 2>/dev/null || true
sleep 1

echo "=== 6. Stop capture ==="
sleep 2
kill $FFMPEG_PID 2>/dev/null || true
sleep 1

echo "=== driver log key lines ==="
grep -E "AGENT_IDS_WRITTEN|READY:|balance after|APPROVED|DENIED|RECONFIGURED|AUTONOMOUS|RECORDING_COMPLETE|timed out|panic|Error" "$VIDEO/driver.log" 2>/dev/null || echo "no key lines found"

# ===========================================================================
# Phase B: build stage captions from the timestamped events.tsv (recorded
# live as the cues appeared), then overlay them + a settlement summary.
# events.tsv lines:
#   stage_start	<t>	<caption>
#   stage_end	<t>
# Each stage runs from its stage_start until the next stage_start (or
# stage_end for the last one).
# ===========================================================================
echo "=== 7. Phase B: overlay captions ==="
echo "events.tsv:"; cat "$VIDEO/events.tsv" 2>/dev/null

python3 - "$VIDEO/events.tsv" > "$VIDEO/overlay-filter.txt" <<'PYEOF'
import sys, os, subprocess
events_path = sys.argv[1]
VIDEO = os.path.dirname(events_path)
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
W, H = 1920, 1080
TITLE_DUR = 2.5

def esc(s):
    return s.replace("\\","\\\\").replace(":","\\:").replace("%","\\%").replace("'","’")

starts = []
end_t = None
with open(events_path) as f:
    for line in f:
        p = line.rstrip("\n").split("\t")
        if p[0] == "stage_start" and len(p) >= 3:
            starts.append((float(p[1]), p[2]))
        elif p[0] == "stage_end":
            end_t = float(p[1])

dur = float(subprocess.check_output(
    ["ffprobe","-v","error","-show_entries","format=duration",
     "-of","default=noprint_wrappers=1:nokey=1",
     os.path.join(VIDEO,"basecamp-gui-raw.mp4")]).strip())
if end_t is None or end_t > dur:
    end_t = dur

# Build stage (start,end,caption) list.
stages = []
for i,(s,cap) in enumerate(starts):
    e = starts[i+1][0] if i+1 < len(starts) else end_t
    stages.append((s, e, cap))

steps = []
prev = "0:v"
def add(filt):
    global prev
    lbl = f"v{len(steps)}"
    steps.append(f"[{prev}]{filt}[{lbl}]")
    prev = lbl

# Persistent top legend.
legend = "Basecamp owner app driving a live autonomous agent over real Waku - settled on a local LEZ sequencer"
add(f"drawbox=x=0:y=0:w=iw:h=44:color=black@0.5:t=fill")
add(f"drawtext=fontfile={FONT}:text='{esc(legend)}':fontcolor=white:fontsize=22:x=(w-text_w)/2:y=10")

# Title card at the start of each stage + persistent caption bar.
for (s,e,cap) in stages:
    if e <= s: continue
    te = min(s+TITLE_DUR, e)
    py = (H-160)//2
    add(f"drawbox=x=0:y={py}:w=iw:h=160:color=black@0.7:t=fill:enable='between(t,{s},{te})'")
    add(f"drawtext=fontfile={FONT}:text='{esc(cap)}':fontcolor=white:fontsize=44:x=(w-text_w)/2:y={py+55}:enable='between(t,{s},{te})'")
    add(f"drawbox=x=0:y=44:w=iw:h=50:color=black@0.55:t=fill:enable='between(t,{s},{e})'")
    add(f"drawtext=fontfile={FONT}:text='{esc(cap)}':fontcolor=white:fontsize=30:x=(w-text_w)/2:y=54:enable='between(t,{s},{e})'")

# Final settlement summary card, last 8s.
if dur > 14 and end_t:
    s = max(end_t - 8, 0)
    summary = "On-chain balance: 100 -> 50 (approve) -> 50 (deny) -> 50 -> 10 (autonomous under limit 45)"
    py = (H-140)//2
    add(f"drawbox=x=0:y={py}:w=iw:h=140:color=black@0.75:t=fill:enable='between(t,{s},{dur})'")
    add(f"drawtext=fontfile={FONT}:text='{esc(summary)}':fontcolor=white:fontsize=36:x=(w-text_w)/2:y={py+50}:enable='between(t,{s},{dur})'")

if not steps:
    print("[0:v]null[vlast]")
else:
    last = steps[-1]
    idx = last.rfind("[")
    steps[-1] = last[:idx]+"[vlast]"
    print(";".join(steps))
PYEOF

ffmpeg -y -i "$VIDEO/basecamp-gui-raw.mp4" \
  -filter_complex "$(cat "$VIDEO/overlay-filter.txt")" \
  -map "[vlast]" -c:v libx264 -preset fast -crf 20 \
  "$VIDEO/basecamp-gui-demo.mp4" -loglevel error 2>&1 | tail -8 || {
    echo "overlay ffmpeg failed; raw capture kept"
    cp "$VIDEO/basecamp-gui-raw.mp4" "$VIDEO/basecamp-gui-demo.mp4"
  }

echo "=== Done ==="
ls -la "$VIDEO/basecamp-gui-demo.mp4" 2>/dev/null
ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 "$VIDEO/basecamp-gui-demo.mp4" 2>/dev/null
