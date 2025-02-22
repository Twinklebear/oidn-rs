#!/bin/bash

export DYLD_LIBRARY_PATH=$DYLD_LIBRARY_PATH:${OIDN_DIR}/lib

echo "Building oidn-rs tests"
cargo test
if [[ "$?" != "0" ]]; then
    exit 1
fi

cargo test --examples
if [[ "$?" != "0" ]]; then
    exit 1
fi
