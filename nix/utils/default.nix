{ pkgs, lib, compression, ... }:

lib.makeScope pkgs.newScope (self: with self; {
  inherit compression;
  openrc = callPackage ./openrc.nix { };
  alpine = callPackage ./alpine.nix { };
  mkSquashFS = callPackage ./squashfs.nix { };
})
