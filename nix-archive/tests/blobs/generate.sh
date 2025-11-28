#!/bin/sh
set -e

RESET="\e[0m"
INFO="\e[1;34minfo:$RESET "
ERROR="\e[1;31merror:$RESET "
WARN="\e[1;33mwarning:$RESET "
HINT="\e[1;35mhint:$RESET "

expect_cmd() {
  if [ -z "$1" ]; then
    echo -e "${ERROR}expected program"
    exit 1
  fi

  if [ -z "$2" ]; then
    echo -e "${ERROR}expected reason"
    exit 1
  fi

  if ! command -v $1 &> /dev/null; then
    echo -e "${ERROR}$1 not found"
    echo -e "${HINT}$1 is needed to $2"

    exit 1
  fi
}

expect_cmd git "clone testing repos"
expect_cmd nix "pack testing directories"

REPO="https://github.com/rust-lang/rust"
TEMP=$(mktemp -d)
OUTPUT=$(dirname "$0")
trap "rm -rf $TEMP" EXIT

  echo -e "${INFO}cloning $REPO"
git clone --depth 1 $REPO $TEMP

echo -n "" > $OUTPUT/README.md
write() {
  echo -en "$1" >> $OUTPUT/README.md
}

pack() {
  echo -e "${INFO}packing $2.nar"
  nix nar pack "$TEMP/$1" > "$OUTPUT/$2.nar"
  write "- \`$2\`\n"
}

write "# Blobs\n\n"
write "Extracted from $REPO at $(git -C $TEMP rev-parse --short HEAD)"
write "\n\n## Files\n\n"
pack compiler rust-compiler
pack library/alloc rust-alloc
pack library/core rust-core
pack library/std rust-std
