#!/bin/sh
# Optional static build. The container image is the supported way to run this.
set -eu
cd "$(dirname "$0")/.."
if ! rustup target list --installed | grep -q x86_64-unknown-linux-musl; then
  rustup target add x86_64-unknown-linux-musl
fi
( cd frontend && npm ci && npm run build )
cargo build --release --locked --target x86_64-unknown-linux-musl
echo "target/x86_64-unknown-linux-musl/release/relaybook-dashboard"
