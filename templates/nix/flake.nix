# A project's development shell, at the repository root beside flake.lock; both are tracked by git, since a flake
# sees only tracked files. Enter it with `nix develop`, or run one command in it with `nix develop -c <command>`.
# Written for Lix and kept to what other Nix implementations also evaluate.
#
# Adding a tool: pick the helper that fits (`crate` for a cargo tool, `official` for a published binary), write
# its exact version and its hash, and add it to `packages`. To learn a hash, write `lib.fakeHash`, enter the shell
# and copy the hash the error reports; for a published binary, prefer the digest its publisher prints. A pin moves
# in the same change as the file that checks it (the project's AGENTS.md, a script that refuses another version).
# A Rust project pins cargo-mutants with `crate`, as `rust-mutants` runs it (languages/rust/tests.md, Mutation report).
# A project with Python pins its interpreter, Ruff and Pyrefly (languages/python/README.md); nixpkgs' builds are fine if pinned by the lock.
{
  description = "<project> development shell";

  # Inputs are fetched as tarballs (`github:` or a tarball URL), never `git+https`.
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # The standards' shared tools, pinned by flake.lock like every input and moved with the other pins.
    standards = {
      url = "github:zuccherocafe/standards";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, rust-overlay, standards }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
      lib = pkgs.lib;

      # The toolchain file is the one pin for Rust: channel, profile, components and targets come from it.
      # project-specific: the path of the workspace's rust-toolchain.toml
      rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

      # A cargo tool built from crates.io at an exact version, with the lockfile it was published with; `hash`
      # covers the crate and `cargoHash` its vendored dependencies.
      crate = { pname, version, hash, cargoHash, nativeBuildInputs ? [ ], buildInputs ? [ ] }:
        pkgs.rustPlatform.buildRustPackage {
          inherit pname version cargoHash nativeBuildInputs buildInputs;
          src = pkgs.fetchCrate { inherit pname version hash; };
          doCheck = false;
        };

      # A published binary, fetched by its publisher's digest and installed unchanged: no patching, no stripping.
      # A program that draws or plays sound (an engine, a 3D suite) is fetched this way, so it loads the host's
      # graphics and audio libraries and the host's GPU drivers, exactly as its publisher built it to.
      official = { install, ... }@args:
        pkgs.stdenvNoCC.mkDerivation (builtins.removeAttrs args [ "install" ] // {
          nativeBuildInputs = [ pkgs.unzip ];
          dontConfigure = true;
          dontBuild = true;
          dontFixup = true;
          sourceRoot = ".";
          installPhase = install;
        });

      # The standards' report scans as `rust-scans`; the project passes its cores and kernels (its AGENTS.md).
      rustScans = pkgs.writeShellScriptBin "rust-scans" ''
        exec ${pkgs.python3}/bin/python3 -I ${standards}/tools/rust_scans.py "$@"
      '';

      # The standards' mutation report as `rust-mutants`, over the cargo-mutants the project pins below.
      rustMutants = pkgs.writeShellScriptBin "rust-mutants" ''
        exec ${pkgs.python3}/bin/python3 -I ${standards}/tools/rust_mutants.py "$@"
      '';

      # project-specific: the project's tools, each by exact version and hash, cargo-mutants among them. For example:
      #
      # cargoDeny = crate {
      #   pname = "cargo-deny";
      #   version = "<x.y.z>";
      #   hash = "<sri hash>";
      #   cargoHash = "<sri hash>";
      #   nativeBuildInputs = [ pkgs.pkg-config ];
      #   buildInputs = [ pkgs.zstd ];
      # };
      #
      # engine = official {
      #   pname = "<engine>";
      #   version = "<x.y.z>";
      #   src = pkgs.fetchurl {
      #     url = "<the publisher's download URL>";
      #     sha512 = "<the publisher's published digest>";
      #   };
      #   install = "install -Dm755 <binary in the archive> $out/bin/<engine>";
      # };
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rust
          rustScans
          rustMutants
          # project-specific: the project's tools
        ];

        # project-specific: a variable for each program the project's scripts find by one, such as
        # ENGINE_BIN = "${engine}/bin/<engine>";

        # The standards' skills at the pinned revision, linked into .agents/skills/ for agents that load skills, and
        # .claude/skills linked there for Claude Code. Nothing under .claude is committed: the links name store paths,
        # so they are ignored and remade here.
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
