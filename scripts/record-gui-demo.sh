#!/bin/bash
# record-gui-demo.sh - headless recording of the Basecamp owner app GUI
#
# Brings up Xvfb + nwaku + logos-standalone-app (agent + owner modules) + the
# agent driver test, drives the QML approve/deny/reconfigure UI through the
# QML Inspector TCP API (port 3768), and captures with ffmpeg.
#
# The QML Inspector (newline-delimited JSON over TCP on port 3768) is the
# proper way to drive the QML UI: findAndClick finds buttons by their text
# label and sends real mouse events into the Qt scene graph, sendKeys types
# into text fields, and evaluate runs QML expressions. This is far more
# reliable than xdotool coordinate guessing against a QQuickWidget.
#
# Flow:
#   1. Xvfb + nwaku + ffmpeg capture start
#   2. Driver test starts (creates agent, funds it, writes agent-ids.env,
#      then waits for logoscore-ready.flag)
#   3. Script reads agent-ids.env, starts logos-standalone-app with the
#      owner module (which auto-loads the agent dependency + capability_module)
#   4. Script touches logoscore-ready.flag -> driver proposes first spend
#   5. QML UI receives the pending request; inspector clicks Poll, then
#      Approve/Deny, types in the limit field + Set
#   6. Driver verifies on-chain settlement; recording stops

set -euo pipefail

export PATH="$HOME/.risc0/bin:$HOME/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu/bin:$HOME/.cargo/bin:$PATH"

REPO=/home/ubuntu/logos-agent
WORK=/home/ubuntu/recording/work
VIDEO=/home/ubuntu/recording/video
DISPLAY=:99
SCREEN=1920x1080x24
INSPECTOR_PORT=3768

LOGOSCORE="$WORK/logoscore/bin/logos-standalone-app"
INSTALL_ROOT="$WORK/combined-install"
# The FFI lib is in /home/ubuntu/recording/bundles/ (not $WORK/bundles/).
# QLibrary adds a "lib" prefix to the filename, so point at the path WITHOUT
# the "lib" prefix; QLibrary resolves it to liblogos_agent.so on disk.
BUNDLES=/home/ubuntu/recording/bundles
FFI_LIB="$BUNDLES/logos_agent.so"
LC="$WORK/logos-config"

mkdir -p "$WORK" "$VIDEO" "$LC"
rm -f "$WORK/agent-ids.env" "$WORK/logoscore-ready.flag"
# Remove any stale agent runtime state. AgentRuntime::with_state loads this
# file if it exists and restores `consumed`/`next_id`/`pending` from it; a
# leftover file from a prior run (with consumed>0) makes the driver skip the
# first decision message on the owner channel (skip(consumed) steps over it),
# so the approval never lands and the driver times out. Start each run clean.
rm -f "$WORK/record-state.json" "$REPO/record-state.json"

# --- Inspector helper: send a JSON command, read the response ---
inspector_cmd() {
  local cmd="$1"
  local resp
  resp=$(echo "$cmd" | nc -q3 127.0.0.1 $INSPECTOR_PORT 2>/dev/null)
  echo "$resp"
}

# --- Cleanup ---
cleanup() {
  echo "=== Cleaning up ==="
  kill ${FFMPEG_PID:-} ${LOGOSCORE_PID:-} ${DRIVER_PID:-} ${XVFB_PID:-} 2>/dev/null || true
  pkill -f "logos-standalone-app" 2>/dev/null || true
  pkill -f "ui-host" 2>/dev/null || true
  docker rm -f nwaku 2>/dev/null || true
  pkill Xvfb 2>/dev/null || true
  docker rm -f $(docker ps -q --filter "ancestor=ghcr.io/logos-blockchain/logos-blockchain:0.2.1-lssa") 2>/dev/null || true
}
trap cleanup EXIT

echo "=== 1. Start Xvfb ==="
pkill Xvfb 2>/dev/null || true
Xvfb "$DISPLAY" -screen 0 "$SCREEN" -ac +extension GLX +render -noreset &
XVFB_PID=$!
sleep 2
export DISPLAY="$DISPLAY"

echo "=== 2. Start nwaku ==="
docker rm -f nwaku 2>/dev/null || true
docker run -d --name nwaku -p 8645:8645 \
  wakuorg/nwaku:v0.38.0 \
  --rest=true --rest-address=0.0.0.0 --rest-port=8645 \
  --relay=true --cluster-id=2
sleep 3
echo "nwaku started"

echo "=== 3. Start ffmpeg capture ==="
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

# Wait for the inspector to be ready
echo "waiting for QML inspector..."
for i in $(seq 1 30); do
  if echo '{"command":"getTree","params":{"depth":1},"id":1}' | nc -q1 127.0.0.1 $INSPECTOR_PORT 2>/dev/null | grep -q '"ok"'; then
    echo "inspector ready"
    break
  fi
  sleep 1
done

sleep 3
# Signal the driver that the GUI + owner module are loaded
touch "$WORK/logoscore-ready.flag"
echo "logoscore ready flag set"

import -window root "$VIDEO/screenshot-01-initial.png" 2>/dev/null || true

echo "=== 6. Drive QML UI via inspector ==="

# --- Stage 1: Approve ---
echo "waiting for approve signal..."
for i in $(seq 1 120); do
  grep -q "READY:approve" "$VIDEO/driver.log" 2>/dev/null && break
  sleep 1
done
sleep 3
import -window root "$VIDEO/screenshot-02-pending-approve.png" 2>/dev/null || true

echo "clicking Poll requests..."
inspector_cmd '{"command":"findAndClick","params":{"text":"Poll requests"},"id":1}' | python3 -c "import json,sys; d=json.load(sys.stdin); print('poll:', d.get('ok'), d.get('matchedText',''))" 2>/dev/null
sleep 3
import -window root "$VIDEO/screenshot-02b-after-poll.png" 2>/dev/null || true

echo "clicking Approve..."
inspector_cmd '{"command":"findAndClick","params":{"text":"Approve","exact":true},"id":2}' | python3 -c "import json,sys; d=json.load(sys.stdin); print('approve:', d.get('ok'), d.get('matchedText',''))" 2>/dev/null
sleep 5
import -window root "$VIDEO/screenshot-03-after-approve.png" 2>/dev/null || true

# --- Stage 2: Deny ---
echo "waiting for deny signal..."
for i in $(seq 1 120); do
  grep -q "READY:deny" "$VIDEO/driver.log" 2>/dev/null && break
  sleep 1
done
sleep 3
import -window root "$VIDEO/screenshot-04-pending-deny.png" 2>/dev/null || true

echo "clicking Poll requests..."
inspector_cmd '{"command":"findAndClick","params":{"text":"Poll requests"},"id":3}' | python3 -c "import json,sys; d=json.load(sys.stdin); print('poll:', d.get('ok'), d.get('matchedText',''))" 2>/dev/null
sleep 3

echo "clicking Deny..."
inspector_cmd '{"command":"findAndClick","params":{"text":"Deny","exact":true},"id":4}' | python3 -c "import json,sys; d=json.load(sys.stdin); print('deny:', d.get('ok'), d.get('matchedText',''))" 2>/dev/null
sleep 3
import -window root "$VIDEO/screenshot-05-after-deny.png" 2>/dev/null || true

# --- Stage 3: Reconfigure (set per-tx limit to 45) ---
echo "waiting for reconfigure signal..."
for i in $(seq 1 120); do
  grep -q "READY:reconfigure" "$VIDEO/driver.log" 2>/dev/null && break
  sleep 1
done
sleep 3
import -window root "$VIDEO/screenshot-06-reconfigure.png" 2>/dev/null || true

echo "setting per-tx limit to 45..."
# Find the per-tx limit TextField (first TextField) and type 45
# Use evaluate to set the limitField.text property directly
inspector_cmd '{"command":"evaluate","params":{"expression":"limitField.text = \"45\""},"id":5}' | python3 -c "import json,sys; d=json.load(sys.stdin); print('set text:', d.get('ok'))" 2>/dev/null
sleep 1

echo "clicking Set (per-tx)..."
# Use exact match so "Set" matches only the Button whose text is exactly "Set",
# not the error Label "...Set LOGOS_AGENT_ACCOUNT_ID..." (which contains "Set"
# as a substring). There are two Set buttons (per-tx, per-period); the first
# match in tree order is the per-tx one, which is what we want.
inspector_cmd '{"command":"findAndClick","params":{"text":"Set","exact":true},"id":6}' | python3 -c "import json,sys; d=json.load(sys.stdin); print('set:', d.get('ok'), d.get('matchedText',''))" 2>/dev/null
sleep 5
import -window root "$VIDEO/screenshot-07-after-reconfigure.png" 2>/dev/null || true

echo "GUI driving complete, waiting for driver to finish..."

# Wait for the driver to finish (max 5 min)
for i in $(seq 1 300); do
  grep -q "RECORDING_COMPLETE" "$VIDEO/driver.log" 2>/dev/null && break
  if ! kill -0 $DRIVER_PID 2>/dev/null; then
    echo "driver process exited"
    break
  fi
  sleep 1
done

echo "=== 7. Stop capture ==="
sleep 2
import -window root "$VIDEO/screenshot-08-final.png" 2>/dev/null || true

echo "=== Done ==="
echo "raw video: $VIDEO/basecamp-gui-raw.mp4"
echo "screenshots: $VIDEO/screenshot-*.png"
echo "=== driver log key lines ==="
grep -E "AGENT_IDS_WRITTEN|READY:|balance after|APPROVED|DENIED|RECONFIGURED|AUTONOMOUS|RECORDING_COMPLETE|timed out|panic|Error" "$VIDEO/driver.log" 2>/dev/null || echo "no key lines found"
