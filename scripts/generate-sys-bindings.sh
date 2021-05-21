#!/bin/bash

# This should be run on nightly rust, as it requires
# rustfmt-nightly to do the formatting

bindgen $1 -o $2 \
	--no-doc-comments \
	--distrust-clang-mangling \
	--allowlist-function "oidn.*" \
	--allowlist-type "OIDN.*" \
	--rust-target nightly
