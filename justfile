#!/usr/bin/env just --justfile

apk_base := "apk --arch aarch64 --root"
extra_pkgs := "busybox-openrc busybox-suid btrfs-progs musl-utils openrc mdevd doas util-linux \
openssh-server tailscale git"

set script-interpreter := ["sh", "-euo", "pipefail"]
set unstable

# utils

# internal - do not run manually
# creates alpine rootfs
[script]
alpine output:
    mkdir -p sources

    if [ ! -e sources/alpine.tar.gz ]; then
        base="https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/aarch64"
        file=$(curl $base/latest-releases.yaml | \
            yq '.[] | select(.flavor == "alpine-minirootfs") | .file')
        curl -o sources/alpine.tar.gz $base/$file
    fi

    tar -xzf sources/alpine.tar.gz -C {{output}}
    sed -i 's/v[0-9]\+\.[0-9]\+/edge/' {{output}}/etc/apk/repositories

# internal - do not run manually
# builds nix source if needed
[script]
nix-build package:
    mkdir -p sources
    if [ ! -e sources/{{package}} ]; then
        nix build .#packages.aarch64-linux.{{package}} --out-link sources/{{package}}
    fi

# internal - do not run manually
# creates an archives inside artifacts/, and updates the current archive symlink
[script]
pack directory name:
    file={{name}}-$(date +%F_%H-%M-%S).tar.zst

    cd artifacts
    tar --zstd -C {{directory}} -cf $file .
    sha256sum $file > $file.sha256
    ln -sf $file {{name}}-current.tar.zst
    ln -sf $file.sha256 {{name}}-current.tar.zst.sha256

# internal - do not run manually
# checks if effective uid is 0
[script]
enforce-root:
    if [ $(id -u) -ne 0 ]; then
        echo "effective user id is not 0, please run this as root!"
        exit 1
    fi

# internal - do not run manually
# de-symlinks etc and sets permissions
[script]
etc identifier output: enforce-root
    build_output=$(nix build .#packages.aarch64-linux.etc.{{identifier}} --no-link --print-out-paths)

    cp -aL $build_output/etc/. {{output}}
    find {{output}} -type f | \
        grep -e gid -e uid -e mode | \
        while read -r metadata; do
            echo "$metadata" | sed -E 's/^(.*)\.(gid|uid|mode)$/\1 \2/' | {
                read file type

                value=$(cat "$metadata")
                case $type in
                    gid)
                        chown ":$value" "$file"
                        ;;
                    uid)
                        chown "$value" "$file"
                        ;;
                    mode)
                        chmod "$value" "$file"
                        ;;
                esac
            }

            chmod o+w "$metadata"
            rm "$metadata"
        done

# outputs

# builds configs and package caches
[script]
extra identifier:
    # setup
    tmp=$(mktemp --directory)
    rootfs=$tmp/rootfs
    output=$tmp/output
    mkdir $rootfs $output
    # trap "rm -rf $tmp" EXIT

    # rootfs
    just alpine $rootfs
    just etc {{identifier}} $rootfs/etc
    mkdir $rootfs/etc/apk/cache

    {{apk_base}} $rootfs update
    {{apk_base}} $rootfs --add-dependencies cache download {{extra_pkgs}}
    echo {{extra_pkgs}} > $rootfs/etc/apk/cache/installed

    useradd --prefix $rootfs --shell /bin/sh \
        --user-group --groups users,wheel \
        --password $(mkpasswd -m sha512crypt {{identifier}}) {{identifier}}

    # output
    mv $rootfs/etc/apk/cache $output/cache

    echo $rootfs
    # just pack $output extra

# builds kernel, initramfs, ect
[script]
base: enforce-root
    just nix-build linux

    # setup
    tmp=$(mktemp --directory)
    rootfs=$tmp/rootfs
    output=$tmp/output
    mkdir $rootfs $output
    trap "chmod -R 755 $tmp && rm -rf $tmp" EXIT

    # rootfs
    just alpine $rootfs
    {{apk_base}} $rootfs --no-scripts add zstd zmap
    cp -r config/root/* $rootfs
    cp -r sources/linux/lib/* $rootfs/lib

    # output
    cp sources/linux/Image $output
    cp sources/linux/dtbs/rockchip/rk3588-rock-5b-plus.dtb $output/rock-5b-plus.dtb
    mkimage --architecture arm64 --type script --image config/boot.txt $output/boot.scr

    (cd $rootfs && find . | cpio --create --format newc | zstd) > \
        $tmp/initramfs.cpio.zst
    mkimage \
        --architecture arm64 \
        --type ramdisk \
        --compression zstd \
        --image $tmp/initramfs.cpio.zst $output/uInitrd

    just pack $output base

# flashes the device, follow link in README.md before using this
[script]
flash:
    just nix-build u-boot
    just nix-build spl-loader
    just enforce-root

    rkdeveloptool db sources/spl-loader
    rkdeveloptool wl 0 sources/u-boot/u-boot-rockchip-spi.bin
    rkdeveloptool rd
