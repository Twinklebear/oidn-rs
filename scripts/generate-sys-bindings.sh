#!/bin/bash

# This should be run on nightly rust, as it requires
# rustfmt-nightly to do the formatting

bindgen $1 -o $2 \
	--no-doc-comments \
	--distrust-clang-mangling \
	--whitelist-function "oidn.*" \
	--whitelist-type "OIDN.*" \
	--rustified-enum "OIDNDeviceType" \
	--rustified-enum "OIDNError" \
	--rustified-enum "OIDNFormat" \
	--rustified-enum "OIDNAccess" \
	--rust-target nightly

# Run some sed to polish up the enums
sed -i "s/OIDN_DEVICE_TYPE_//g" $2
sed -i "s/OIDN_ERROR_//g" $2
sed -i "s/OIDN_FORMAT_//g" $2
sed -i "s/OIDN_ACCESS_//g" $2

