{ pkgs, ... }:
let
  pname = "shot";
  lockfile = builtins.fromTOML (builtins.readFile ../../Cargo.lock);
  package = builtins.head (builtins.filter (package: package.name == pname) lockfile.package);
in
pkgs.rustPlatform.buildRustPackage {
  inherit pname;
  inherit (package) version;
  src = ../../.;
  cargoLock.lockFile = ../../Cargo.lock;
}
