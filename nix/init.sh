# stolen from https://github.com/NixOS/nixpkgs/blob/master/nixos/modules/system/boot/stage-1-init.sh
specialMount() {
  local device="$1"
  local mountPoint="$2"
  local options="$3"
  local fsType="$4"

  mkdir -m 0755 -p "$mountPoint"
  mount -n -t "$fsType" -o "$options" "$device" "$mountPoint"
}

source @earlyMountScript@
