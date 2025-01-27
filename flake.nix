{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-24.11";
    flakelight = {
      url = "github:nix-community/flakelight";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    mill-scale = {
      url = "github:icewind1991/mill-scale";
      inputs.flakelight.follows = "flakelight";
    };
  };
  outputs = { mill-scale, ... }: mill-scale ./. {
    packages.palantir = import ./package.nix;

    crossTargets = [
      "x86_64-pc-windows-gnu"
      "x86_64-unknown-linux-musl"
      "i686-unknown-linux-musl"
      "armv7-unknown-linux-musleabihf"
      "aarch64-unknown-linux-musl"
    ];

    nixosModules = { outputs, ... }: {
      default =
        { pkgs
        , config
        , lib
        , ...
        }: {
          imports = [ ./module.nix ];
          config = lib.mkIf config.services.palantir.enable {
            nixpkgs.overlays = [ outputs.overlays.default ];
            services.palantir.package = lib.mkDefault pkgs.palantir;
          };
        };
    };
  };
}
