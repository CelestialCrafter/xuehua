{
  nixpkgs,
  pkgs,
  lib,
  ...
}:

{
  blacklist ? [ ],
  whitelist ? [ ],
  nixosModule ? { ... }: { },
}:

let
  # matches file against whitelist & blacklist
  isFileAllowed =
    with lib;
    let
      # automatically whitelist paths that are defined in environment.etc
      autoWhitelist = map path.subpath.components (
        builtins.attrNames (nixosModule { config = { }; }).environment.etc
      );
      mergedWhitelist = whitelist ++ autoWhitelist;
      matches =
        list: file: builtins.any (prefix: lists.hasPrefix prefix (path.subpath.components file)) list;
    in
    file: matches mergedWhitelist file && !matches blacklist file;

  defaultEtc =
    (lib.nixosSystem {
      modules = [
        nixosModule
        (
          { config, ... }:
          {
            # de-nixify
            boot.kernel.sysctl."kernel.poweroff_cmd" = lib.mkForce null;
            systemd.coredump.enable = false;
            services.openssh.sftpServerExecutable = "internal-sftp";

            # defaults
            nixpkgs.pkgs = pkgs;
            system.stateVersion = config.system.nixos.release;
          }
        )
      ];
    }).config.environment.etc;
  filteredEtc = builtins.mapAttrs (
    _: file: file // { enable = isFileAllowed file.target; }
  ) defaultEtc;
in
(pkgs.callPackage (import "${pkgs.path}/nixos/modules/system/etc/etc.nix") {
  config = {
    environment.etc = filteredEtc;
  };
}).config.system.build.etc
