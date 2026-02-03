#!/bin/bash

SCRIPTPATH=$( cd "$(dirname "$(readlink -f "$0")")" || exit 1 ; pwd -P )

cd "$SCRIPTPATH"

cargo run --bin stub_gen --no-default-features --features pyo3-embed
