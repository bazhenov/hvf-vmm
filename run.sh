#!/usr/bin/env bash
set -eo pipefail

EXE=target/release/vmm

cargo -q build --release
codesign --sign - --entitlements entitlements.xml --deep --force ${EXE}
exec ${EXE}
