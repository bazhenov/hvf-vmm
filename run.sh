#!/usr/bin/env bash
set -eo pipefail

EXE=target/release/vmm
OBJCOPY=/opt/homebrew/opt/binutils/bin/gobjcopy
AS=as

"$AS" --target=arm64 -march=armv8-a -nostdlib bootloader/main.S -o target/main.o
"$OBJCOPY" -O binary target/main.o target/main.bin

cargo build --release
codesign --sign - --entitlements entitlements.xml --deep --force ${EXE}
exec ${EXE}
