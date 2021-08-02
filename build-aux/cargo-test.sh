#!/bin/sh

export MESON_SOURCE_ROOT="$1"

cargo test --manifest-path="$MESON_SOURCE_ROOT"/Cargo.toml
