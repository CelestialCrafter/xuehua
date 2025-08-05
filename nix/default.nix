{
  pkgs,
  lib,
  nixosSystem,
  compression ? {
    algorithm = "zstd";
    level = "10";
  },
}:

let
  scope = lib.makeScope pkgs.newScope (
    self: with self; {
      inherit compression;
      system = callPackage ./system.nix { inherit nixosSystem; };
      initrd = callPackage ./initrd.nix { };
      # utils should not have access to the full scope
      utils = pkgs.callPackage ./utils { inherit compression; };
    }
  );
in
{
  inherit (scope) system utils;
}
