#!/bin/sh
set -eu

# Only remove wrappers archived in 37a0c44, never installed tool binaries.
helper="$HOME/.local/bin/build-scope"
if [ -e "$helper" ] || [ -L "$helper" ]; then
  digest=$(sha256sum -- "$helper")
  if [ -L "$helper" ] || [ "${digest%% *}" != 4323d6817e692ec2a89f111edf42130f7d198aad9515331fed974795ecb67fe7 ]; then
    printf 'Refusing to remove modified build wrapper: %s\n' "$helper" >&2
    exit 1
  fi
fi

for tool in bevy cmake dx make ninja trunk wasm-opt wasm-pack; do
  path="$HOME/.local/bin/$tool"
  if [ -L "$path" ] && [ "$(readlink -- "$path")" = build-scope ]; then
    rm -- "$path"
  fi
done

cargo="$HOME/.local/share/cargo/bin/cargo"
if [ -L "$cargo" ]; then
  if [ "$(readlink -- "$cargo")" = ../../../bin/build-scope ]; then
    rm -- "$cargo"
  fi
elif [ -f "$cargo" ]; then
  digest=$(sha256sum -- "$cargo")
  if [ "${digest%% *}" = f8a7f52a424efbcb5a7a3ab395b22708d856399380bc3ea75383826dabdab86f ]; then
    rm -- "$cargo"
  fi
fi

if [ -f "$helper" ]; then
  rm -- "$helper"
fi
