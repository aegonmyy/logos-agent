{
  description = "Logos autonomous agent — Logos Core module";

  inputs = {
    logos-module-builder.url = "github:logos-co/logos-module-builder";
    nixpkgs.follows = "logos-module-builder/nixpkgs";
  };

  outputs = inputs@{ self, nixpkgs, logos-module-builder, ... }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];

      # The agent's Rust core is built with cargo (`cargo build --release`,
      # producing liblogos_agent.so) and loaded by the module at runtime via
      # LOGOS_AGENT_FFI_PATH. The module itself has no Logos-module dependencies,
      # so it builds against only Qt + the Logos Core SDK.
      agentModule = logos-module-builder.lib.mkLogosModule {
        src = ./module;
        configFile = ./module/metadata.json;
        flakeInputs = inputs;
      };
    in {
      packages = nixpkgs.lib.genAttrs supportedSystems
        (system: agentModule.packages.${system} or {});

      devShells = agentModule.devShells or {};
    };
}
