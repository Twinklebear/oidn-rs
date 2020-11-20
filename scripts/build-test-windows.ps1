$env:WORK_DIR=(get-location)
$env:OIDN_DIR="${env:WORK_DIR}\oidn-${env:OIDN_VERSION}.x64.vc14.windows\"

Write-Output "Building oidn-rs"
cargo build
if (!$?) {
    exit 1
}

Write-Output "Running oidn-rs Tests"
cargo test
if (!$?) {
    exit 1
}

# build the examples
cd examples
Get-ChildItem .\ -Directory | ForEach-Object {
	Write-Output $_
	cd $_
	cargo build
	if (!$?) {
		exit 1
	}
	cd ..
}

