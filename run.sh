#!/usr/bin/env bash
set -eo pipefail

EXE=target/release/vmm
OBJCOPY=/opt/homebrew/opt/binutils/bin/gobjcopy
AS=as

"$AS" --target=arm64 -march=armv8-a -nostdlib bootloader/main.S
"$OBJCOPY" -O binary main.o main.bin

cargo build --release
codesign --sign - --entitlements entitlements.xml --deep --force ${EXE}
exec ${EXE}
