{
  description = "<project> development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # Keeps its own nixpkgs, so its tools build as the standards' own checks build them.
    standards.url = "github:zuccherocafe/standards";
  };

  outputs = { self, nixpkgs, rust-overlay, standards }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
      lib = pkgs.lib;

      rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

      # A cargo tool from crates.io at an exact version, built with the lockfile it was published with.
      crate = { pname, version, hash, cargoHash, nativeBuildInputs ? [ ], buildInputs ? [ ] }:
        pkgs.rustPlatform.buildRustPackage {
          inherit pname version cargoHash nativeBuildInputs buildInputs;
          src = pkgs.fetchCrate { inherit pname version hash; };
          doCheck = false;
        };

      # A published binary installed unchanged, so it loads the host's graphics, audio and GPU drivers.
      official = { install, ... }@args:
        pkgs.stdenvNoCC.mkDerivation (builtins.removeAttrs args [ "install" ] // {
          nativeBuildInputs = [ pkgs.unzip ];
          dontConfigure = true;
          dontBuild = true;
          dontFixup = true;
          sourceRoot = ".";
          installPhase = install;
        });

      rustScans = standards.packages.${system}.rust-scans;
      rustMutants = standards.packages.${system}.rust-mutants;
      dandegate = standards.packages.${system}.dandegate;

    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rust
          rustScans
          rustMutants
          dandegate
        ];

        # Links the pinned standards' skills for agents; the links name store paths, so they are ignored and remade.
        shellHook = ''
          top="$(git rev-parse --show-toplevel)"
          skills="$top/.agents/skills"
          mkdir -p "$skills" "$top/.claude" && printf '*\n' > "$skills/.gitignore"
          find "$skills" -maxdepth 1 -type l -delete
          for skill in ${standards}/skills/*/; do ln -sfn "''${skill%/}" "$skills/"; done
          ln -sfn ../.agents/skills "$top/.claude/skills"
        '';
      };
    };
}
