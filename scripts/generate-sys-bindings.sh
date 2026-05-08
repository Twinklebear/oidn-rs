#!/usr/bin/env bash

# Regenerates the raw OIDN bindings with stable Rust-compatible output.

bindgen $1 -o $2 \
	--no-doc-comments \
	--distrust-clang-mangling \
	--allowlist-function "oidn.*" \
	--allowlist-type "OIDN.*"
