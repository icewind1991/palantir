{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-23.05";
    utils.url = "github:numtide/flake-utils";
    naersk.url = "github:icewind1991/naersk?rev=6d245a3bbb2ee31ec726bb57b9a8b206302e7110";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.inputs.flake-utils.follows = "utils";
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
      lib = pkgs.lib;

      pkgs-cross-mingw = import nixpkgs {
        crossSystem = {
          config = "x86_64-w64-mingw32";
        };
        inherit system overlays;
      };

      targets = [
        "x86_64-unknown-linux-gnu"
        "x86_64-pc-windows-gnu"
        "x86_64-unknown-linux-musl"
        "i686-unknown-linux-musl"
        "armv7-unknown-linux-musleabihf"
        "aarch64-unknown-linux-musl"
       ];

      toolchain = (pkgs.rust-bin.stable.latest.default.override { inherit targets; });

      crossArgs = {
        "armv7-unknown-linux-musleabihf" = {
          nativeBuildInputs = [ pkgs.pkgsCross.armv7l-hf-multiplatform.stdenv.cc ];
          CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUSTFLAGS = "-C target-feature=+crt-static";
          CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER = "${pkgs.pkgsCross.armv7l-hf-multiplatform.stdenv.cc.targetPrefix}cc";
        };
        "aarch64-unknown-linux-musl" = {
          nativeBuildInputs = [ pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.cc ];
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS = "-C target-feature=+crt-static";
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER = "${pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.cc.targetPrefix}cc";
        };
        "i686-unknown-linux-musl" = {
          nativeBuildInputs = [ pkgs.pkgsCross.musl32.stdenv.cc ];
          CARGO_TARGET_I686_UNKNOWN_LINUX_MUSL_RUSTFLAGS = "-C target-feature=+crt-static";
          CARGO_TARGET_I686_UNKNOWN_LINUX_MUSL_LINKER = "${pkgs.pkgsCross.musl32.stdenv.cc.targetPrefix}cc";
        };
      };

      mingw_w64_cc = pkgs.pkgsCross.mingwW64.stdenv.cc;
      windows = pkgs.pkgsCross.mingwW64.windows;

      naersk' = pkgs.callPackage naersk {
        cargo = toolchain;
        rustc = toolchain;
      };

      addUdev = ''
        mkdir -p $out/lib/udev/rules.d/
        echo 'SUBSYSTEM=="powercap", ACTION=="add", RUN+="${pkgs.coreutils-full}/bin/chgrp -R powermonitoring /sys%p", RUN+="${pkgs.coreutils-full}/bin/chmod -R g=u /sys%p"' >> $out/lib/udev/rules.d/51-palantir.rules
               echo 'SUBSYSTEM=="powercap", ACTION=="change", ENV{TRIGGER}!="none", RUN+="${pkgs.coreutils-full}/bin/chgrp -R powermonitoring /sys%p", RUN+="${pkgs.coreutils-full}/bin/chmod -R g=u /sys%p"' >> $out/lib/udev/rules.d/51-palantir.rules
      '';

      src = lib.sources.sourceByRegex (lib.cleanSource ./.) ["Cargo.*" "(src|benches)(/.*)?"];

      nearskOpt = {
        pname = "palantir";
        root = src;
      };

      buildWindows = target: naersk'.buildPackage (nearskOpt // {
        strictDeps = true;
        depsBuildBuild = with pkgs; [
          mingw_w64_cc
        ];
        nativeBuildInputs = [ mingw_w64_cc ];
        # only add pthreads when building the final package, not when building the dependencies
        # otherwise it interferes with building build scripts
        overrideMain = args: args // { buildInputs = [ windows.pthreads ]; };

        CARGO_BUILD_TARGET = target;
      });

      buildLinux = target: naersk'.buildPackage (nearskOpt // {
        postInstall = addUdev;

        CARGO_BUILD_TARGET = target;
      } // (if (pkgs.hostPlatform.config != target) then (crossArgs.${target} or {}) else {}));
      buildAny = target: if (nixpkgs.lib.strings.hasInfix "windows" target) then (buildWindows target) else (buildLinux target);
    in rec {
      # `nix build`
      packages = nixpkgs.lib.attrsets.genAttrs targets buildAny // rec {
        palantir = naersk'.buildPackage (nearskOpt // {
          postInstall = addUdev;
        });
        check = naersk'.buildPackage (nearskOpt // {
          mode = "check";
        });
        clippy = naersk'.buildPackage (nearskOpt // {
          mode = "clippy";
        });
        default = palantir;
      };

      apps.palantir = utils.lib.mkApp {
        drv = packages.palantir;
      };
      defaultApp = apps.palantir;

      inherit targets;

      # `nix develop`
      devShells.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          toolchain
          bacon
          mingw_w64_cc
        ];
        depsBuildBuild = [ pkgs.wine64 ];
#        buildInputs = [ windows.pthreads ];

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
