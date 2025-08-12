{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixos-module-stripper = {
      url = "github:CelestialCrafter/nixos-module-stripper";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    collabora-linux = {
      url = "git+https://gitlab.collabora.com/hardware-enablement/rockchip-3588/linux.git?shallow=1";
      flake = false;
    };
    collabora-u-boot = {
      url = "git+https://gitlab.collabora.com/hardware-enablement/rockchip-3588/u-boot.git?ref=rk3588&shallow=1";
      flake = false;
    };
  };

  outputs =
    inputs@{
      nixpkgs,
      collabora-linux,
      collabora-u-boot,
      ...
    }:
    let
      pkgs-x86 = nixpkgs.legacyPackages.x86_64-linux;
      pkgs-aarch64 = nixpkgs.legacyPackages.aarch64-linux;
      systems =
        pkgs:
        let
          inherit (pkgs.callPackage ./nix { inherit (nixpkgs.lib) nixosSystem inputs; }) system utils;
          configuration = pkgs.callPackage ./configuration.nix;
        in
        pkgs.lib.genAttrs [ "scarameow" ] (
          identifier: (system [ (configuration { inherit utils identifier; }) ])
        );
    in
    {
      packages.aarch64-linux =
        with pkgs-aarch64;
        {
          u-boot = ubootRock5ModelB.overrideAttrs ({
            src = collabora-u-boot;
            patches = [ ];
          });

          linux = buildLinux rec {
            version = "6.15.0";
            modDirVersion = version;
            src = collabora-linux;
            extraMeta.branch = lib.version.majorMinor version;
          };
        }
        // systems pkgs-aarch64;

      packages.x86_64-linux.spl-loader =
        with pkgs-x86;
        stdenv.mkDerivation {
          name = "spl-loader";
          src = rkbin.src;
          buildPhase = "ls && tools/boot_merger RKBOOT/RK3588MINIALL.ini";
          installPhase = "cp rk3588_spl_loader_v*.bin $out";
        };

      devShells.x86_64-linux.default = pkgs-x86.mkShell {
        packages = with pkgs-x86; [
          apk-tools
          ubootTools
          just
          rkdeveloptool
          yq-go

          cargo
          rustc
          clang
        ];
      };
    };
}
