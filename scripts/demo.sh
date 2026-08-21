#!/usr/bin/env bash
#
# demo.sh — reproducible end-to-end demo of the Logos autonomous agent.
#
# Runs the agent's scenarios against a real local LEZ stack (bedrock + sequencer
# + indexer + wallet, brought up automatically via Docker) with REAL proofs:
# RISC0_DEV_MODE=0. The proof-generation output on screen is the evidence that
# dev mode is off.
#
# Prerequisites: Docker running; the RISC0 toolchain installed (r0vm 3.0.5).
# Run from the repository root.
#
# Usage:
#   ./scripts/demo.sh            # run the curated scenario sequence
#   ./scripts/demo.sh all        # run every agent test
#   ./scripts/demo.sh <test>     # run one test, e.g. a2a_two_agents

set -euo pipefail

export RISC0_DEV_MODE=0

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"

banner() {
  echo
  echo "=================================================================="
  echo "  $1"
  echo "=================================================================="
  echo
}

run_scenario() {
  local test_name="$1"
  local description="$2"
  banner "$description  [RISC0_DEV_MODE=$RISC0_DEV_MODE]"
  ( cd "$REPO_DIR" && RUST_LOG=warn cargo test --test "$test_name" \
      -- --nocapture --test-threads=1 )
}

banner "Logos Autonomous Agent — end-to-end demo"
echo "RISC0_DEV_MODE = $RISC0_DEV_MODE   (0 = real Groth16 proofs)"
echo "Proof generation below is real; expect each private transaction to take"
echo "a few minutes while the proof is produced."

case "${1:-curated}" in
  curated)
    run_scenario agent_spending \
      "Scenario 1 — Sovereign wallet: shielded account, spends within its limit, holds an over-limit spend"
    run_scenario owner_approval_flow \
      "Scenario 2 — Owner control: over-limit spend approved over the owner channel, a later one denied, and the limit reconfigured"
    run_scenario a2a_two_agents \
      "Scenario 3 — Agent marketplace: two agents discover each other and settle a paid task autonomously (A2A + LEZ payment)"
    run_scenario three_use_cases_local \
      "Scenario 4 — Three use cases: personal file vault, privacy-preserving notary, and a paid multi-agent task"
    ;;
  all)
    for t in agent_spending skills_dispatch storage_messaging_skills owner_approval_flow a2a_two_agents three_use_cases_local owner_ffi_e2e three_category_agents; do
      run_scenario "$t" "Running $t"
    done
    ;;
  *)
    run_scenario "$1" "Running $1"
    ;;
esac

banner "Demo complete — all scenarios passed with RISC0_DEV_MODE=0"
