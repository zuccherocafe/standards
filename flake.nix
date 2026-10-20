{
  description = "zucchero✿café standards: development shell and checks";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };

      rust = pkgs.rust-bin.fromRustupToolchainFile ./tools/rust-toolchain.toml;
      rustPlatform = pkgs.makeRustPlatform { cargo = rust; rustc = rust; };

      # A binary of the workspace, built from this repository's own source and lockfile with the pinned toolchain.
      workspaceTool = pname: rustPlatform.buildRustPackage {
        inherit pname;
        inherit ((builtins.fromTOML (builtins.readFile ./tools/Cargo.toml)).workspace.package) version;
        src = pkgs.lib.fileset.toSource {
          root = ./tools;
          fileset = pkgs.lib.fileset.unions [ ./tools/Cargo.toml ./tools/Cargo.lock ./tools/crates ];
        };
        cargoLock.lockFile = ./tools/Cargo.lock;
        cargoBuildFlags = [ "--package" pname ];
        # The checks run its tests.
        doCheck = false;
      };

      # Both run what they need from the PATH of the shell they are used in: rust-scans runs `cargo metadata`;
      # dandegate runs `git` and `nix develop`.
      rustScans = workspaceTool "rust-scans";
      dandegate = workspaceTool "dandegate";

      # It runs `cargo mutants` from the shell it is used in. `git`, `systemd-run`, `unshare` and mold's `ld.mold` follow
      # the shell's own tools, so a shell without them, as dandegate runs a step in, still has them.
      rustMutants = pkgs.writeShellScriptBin "rust-mutants" ''
        export PATH="$PATH:${pkgs.git}/bin:${pkgs.systemdMinimal}/bin:${pkgs.util-linux}/bin:${pkgs.mold}/bin"
        exec ${workspaceTool "rust-mutants"}/bin/rust-mutants "$@"
      '';

      # Advises only: it fails on a pin file it cannot read, never on a pin's age.
      pinAge = workspaceTool "pin-age";

      # A new minor version moves the target in ruff.toml and pyrefly.toml with it.
      python = pkgs.python315;

      crate = { pname, version, hash, cargoHash }:
        pkgs.rustPlatform.buildRustPackage {
          inherit pname version cargoHash;
          src = pkgs.fetchCrate { inherit pname version hash; };
          doCheck = false;
        };

      cargoMutants = crate {
        pname = "cargo-mutants";
        version = "27.1.0";
        hash = "sha256-GCB0py7sp67epx+tB4VRpJIeDf4WlCSTPHHTENv06rc=";
        cargoHash = "sha256-ix/aR9SfYw14w930IhOocr8eU+895zuMxkSvtD7WaD0=";
      };

      check = pkgs.writeShellApplication {
        name = "check";
        # Each check is a function `step` calls by name, which ShellCheck cannot follow.
        excludeShellChecks = [ "SC2329" ];
        runtimeInputs = [
          rust python pinAge pkgs.ruff pkgs.pyrefly pkgs.lychee pkgs.git pkgs.findutils pkgs.coreutils
        ];
        text = ''
          cd "$(git rev-parse --show-toplevel)"
          export PYTHONDONTWRITEBYTECODE=1
          failed=0
          step() {
            local name="$1" started status
            shift
            started=$(date +%s%N)
            if "$@"; then status=passed; else status=FAILED; failed=1; fi
            echo "check: $name $status in $(( ($(date +%s%N) - started) / 1000000 )) ms"
          }
          # The repository's files, ignored ones left out: jj does not add a new file to git's index, so untracked
          # files count, and a file deleted since the index was written does not.
          files() {
            local file
            while IFS= read -r -d "" file; do
              if [ -e "$file" ]; then printf '%s\0' "$file"; fi
            done < <(git ls-files -z --cached --others --exclude-standard "$@")
          }
          python_files() { files '*.py'; }
          links() { files '*.md' | xargs -0 lychee --offline --include-fragments --no-progress; }
          format() { python_files | xargs -0 -r ruff format --check; }
          lint() { python_files | xargs -0 -r ruff check; }
          types() { python_files | xargs -0 -r pyrefly check; }
          tests() {
            local suite folder result=0
            while IFS= read -r -d "" suite; do
              folder="$(dirname "$(dirname "$suite")")"
              python3 -I -m unittest discover -s "$folder/tests" -t "$folder" || result=1
            done < <(files '*/tests/__init__.py')
            return "$result"
          }
          step links links
          step "python format" format
          step "python lint" lint
          step "python types" types
          step "python tests" tests
          step "pin age" pin-age --toolchain tools/rust-toolchain.toml \
            --toolchain templates/rust/rust-toolchain.toml --lock flake.lock
          step "rust format" cargo fmt --manifest-path tools/Cargo.toml --check
          step "rust lint" cargo clippy --manifest-path tools/Cargo.toml --locked --all-targets -- -D warnings
          step "rust tests" cargo test --manifest-path tools/Cargo.toml --locked
          exit "$failed"
        '';
      };
    in
    {
      packages.${system} = {
        rust-scans = rustScans;
        rust-mutants = rustMutants;
        pin-age = pinAge;
        inherit dandegate;
      };

      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rust
          python
          pkgs.ruff
          pkgs.pyrefly
          pkgs.lychee
          cargoMutants
          check
          dandegate
          rustScans
          rustMutants
          pinAge
          pkgs.jujutsu
        ];
      };
    };
}
