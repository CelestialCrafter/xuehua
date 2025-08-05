{
  lib,
  stdenv,
  squashfsTools,
  compression,
}:
src:

stdenv.mkDerivation {
  inherit src;
  name = src.name + "-squashfs";
  nativeBuildInputs = [ squashfsTools ];
  sourceRoot = ".";
  buildPhase = ''
    SOURCE_DATE_EPOCH=0 mksquashfs . $out \
      -comp ${compression.algorithm} -Xcompression-level ${compression.level} \
      -b 1M -processors $NIX_BUILD_CORES
  '';
}
