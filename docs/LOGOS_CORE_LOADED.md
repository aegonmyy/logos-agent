# The agent module loads and runs inside Logos Core

The module is loaded into the actual Logos Core runtime (`logoscore`) and the
runtime invokes it — including starting an agent session and executing a skill
end-to-end through the runtime.

## Build

1. Build the module (Qt plugin) with the Logos module builder:
   `nix build .#agent-install` — produces `modules/agent/agent_plugin.so`.
2. Build the Rust core so it matches the runtime's libraries:
   `./scripts/build-ffi.sh` — produces `target/ffi/debug/liblogos_agent.so`,
   linked against the same nixpkgs (glibc / libstdc++ / libpython) the Logos
   Core runtime uses. Point the module at it:
   `export LOGOS_AGENT_FFI_PATH=<repo>/target/ffi/debug/liblogos_agent.so`.

## Run

```
$ logoscore -D --config-dir "$LC" -m <agent-install>/modules -v &
$ logoscore --config-dir "$LC" load-module agent
{"dependencies_loaded":[],"module":"agent","status":"ok","version":"1.0.0"}

$ logoscore --config-dir "$LC" call agent health
{"method":"health","module":"agent","result":"{\"ok\":true}","status":"ok"}

$ logoscore --config-dir "$LC" call agent agentVersionJson
{"method":"agentVersionJson","module":"agent","result":"{\"ok\":true,\"version\":\"0.1.0\"}","status":"ok"}

$ logoscore --config-dir "$LC" call agent startSessionJson <account-id>
{"method":"startSessionJson","module":"agent","result":"{\"ok\":true}","status":"ok"}

$ logoscore --config-dir "$LC" call agent invokeSkillJson storage.upload '{"label":"vault","data":"secret via logos core"}'
{"method":"invokeSkillJson","module":"agent","result":"{\"ok\":true,\"result\":{\"address\":\"f867f1d3...\"}}","status":"ok"}
```

So, through the Logos Core runtime: the module loads, `health` reports the Rust
core is live, an agent session starts, and a real skill (`storage.upload` —
encrypt + store) executes and returns a content address.

## Why the FFI build is separate

The Rust core links libpython (via a transitive `pyo3` dependency), plus
libstdc++ and libc. For `logoscore` to `dlopen` it, those must be the same
libraries the runtime was built with. `scripts/build-ffi.sh` builds the core in a
nix-shell pinned to the same nixpkgs revision as the Logos module builder, so the
glibc / libstdc++ / libpython all match and the core loads cleanly.
