{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-23.05";
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
      overlays = [ (import rust-overlay) ];
      pkgs = import nixpkgs {
        inherit system overlays;
      };
      lib = pkgs.lib;

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
      assetNameForTarget = target: "palantir-${builtins.replaceStrings ["-unknown" "-gnu" "-musl" "abihf" "-pc"] ["" "" "" "" ""] target}${cross-naersk'.execSufficForTarget target}";

      cross-naersk' = pkgs.callPackage cross-naersk {inherit naersk;};

      addUdev = ''
        mkdir -p $out/lib/udev/rules.d/
        echo 'SUBSYSTEM=="powercap", ACTION=="add", RUN+="${pkgs.coreutils-full}/bin/chgrp -R powermonitoring /sys%p", RUN+="${pkgs.coreutils-full}/bin/chmod -R g=u /sys%p"' >> $out/lib/udev/rules.d/51-palantir.rules
               echo 'SUBSYSTEM=="powercap", ACTION=="change", ENV{TRIGGER}!="none", RUN+="${pkgs.coreutils-full}/bin/chgrp -R powermonitoring /sys%p", RUN+="${pkgs.coreutils-full}/bin/chmod -R g=u /sys%p"' >> $out/lib/udev/rules.d/51-palantir.rules
      '';

      src = lib.sources.sourceByRegex (lib.cleanSource ./.) ["Cargo.*" "(src|benches)(/.*)?"];

      nearskOpt = {
        pname = "palantir";
        root = src;

        postInstall = addUdev;
      };
      buildTarget = target: (cross-naersk'.buildPackage target) nearskOpt;
      hostNaersk = cross-naersk'.hostNaersk;
    in rec {
      # `nix build`
      packages = nixpkgs.lib.attrsets.genAttrs targets buildTarget // rec {
        palantir = packages.${hostTarget};
        check = hostNaersk.buildPackage (nearskOpt // {
          mode = "check";
        });
        clippy = hostNaersk.buildPackage (nearskOpt // {
          mode = "clippy";
        });
        default = palantir;
      };

      apps.palantir = utils.lib.mkApp {
        drv = packages.palantir;
      };
      defaultApp = apps.palantir;

      inherit targets;
      releaseMatrix = {
        include = builtins.map (target: {
          inherit target;
          artifact_name = artifactForTarget target;
          asset_name = assetNameForTarget target;
        }) releaseTargets;
      };

      # `nix develop`
      devShells.default = cross-naersk'.mkShell targets {
        nativeBuildInputs = with pkgs; [
          bacon
        ];
      };
    })
    // {
      nixosModule = {
        config,
        lib,
        pkgs,
        ...
      }:
        with lib; let
          cfg = config.palantir.services.palantir;
        in {
          options.palantir.services.palantir = {
            enable = mkEnableOption "Enables the palantir service";

            port = mkOption rec {
              type = types.int;
              default = 5665;
              example = default;
              description = "The port to listen on";
            };

            zfs = mkOption rec {
              type = types.bool;
              default = false;
              example = true;
              description = "enable zfs integration";
            };

            docker = mkOption rec {
              type = types.bool;
              default = false;
              example = true;
              description = "enable docker integration";
            };

            mdns = mkOption rec {
              type = types.bool;
              default = true;
              example = true;
              description = "enable mdns discovery";
            };

            openPort = mkOption rec {
              type = types.bool;
              default = false;
              example = true;
              description = "open port";
            };

            openMDNSPort = mkOption rec {
              type = types.bool;
              default = false;
              example = true;
              description = "open mdns port";
            };
          };

          config = mkIf cfg.enable {
            networking.firewall.allowedTCPPorts = lib.optional cfg.openPort cfg.port;
            networking.firewall.allowedUDPPorts = lib.optional cfg.openMDNSPort 5353;

            users.groups.powermonitoring = {};

            services.udev.packages = [self.packages.${pkgs.system}.default];

            systemd.services."palantir" = {
              wantedBy = ["multi-user.target"];
              path = lib.optional cfg.zfs pkgs.zfs;
              environment = {
                PORT = "${toString cfg.port}";
                LD_LIBRARY_PATH = "/run/opengl-driver/lib/"; # needed for nvidia
              } // (if (cfg.mdns == false) then {
                DISABLE_MDNS = "true";
              } else {});

              serviceConfig = let
                pkg = self.packages.${pkgs.system}.default;
              in {
                Restart = "on-failure";
                ExecStart = "${pkg}/bin/palantir";
                DynamicUser = true;
                PrivateTmp = true;
                PrivateUsers = true;
                ProtectSystem = "strict";
                ProtectHome = true;
                NoNewPrivileges = true;
                ProtectClock = !cfg.zfs; # Enabling this breaks libzfs
                CapabilityBoundingSet = true;
                ProtectKernelLogs = true;
                ProtectControlGroups = true;
                SystemCallArchitectures = "native";
                ProtectKernelModules = true;
                RestrictNamespaces = true;
                MemoryDenyWriteExecute = true;
                ProtectHostname = true;
                LockPersonality = true;
                ProtectKernelTunables = true;
                RestrictAddressFamilies = ["AF_INET" "AF_INET6" "AF_NETLINK"] ++ lib.optional cfg.docker "AF_UNIX"; # netlink is required to make `getifaddrs` not err
                RestrictRealtime = true;
                SystemCallFilter = ["@system-service" "~@resources" "~@privileged"];
                IPAddressAllow = ["localhost"] ++ lib.optional cfg.mdns "multicast";
                UMask = "0077";
                SupplementaryGroups = ["powermonitoring"] ++ lib.optional cfg.docker "docker";
              };
            };
          };
        };
    };
}
