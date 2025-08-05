{ pkgs, ... }:

with pkgs;
{
  mkCache =
    {
      packages,
      sha256,
      version ? "latest-stable",
    }:
    let
      shellPackages = lib.escapeShellArgs packages;
    in
    stdenv.mkDerivation {
      name = "apk-cache";

      outputHash = sha256;
      outputHashAlgo = "sha256";
      outputHashMode = "recursive";

      dontUnpack = true;
      dontFixup = true;

      nativeBuildInputs = [
        apk-tools
        cacert
      ];

      buildPhase = ''
        mkdir -p etc/apk/cache
        ${lib.concatLines (
          map (url: "echo ${lib.escapeShellArg url} >> etc/apk/repositories") [
            "https://dl-cdn.alpinelinux.org/alpine/${version}/main"
            "https://dl-cdn.alpinelinux.org/alpine/${version}/community"
          ]
        )}

        apk add --root . --initdb --allow-untrusted alpine-keys
        apk cache download --update-cache --root . --add-dependencies -- ${shellPackages}
        echo ${shellPackages} > etc/apk/cache/installed

        mv etc/apk/cache $out
      '';
    };

  mkRoot =
    {
      version ? "latest-stable",
      sha256,
    }:
    stdenv.mkDerivation {
      # adding .tar.gz so mkSquashFS knows how to unpack it.
      name = "alpine-minirootfs.tar.gz";

      outputHash = sha256;
      outputHashAlgo = "sha256";

      dontUnpack = true;

      nativeBuildInputs = [
        curl
        cacert
        yq-go
      ];
      buildPhase = ''
        base="https://dl-cdn.alpinelinux.org/alpine/${lib.escapeURL version}/releases/${lib.escapeURL stdenv.hostPlatform.uname.processor}"
        file=$(curl $base/latest-releases.yaml | yq  '.[] | select(.flavor == "alpine-minirootfs") | .file')
        curl --fail-with-body --output $out $base/$file
      '';
    };
}
