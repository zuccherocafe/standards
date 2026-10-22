{
  description = "zucchero✿café standards: development shell and checks";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    pyproject-nix = {
      url = "github:pyproject-nix/pyproject.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    uv2nix = {
      url = "github:pyproject-nix/uv2nix";
      inputs.pyproject-nix.follows = "pyproject-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      pyproject-nix,
      uv2nix,
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };

      rust = pkgs.rust-bin.fromRustupToolchainFile ./tools/rust-toolchain.toml;
      rustPlatform = pkgs.makeRustPlatform {
        cargo = rust;
        rustc = rust;
      };

      # A binary of the workspace, built from this repository's own source and lockfile with the pinned toolchain.
      workspaceTool =
        pname:
        rustPlatform.buildRustPackage {
          inherit pname;
          inherit ((builtins.fromTOML (builtins.readFile ./tools/Cargo.toml)).workspace.package) version;
          src = pkgs.lib.fileset.toSource {
            root = ./tools;
            fileset = pkgs.lib.fileset.unions [
              ./tools/Cargo.toml
              ./tools/Cargo.lock
              ./tools/crates
            ];
          };
          cargoLock.lockFile = ./tools/Cargo.lock;
          cargoBuildFlags = [
            "--package"
            pname
          ];
          # The checks run its tests.
          doCheck = false;
        };

      # Both run what they need from the PATH of the shell they are used in: rust-scans runs `cargo metadata`;
      # dandegate runs `git` and `nix develop`.
      rustScans = workspaceTool "rust-scans";
      dandegate = workspaceTool "dandegate";

      # It reads only the project's files; dandegate runs the after revision's copy over both checkouts.
      gdscriptScans = workspaceTool "gdscript-scans";

      # It runs the cargo-mutants pinned below, first on PATH, whatever the shell holds. The tool runs it directly, never
      # through Cargo's subcommand search. `git`, `systemd-run`, `unshare` and mold's `ld.mold` follow the shell's own
      # tools, so a shell without them, as dandegate runs a step in, still has them.
      rustMutants = pkgs.writeShellScriptBin "rust-mutants" ''
        export PATH="${cargoMutants}/bin:$PATH:${pkgs.git}/bin:${pkgs.systemdMinimal}/bin:${pkgs.util-linux}/bin:${pkgs.mold}/bin"
        exec ${workspaceTool "rust-mutants"}/bin/rust-mutants "$@"
      '';

      # Advises only: it fails on a pin file it cannot read, never on a pin's age.
      pinAge = workspaceTool "pin-age";

      designRead = workspaceTool "design-read";

      # Harbor runs the evals. evals/uv.lock pins it and everything it imports, each as a wheel for this system, so
      # nothing builds from source and nothing installs at run time. Only Harbor's own commands reach the shell.
      evals = uv2nix.lib.workspace.loadWorkspace { workspaceRoot = ./evals; };
      evalsPython =
        (pkgs.callPackage pyproject-nix.build.packages { python = pkgs.python314; }).overrideScope
          (evals.mkPyprojectOverlay { sourcePreference = "wheel"; });
      harbor = (pkgs.callPackages pyproject-nix.build.util { }).mkApplication {
        venv = evalsPython.mkVirtualEnv "evals" evals.deps.default;
        package = evalsPython.harbor;
      };

      crate =
        {
          pname,
          version,
          hash,
          cargoHash,
        }:
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

      # Lints the Nix files of the folder given, the current one by default. In a git work tree it lints the tracked and
      # unignored ones and refuses any folder but the root, so a run from a subfolder cannot pass without the rest.
      # Outside one, as in a flake's source in the store, which holds only the files git tracks, it lints every .nix
      # file under the folder. A binding deadnix flags that the code needs takes `# deadnix: skip, <reason>` on the
      # line above it.
      nixLint = pkgs.writeShellApplication {
        name = "nix-lint";
        runtimeInputs = [
          pkgs.nixfmt
          pkgs.statix
          pkgs.deadnix
          pkgs.git
          pkgs.findutils
        ];
        text = ''
          cd "''${1:-.}"
          files=()
          if top=$(git rev-parse --show-toplevel 2>/dev/null); then
            if [ "$top" != "$(pwd -P)" ]; then
              echo "nix-lint: run it from the root of the git work tree, $top, not from $PWD" >&2
              exit 1
            fi
            # jj does not add a new file to git's index, so untracked files count, and ignored ones do not.
            while IFS= read -r -d "" file; do
              if [ -e "$file" ]; then files+=("$file"); fi
            done < <(git ls-files -z --cached --others --exclude-standard -- '*.nix')
          else
            while IFS= read -r -d "" file; do files+=("$file"); done < <(find . -name '*.nix' -type f -print0)
          fi
          if [ "''${#files[@]}" -eq 0 ]; then
            echo "nix-lint: no .nix file under $PWD" >&2
            exit 1
          fi
          failed=0
          nixfmt --check -- "''${files[@]}" || failed=1
          for file in "''${files[@]}"; do statix check -- "$file" || failed=1; done
          deadnix --fail -- "''${files[@]}" || failed=1
          exit "$failed"
        '';
      };

      check = pkgs.writeShellApplication {
        name = "check";
        # Each check is a function `step` calls by name, which ShellCheck cannot follow.
        excludeShellChecks = [ "SC2329" ];
        runtimeInputs = [
          rust
          pinAge
          nixLint
          pkgs.lychee
          pkgs.git
          pkgs.findutils
          pkgs.coreutils
        ];
        text = ''
          cd "$(git rev-parse --show-toplevel)"
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
          links() { files '*.md' | xargs -0 lychee --offline --include-fragments --no-progress; }
          step links links
          step "nix lint" nix-lint
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
        gdscript-scans = gdscriptScans;
        rust-mutants = rustMutants;
        cargo-mutants = cargoMutants;
        pin-age = pinAge;
        design-read = designRead;
        nix-lint = nixLint;
        inherit dandegate harbor;
      };

      checks.${system}.nix-lint = pkgs.runCommand "nix-lint" { } ''
        ${nixLint}/bin/nix-lint ${self}
        touch $out
      '';

      formatter.${system} = pkgs.nixfmt-tree;

      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rust
          pkgs.lychee
          cargoMutants
          check
          dandegate
          rustScans
          gdscriptScans
          rustMutants
          pinAge
          designRead
          nixLint
          harbor
          # Reads Harbor's and pi's records for the evals' accounting.
          pkgs.jq
          pkgs.jujutsu
        ];
        # Harbor otherwise reports each command and job to its developers.
        env.HARBOR_TELEMETRY = "off";
      };
    };
}
