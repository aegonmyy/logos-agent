#!/usr/bin/env bash
#
# package-basecamp.sh — build the agent's Logos Core module and the Basecamp
# owner app into standalone, loadable `.lgx` bundles that can be distributed and
# side-loaded into a Logos app (Basecamp) or `logoscore` without this source tree.
#
# Output: dist/
#   agent.lgx        — the core agent module (headless runtime)
#   agent_owner.lgx  — the Basecamp owner mini-app (status + spend approvals)
#   liblogos_agent.so — the Rust core the module dlopen()s at runtime
#   README.txt       — how to load them
#
# Prerequisites: nix (flakes enabled). Run from the repository root.

set -euo pipefail

NIX=(nix --extra-experimental-features 'nix-command flakes')

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$REPO_DIR/dist"
cd "$REPO_DIR"

mkdir -p "$DIST"

echo "==> Building the core agent module bundle (agent.lgx)"
"${NIX[@]}" build .#agent-lgx --out-link "$DIST/.agent-lgx"
cp -Lf "$DIST"/.agent-lgx/*.lgx "$DIST/agent.lgx" 2>/dev/null \
  || cp -Lf "$(readlink -f "$DIST/.agent-lgx")" "$DIST/agent.lgx"

echo "==> Building the Basecamp owner app bundle (agent_owner.lgx)"
"${NIX[@]}" build .#owner-lgx --out-link "$DIST/.owner-lgx"
cp -Lf "$DIST"/.owner-lgx/*.lgx "$DIST/agent_owner.lgx" 2>/dev/null \
  || cp -Lf "$(readlink -f "$DIST/.owner-lgx")" "$DIST/agent_owner.lgx"

echo "==> Building the Rust core (liblogos_agent.so) matched to the runtime"
./scripts/build-ffi.sh
cp -Lf "$REPO_DIR/target/ffi/debug/liblogos_agent.so" "$DIST/liblogos_agent.so"

cat > "$DIST/README.txt" <<'EOF'
Logos Autonomous Agent — loadable bundles
=========================================

Files:
  agent.lgx          Core agent module (headless runtime).
  agent_owner.lgx    Basecamp owner mini-app (status + spend approvals).
  liblogos_agent.so  Rust core the module loads at runtime.

Load into a Logos app / logoscore:
  1. Point the module at the Rust core:
       export LOGOS_AGENT_FFI_PATH=/absolute/path/to/liblogos_agent.so
  2. Install the bundles into your Logos modules directory (or import the .lgx
     from the app's module manager), then load:
       logoscore --config-dir "$LC" load-module agent
       logoscore --config-dir "$LC" load-module agent_owner
  3. In Basecamp, open the "agent_owner" mini-app to see the agent's status and
     approve/deny over-limit spends over the owner channel.

The .lgx files are self-contained module archives; the source tree is not
required to load them.
EOF

echo
echo "==> Done. Distributable bundles in: $DIST"
ls -lh "$DIST"/*.lgx "$DIST"/liblogos_agent.so 2>/dev/null || true
rm -f "$DIST/.agent-lgx" "$DIST/.owner-lgx"
