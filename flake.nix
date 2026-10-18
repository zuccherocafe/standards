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

      rust = pkgs.rust-bin.fromRustupToolchainFile ./tools/rust/rust-toolchain.toml;
      rustPlatform = pkgs.makeRustPlatform { cargo = rust; rustc = rust; };

      # Built from this repository's own source and lockfile with the pinned toolchain; it runs `cargo metadata`
      # from the PATH of the shell it is used in.
      rustScans = rustPlatform.buildRustPackage {
        pname = "rust-scans";
        inherit ((builtins.fromTOML (builtins.readFile ./tools/rust/Cargo.toml)).workspace.package) version;
        src = pkgs.lib.fileset.toSource {
          root = ./tools/rust;
          fileset = pkgs.lib.fileset.unions [ ./tools/rust/Cargo.toml ./tools/rust/Cargo.lock ./tools/rust/crates ];
        };
        cargoLock.lockFile = ./tools/rust/Cargo.lock;
        cargoBuildFlags = [ "--package" "rust-scans" ];
        # The checks run its tests.
        doCheck = false;
      };

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
        runtimeInputs = [ rust python pkgs.ruff pkgs.pyrefly pkgs.lychee pkgs.git pkgs.findutils pkgs.coreutils ];
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
          format() { python_files | xargs -0 ruff format --check; }
          lint() { python_files | xargs -0 ruff check; }
          types() { python_files | xargs -0 pyrefly check; }
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
          step "rust format" cargo fmt --manifest-path tools/rust/Cargo.toml --check
          step "rust lint" cargo clippy --manifest-path tools/rust/Cargo.toml --locked --all-targets -- -D warnings
          step "rust tests" cargo test --manifest-path tools/rust/Cargo.toml --locked
          exit "$failed"
        '';
      };
    in
    {
      packages.${system}.rust-scans = rustScans;

      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rust
          python
          pkgs.ruff
          pkgs.pyrefly
          pkgs.lychee
          cargoMutants
          check
          pkgs.jujutsu
        ];
      };
    };
}
