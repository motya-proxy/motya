#!/usr/bin/env bash

set -euxo pipefail

# Format check first
cargo fmt --all -- --check

# Test all crates
cargo test --all

# ensure the user manual can be built (press 'X' to doubt)
# cd user-manual
# mdbook build
