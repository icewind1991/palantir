{ stdenv
, rustPlatform
, coreutils
, lib
,
}:
let
  inherit (lib.sources) sourceByRegex;
  inherit (builtins) fromTOML readFile;
  src = sourceByRegex ./. [ "Cargo.*" "(src|benches)(/.*)?" ];
  version = (fromTOML (readFile ./Cargo.toml)).package.version;
in
rustPlatform.buildRustPackage rec {
  pname = "palantir";

  inherit src version;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  doCheck = false;

  postInstall = ''
    mkdir -p $out/lib/udev/rules.d/
    echo 'SUBSYSTEM=="powercap", ACTION=="add", RUN+="${coreutils}/bin/chgrp -R powermonitoring /sys%p", RUN+="${coreutils}/bin/chmod -R g=u /sys%p"' >> $out/lib/udev/rules.d/51-palantir.rules
           echo 'SUBSYSTEM=="powercap", ACTION=="change", ENV{TRIGGER}!="none", RUN+="${coreutils}/bin/chgrp -R powermonitoring /sys%p", RUN+="${coreutils}/bin/chmod -R g=u /sys%p"' >> $out/lib/udev/rules.d/51-palantir.rules
  '';
}
