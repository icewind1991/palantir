{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-22.11";
    utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    utils,
    naersk,
    rust-overlay,
  }:
    utils.lib.eachDefaultSystem (system: let
      overlays = [ (import rust-overlay) ];
      pkgs = import nixpkgs {
        inherit system overlays;
      };

      pkgs-cross-mingw = import nixpkgs {
        crossSystem = {
          config = "x86_64-w64-mingw32";
        };
        inherit system overlays;
      };
      mingw_w64_cc = pkgs-cross-mingw.stdenv.cc;
      mingw_w64 = pkgs-cross-mingw.windows.mingw_w64;
      windows = pkgs-cross-mingw.windows;

      naersk-lib = naersk.lib."${system}";
    in rec {
      # `nix build`
      packages.palantir = naersk-lib.buildPackage {
        pname = "palantir";
        root = ./.;
        postInstall = ''
          mkdir -p $out/lib/udev/rules.d/
          echo 'SUBSYSTEM=="powercap", ACTION=="add", RUN+="${pkgs.coreutils-full}/bin/chgrp -R powermonitoring /sys%p", RUN+="${pkgs.coreutils-full}/bin/chmod -R g=u /sys%p"' >> $out/lib/udev/rules.d/51-palantir.rules
                 echo 'SUBSYSTEM=="powercap", ACTION=="change", ENV{TRIGGER}!="none", RUN+="${pkgs.coreutils-full}/bin/chgrp -R powermonitoring /sys%p", RUN+="${pkgs.coreutils-full}/bin/chmod -R g=u /sys%p"' >> $out/lib/udev/rules.d/51-palantir.rules
        '';
      };
      defaultPackage = packages.palantir;

      # `nix run`
      apps.palantir = utils.lib.mkApp {
        drv = packages.palantir;
      };
      defaultApp = apps.palantir;

      # `nix develop`
      devShell = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          (rust-bin.stable.latest.default.override {
            targets = [ "x86_64-pc-windows-gnu" ];
          })
          bacon
          mingw_w64_cc
        ];
        depsBuildBuild = [ pkgs.wine64 ];
        buildInputs = [ windows.pthreads ];

        CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${mingw_w64_cc.targetPrefix}cc";
        CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER = "wine64";
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

            services.udev.packages = [self.defaultPackage.${pkgs.system}];

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
                pkg = self.defaultPackage.${pkgs.system};
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
