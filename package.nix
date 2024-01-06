{
  stdenv,
  rustPlatform,
  coreutils,
  lib,
}: let
  inherit (lib.sources) sourceByRegex;
  src = sourceByRegex ./. ["Cargo.*" "(src|benches)(/.*)?"];
in
  rustPlatform.buildRustPackage rec {
    name = "palantir";
    version = "1.2.0";

    inherit src;

    cargoLock = {
      lockFile = ./Cargo.lock;
    };

    postInstall = ''
      mkdir -p $out/lib/udev/rules.d/
      echo 'SUBSYSTEM=="powercap", ACTION=="add", RUN+="${coreutils}/bin/chgrp -R powermonitoring /sys%p", RUN+="${coreutils}/bin/chmod -R g=u /sys%p"' >> $out/lib/udev/rules.d/51-palantir.rules
             echo 'SUBSYSTEM=="powercap", ACTION=="change", ENV{TRIGGER}!="none", RUN+="${coreutils}/bin/chgrp -R powermonitoring /sys%p", RUN+="${coreutils}/bin/chmod -R g=u /sys%p"' >> $out/lib/udev/rules.d/51-palantir.rules
    '';
  }
