#!/usr/bin/env bash

set -euxo pipefail

# Format check first
cargo fmt --all -- --check

# Test all crates
cargo test --all

# Run configuration file checks
# cd ./source/motya
# cargo run -p motya -- --config-toml ./assets/example-config.toml --validate-configs
# cargo run -p motya -- --config-toml ./assets/test-config.toml --validate-configs
# cargo run -p motya -- --config-kdl ./assets/test-config.kdl --validate-configs
# cd ../../

# ensure the user manual can be built
cd user-manual
mdbook build
