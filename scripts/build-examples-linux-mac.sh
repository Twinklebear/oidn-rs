#!/usr/bin/env bash

# build the examples
cargo build --examples
if [[ "$?" != "0" ]]; then
	exit 1
fi

