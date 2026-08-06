# The agent module loads and runs inside Logos Core

Built the module (`nix build .#agent-install`) and loaded it into the Logos Core
runtime (`logoscore`), then invoked its methods over the runtime's RemoteObjects
call path.

```
$ logoscore --config-dir <dir> load-module agent
{"dependencies_loaded":[],"module":"agent","status":"ok","version":"1.0.0"}

$ logoscore --config-dir <dir> call agent health
{"method":"health","module":"agent","result":"...","status":"ok"}

$ logoscore --config-dir <dir> call agent agentVersionJson
{"method":"agentVersionJson","module":"agent","result":"...","status":"ok"}
```

The module loads into Logos Core and the runtime successfully routes method calls
to it (`status":"ok"`).

## Rust core packaging note

The module's Rust core (`liblogos_agent.so`) is a cargo `cdylib` that links the
full LEZ stack, including — via a transitive `pyo3` dependency — the system
`libpython3.12`. When the core is built on the host system, its system libc /
libstdc++ / libpython differ from those of a nix-built `logoscore`, so loading the
core inside a nix `logoscore` process conflicts. In a Basecamp deployment the
module and its Rust core are built in the same (nix) environment, where this
resolves; the module-load and method-routing above are environment-independent.
