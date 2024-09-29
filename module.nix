{ config
, lib
, pkgs
, ...
}:
with lib; let
  cfg = config.services.palantir;
in
{
  options.services.palantir = {
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

    logging = mkOption rec {
      type = types.str;
      default = "INFO";
      example = "WARN";
      description = "log level";
    };

    package = mkOption {
      type = types.package;
      description = "package to use";
    };
  };

  config = mkIf cfg.enable {
    networking.firewall.allowedTCPPorts = lib.optional cfg.openPort cfg.port;
    networking.firewall.allowedUDPPorts = lib.optional cfg.openMDNSPort 5353;

    users.groups.powermonitoring = { };

    services.udev.packages = [ cfg.package ];

    systemd.services."palantir" = {
      wantedBy = [ "multi-user.target" ];
      after = [ "systemd-networkd-wait-online.service" ];
      path = lib.optional cfg.zfs pkgs.zfs;
      environment =
        {
          PORT = toString cfg.port;
          RUST_LOG = cfg.logging;
          LD_LIBRARY_PATH = "/run/opengl-driver/lib/"; # needed for nvidia
        }
        // (
          if (cfg.mdns == false)
          then {
            DISABLE_MDNS = "true";
          }
          else { }
        );

      serviceConfig = {
        Restart = "on-failure";
        ExecStart = "${cfg.package}/bin/palantir";
        DynamicUser = true;
        PrivateTmp = true;
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
        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" "AF_NETLINK" ] ++ lib.optional cfg.docker "AF_UNIX"; # netlink is required to make `getifaddrs` not err
        RestrictRealtime = true;
        SystemCallFilter = [ "@system-service" "~@resources" "~@privileged" ];
        IPAddressAllow = [ "localhost" ] ++ lib.optional cfg.mdns "multicast";
        UMask = "0077";
        SupplementaryGroups = [ "powermonitoring" ] ++ lib.optional cfg.docker "docker";
      };
    };
  };
}
