{
  inputs = {
    utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
  };

  outputs = { self, nixpkgs, utils, naersk }:
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
        nativeBuildInputs = with pkgs; [ rustc cargo ];
      };
  }) // {
    nixosModule = { config, lib, pkgs, ... }:
    with lib;
    let cfg = config.palantir.services.palantir;
    in {
      options.palantir.services.palantir = {
        enable = mkEnableOption "Enables the palantir service";

	port = mkOption rec {
          type = types.int;
          default = 5665;
          example = default;
          description = "The port to listen on";
        };
      };

      config = mkIf cfg.enable {
        networking.firewall.allowedTCPPorts = [ cfg.port ];

        users.groups.palantir = {};
        users.users.palantir = {
	  isSystemUser = true;
	  group = "palantir";
	};

	services.udev.packages = [ self.defaultPackage.${pkgs.system} ];

        systemd.services."palantir" = {
          wantedBy = [ "multi-user.target" ];

          serviceConfig = let pkg = self.defaultPackage.${pkgs.system};
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
