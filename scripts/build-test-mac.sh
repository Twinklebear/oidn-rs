#!/bin/bash

export DYLD_LIBRARY_PATH=$DYLD_LIBRARY_PATH:${OIDN_DIR}/lib

echo "Building oidn-rs tests"
cargo test
if [[ "$?" != "0" ]]; then
    exit 1
fi

# build the examples
cd examples
for d in `ls ./`; do
	cd $d
	pwd
	cargo build
	if [[ "$?" != "0" ]]; then
		exit 1
	fi
	cd ../
done

