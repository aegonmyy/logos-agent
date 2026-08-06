{
  description = "Logos autonomous agent — Logos Core module and Basecamp owner app";

  inputs = {
    logos-module-builder.url = "github:logos-co/logos-module-builder";
    nixpkgs.follows = "logos-module-builder/nixpkgs";
  };

  outputs = inputs@{ self, nixpkgs, logos-module-builder, ... }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];

      # The agent core module. Its Rust core (liblogos_agent.so) is built with
      # cargo and loaded at runtime via LOGOS_AGENT_FFI_PATH. No Logos-module
      # dependencies, so it builds against only Qt + the Logos Core SDK.
      agentModule = logos-module-builder.lib.mkLogosModule {
        src = ./module;
        configFile = ./module/metadata.json;
        flakeInputs = inputs;
      };

      # The Basecamp owner app (QML): status, skills, and spend approvals. Talks
      # to the agent module over RemoteObjects.
      agentOwnerModule = logos-module-builder.lib.mkLogosQmlModule {
        src = ./app;
        configFile = ./app/metadata.json;
        flakeInputs = inputs // { agent = agentModule; };
      };

      packagesFor = system:
        let
          agentPkgs = agentModule.packages.${system} or {};
          ownerPkgs = agentOwnerModule.packages.${system} or {};
          prefix = tag: set:
            nixpkgs.lib.mapAttrs' (k: v: nixpkgs.lib.nameValuePair "${tag}-${k}" v) set;
        in
        prefix "agent" agentPkgs // prefix "owner" ownerPkgs;
    in {
      packages = nixpkgs.lib.genAttrs supportedSystems packagesFor;
    };
}
