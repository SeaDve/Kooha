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
cargo vendor-filterer --platform=x86_64-unknown-linux-gnu --platform=aarch64-unknown-linux-gnu > "$DIST"/.cargo/config
rm -f vendor/gettext-sys/gettext-*.tar.*
# remove the tarball from checksums
echo $(jq -c 'del(.files["gettext-0.21.tar.xz"])' vendor/gettext-sys/.cargo-checksum.json) > vendor/gettext-sys/.cargo-checksum.json
# Don't combine the previous and this line with a pipe because we can't catch
# errors with "set -o pipefail"
sed -i 's/^directory = ".*"/directory = "vendor"/g' "$DIST/.cargo/config"
# Move vendor into dist tarball directory
mv vendor "$DIST"
