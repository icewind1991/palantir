{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-23.11";
    utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.inputs.flake-utils.follows = "utils";
    cross-naersk.url = "github:icewind1991/cross-naersk";
    cross-naersk.inputs.nixpkgs.follows = "nixpkgs";
    cross-naersk.inputs.naersk.follows = "naersk";
  };

  outputs = {
    self,
    nixpkgs,
    utils,
    naersk,
    rust-overlay,
    cross-naersk,
  }:
    utils.lib.eachDefaultSystem (system: let
      overlays = [
        (import rust-overlay)
        (import ./overlay.nix)
      ];
      pkgs = import nixpkgs {
        inherit system overlays;
      };
      inherit (pkgs) lib rust-bin callPackage;
      inherit (builtins) fromTOML readFile map replaceStrings;

      hostTarget = pkgs.hostPlatform.config;
      targets = [
        hostTarget
        "x86_64-pc-windows-gnu"
        "x86_64-unknown-linux-musl"
        "i686-unknown-linux-musl"
        "armv7-unknown-linux-musleabihf"
        "aarch64-unknown-linux-musl"
      ];

      releaseTargets = lib.lists.remove hostTarget targets;

      artifactForTarget = target: "palantir${cross-naersk'.execSufficForTarget target}";
      assetNameForTarget = target: "palantir-${replaceStrings ["-unknown" "-gnu" "-musl" "abihf" "-pc"] ["" "" "" "" ""] target}${cross-naersk'.execSufficForTarget target}";

      cross-naersk' = callPackage cross-naersk {inherit naersk;};

      src = lib.sources.sourceByRegex (lib.cleanSource ./.) ["Cargo.*" "(src|benches)(/.*)?"];

      msrv = (fromTOML (readFile ./Cargo.toml)).package.rust-version;
      toolchain = rust-bin.stable.latest.default;
      msrvToolchain = rust-bin.stable."${msrv}".default;

      nearskOpt = {
        pname = "palantir";
        root = src;

        postInstall = pkgs.palantir.postInstall;
      };
      buildTarget = target: (cross-naersk'.buildPackage target) nearskOpt;
      hostNaersk = cross-naersk'.hostNaersk;
      msrvNaersk = callPackage naersk {
        rustc = msrvToolchain;
        cargo = msrvToolchain;
      };
    in rec {
      packages =
        nixpkgs.lib.attrsets.genAttrs targets buildTarget
        // rec {
          palantir = pkgs.palantir;
          check = hostNaersk.buildPackage (nearskOpt
            // {
              mode = "check";
            });
          clippy = hostNaersk.buildPackage (nearskOpt
            // {
              mode = "clippy";
            });
          msrv = msrvNaersk.buildPackage (nearskOpt
            // {
              mode = "check";
            });
          default = palantir;
        };

      apps.palantir = utils.lib.mkApp {
        drv = packages.palantir;
      };
      defaultApp = apps.palantir;

      inherit targets;
      releaseMatrix = {
        include =
          builtins.map (target: {
            inherit target;
            artifact_name = artifactForTarget target;
            asset_name = assetNameForTarget target;
          })
          releaseTargets;
      };

      # `nix develop`
      devShells.default = cross-naersk'.mkShell targets {
        nativeBuildInputs = with pkgs; [
          toolchain
          bacon
          cargo-msrv
        ];
      };
    })
    // {
      overlays = import ./overlay.nix;
      nixosModules.default = {
        pkgs,
        config,
        lib,
        ...
      }: {
        imports = [./module.nix];
        config = lib.mkIf config.services.palantir.enable {
          nixpkgs.overlays = [self.overlays.default];
          services.palantir.package = lib.mkDefault pkgs.palantir;
        };
      };
    };
}
