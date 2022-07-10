{
  inputs = {
    utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
  };

  outputs = {
    self,
    nixpkgs,
    utils,
    naersk,
  }:
    utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages."${system}";
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
        nativeBuildInputs = with pkgs; [rustc cargo];
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

            users.groups.palantir = {};
            users.groups.powermonitoring = {};
            users.users.palantir = {
              isSystemUser = true;
              group = "palantir";
              extraGroups = ["powermonitoring"] ++ lib.optional cfg.docker "docker";
            };

            services.udev.packages = [self.defaultPackage.${pkgs.system}];

            systemd.services."palantir" = {
              wantedBy = ["multi-user.target"];
              path = lib.optional cfg.zfs pkgs.zfs;

              serviceConfig = let
                pkg = self.defaultPackage.${pkgs.system};
              in {
                Restart = "on-failure";
                ExecStart = "${pkg}/bin/palantir";
                User = "palantir";
                Environment = "PORT=${toString cfg.port}";
                PrivateTmp = true;
                ProtectSystem = "full";
                ProtectHome = true;
                NoNewPrivileges = true;
              };
            };
          };
        };
    };
}
