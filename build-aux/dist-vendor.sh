#!/bin/sh
# Since Meson invokes this script as
# "/bin/sh .../dist-vendor.sh DIST SOURCE_ROOT" we can't rely on bash features
set -eu
export DIST="$1"
export SOURCE_ROOT="$2"

cd "$SOURCE_ROOT"
mkdir "$DIST"/.cargo
# cargo-vendor-filterer can be found at https://github.com/coreos/cargo-vendor-filterer
# It is also part of the Rust SDK extension.
cargo vendor-filterer --all-features --platform=x86_64-unknown-linux-gnu --platform=aarch64-unknown-linux-gnu > "$DIST"/.cargo/config.toml
set -- vendor/gettext-sys/gettext-*.tar.*
TARBALL_PATH=$1
TARBALL_NAME=$(basename "$TARBALL_PATH")
rm -f "$TARBALL_PATH"
# remove the tarball from checksums
cargo_checksum='vendor/gettext-sys/.cargo-checksum.json'
tmp_f=$(mktemp --tmpdir='vendor/gettext-sys' -t)
jq -c "del(.files[\"$TARBALL_NAME\"])" "$cargo_checksum" > "$tmp_f"
mv -f "$tmp_f" "$cargo_checksum"
# Don't combine the previous and this line with a pipe because we can't catch
# errors with "set -o pipefail"
sed -i 's/^directory = ".*"/directory = "vendor"/g' "$DIST/.cargo/config.toml"
# Move vendor into dist tarball directory
mv vendor "$DIST"
