{
  pkgs,
  lib,
  nixosSystem,
  initrd,
  inputs,
  ...
}:
extraModules:

with lib;
let
  alpine =
    { config, ... }:
    {
      options.environment = {
        alpinePackages = mkOption {
          default = [ ];
          example = [
            "zstd"
            "openrc"
            "tailscale"
          ];
          description = "packages to install from alpine repositories";
          type = types.listOf types.str;
        };
      };

      config.environment = {
        # systemPackages should be reserved for packages that aren't in alpine repos
        systemPackages = mkImageMediaOverride [ ];
        alpinePackages = with config; [
          (mkIf services.openssh.enable "openssh")
          (mkIf services.tailscale.enable "tailscale")
          (mkIf security.doas.enable "doas")
        ];
      };
    };

  defaults =
    { config, ... }:
    {
      options = {

        system.nixos.full = mkOption {
          description = "full name, including: distro name, nixos label, system name";
          readOnly = true;
          type = types.str;
        };

        environment.etcFilter =
          let
            list = mkOption {
              default = [ ];
              example = [
                [
                  "ssh"
                  "ssh_config"
                ]
                [ "doas.conf" ]
                [ "hostname" ]
                [ "fstab" ]
              ];
              description = "enables or disables paths in /etc via their components";
              type = types.listOf (types.listOf types.str);
            };
          in
          {
            whitelist = list // {
              default = [
                [ "hostname" ]
                [ "sysctl.d" ]
                [ "fstab" ]
              ];
            };
            blacklist = list;
          };
      };

      config = {
        boot.kernel.sysctl."kernel.poweroff_cmd" = mkForce null;
        networking.resolvconf.enable = false;
        systemd.coredump.enable = false;
        services.openssh.sftpServerExecutable = "internal-sftp";

        system = {
          nixos = {
            distroName = "Xuěhuā";
            full = "${config.system.nixos.distroName}-${config.system.nixos.label}-${config.system.name}";
          };
          stateVersion = config.system.nixos.release;
        };
      };
    };

  stripModule =
    path:
    pkgs.stdenvNoCC.mkDerivation {
      nativeBuildInputs = [ inputs.nixos-module-stripper.packages.${currentSystem}.default ];
      buildPhase = "cat $src | nixos-module-stripper > $out";
    };
in
nixosSystem ({
  system = pkgs.stdenv.hostPlatform.config;
  # baseModules = map stripModule (
    # filter (v: isPath v) (import "${pkgs.path}/nixos/modules/module-list.nix")
  # );
  modules = [
    initrd
    alpine
    defaults
  ]
  ++ extraModules;
})
