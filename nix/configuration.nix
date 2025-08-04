args@{ pkgs, ... }:
identifier:

with (pkgs.callPackage ./openrc.nix { });
(import ./build-etc.nix args) {
  whitelist = [
    [ "ssh" ]
    [ "doas.conf" ]
    [ "hostname" ]
    [ "sysctl.d" ]
    [ "fstab" ]
  ];

  blacklist = [
    [
      "ssh"
      "ssh_config"
    ]
  ];

  nixosModule =
    { ... }:
    {
      # zram
      boot.kernel.sysctl = {
        "vm.swappiness" = 180;
        "vm.watermark_boost_factor" = 0;
        "vm.watermark_scale_factor" = 125;
        "vm.page-cluster" = 0;
        "vm.extfrag_threshold" = 0;
      };

      environment.etc."conf.d/zram-init".source = confd "zram-init" {
        numDevices = 2;
        size0 = 8000;
        para1 = "level=5";
      };

      environment.etc = {
        "conf.d/tailscale".source = confd "tailscale" { commandUser = "tailscale:tailscale"; };
        "network/interfaces".text = ''
          auto lo
          iface lo inet loopback

          auto eth0
          iface eth0 inet dhcp
        '';
        "udhcpc/udhcpc.conf".text = "RESOLV_CONF=\"no\"";
        "resolv.conf".text = ''
          # cloudflare
          nameserver 1.1.1.1
          nameserver 1.0.0.1
          nameserver 2606:4700:4700::1111
          nameserver 2606:4700:4700::1001

          # google
          nameserver 8.8.8.8
          nameserver 8.8.4.4
          nameserver 2001:4860:4860::8888
          nameserver 2001:4860:4860::8844
        '';
      };

      fileSystems = {
        "/mnt/deploy" = {
          label = "deploy";
          fsType = "ext4";
          options = [ "noauto" ];
        };

        "/var/lib" = {
          label = "data";
          fsType = "btrfs";
          options = [ "subvol=@var-lib" ];
        };

        "/home" = {
          label = "data";
          fsType = "btrfs";
          options = [ "subvol=@home" ];
        };
      };

      # openssh
      users.users.${identifier}.openssh.authorizedKeys.keys = [
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPbAoAEvQfpRnvRuYry1FE36kmLKFwywyC/TZGWHPAHM celestial.moe | 23/02/2025"
      ];

      environment.etc = {
        "motd".text = ''
               welcome back! [36m<3
          ──────────────────────────
          (B[m
        '';
        "conf.d/sshd".source = confd "sshd" { sshdDisableKeygen = true; };
      };

      services.openssh = {
        enable = true;
        settings = {
          PermitRootLogin = "no";
          PasswordAuthentication = false;
        };
      };

      # misc
      security.doas.enable = true;
      networking.hostName = "celestial-homelab-${identifier}";
    };
}
