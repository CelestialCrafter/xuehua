{
  lib,
  pkgs,
  utils,
  compression,
  ...
}:
{ config, ... }:
with lib;
let
  init = pkgs.replaceVarsWith {
    src = ./init.sh;
    isExecutable = true;

    postInstall = ''
      echo checking syntax
      ${pkgs.pkgsBuildHost.busybox}/bin/sh -n $target
    '';

    replacements = {
      inherit (config.system.build) earlyMountScript;
    };
  };

  images = lib.mapAttrs (_: utils.mkSquashFS) {
    root = utils.alpine.mkRoot { sha256 = "sha256-GIQW1B+fDJpulCe3UUnkPM86iVh7LSfJrVBuf/ynjRw="; };
    extra =
      let
        usr = config.system.path;

        cache = utils.alpine.mkCache {
          packages = config.environment.alpinePackages;
          sha256 = "sha256-65j8sTskDHNXtmMGYZPmVzikZJw0qKoCSkqjT/BduzU=";
        };

        etc =
          with config.environment.etcFilter;
          let
            matches =
              list: file: builtins.any (prefix: lists.hasPrefix prefix (path.subpath.components file)) list;
            isFileAllowed = file: matches whitelist file && !matches blacklist file;
          in
          (pkgs.callPackage (import "${pkgs.path}/nixos/modules/system/etc/etc.nix") {
            config = {
              environment.etc = builtins.mapAttrs (
                _: file: file // { enable = isFileAllowed file.target; }
              ) config.environment.etc;
            };
          }).config.system.build.etc;
      in
      pkgs.runCommand "extra" { } ''
        mkdir -p $out/etc/apk/cache
        ln -s ${usr} $out/usr
        ln -s ${etc}/etc $out/etc
        ln -s ${cache} $out/etc/apk/cache
      '';
  };
in
{
  system.build.initialRamdisk = mkForce (
    pkgs.makeInitrd {
      name = "initrd-" + config.system.nixos.full;
      compressor = compression.algorithm;
      compressorArgs = [ "-${compression.level}" ];

      # @TODO remove this and let it be the default, instead just making the kernel a u-boot
      # makeUInitrd = true;

      contents = mapAttrsToList (symlink: options: {
        inherit symlink;
        object = options.source;
      }) ({ "/init".source = init; } // config.boot.initrd.extraFiles);
    }
  );

  # add images to initrd
  boot.initrd.extraFiles = mapAttrs' (
    name: image: (nameValuePair "/setup/${name}.img" { source = image; })
  ) images;
  fileSystems =
    let
      mounts = mapAttrs' (
        name: image:
        (nameValuePair "/run/overlay/${name}-lower" {
          fsType = "squashfs";
          device = "/setup/${name}.img";
          neededForBoot = true;
        })
      ) images;
    in
    mounts
    // {
      "/" = {
        neededForBoot = true;
        fsType = "overlay";
        device = "overlay";
        overlay = {
          workdir = "/run/overlay/work";
          upperdir = "/run/overlay/upper";
          lowerdir = mapAttrsToList (name: _: "/run/overlay/${name}-lower") images;
        };
      };
    };

  # restrict /usr directories
  environment.pathsToLink = mkImageMediaOverride [
    "/bin"
    "/lib"
    "/sbin"
    "/share"
  ];
}
