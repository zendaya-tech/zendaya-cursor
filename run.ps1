$ErrorActionPreference = 'Stop'
cargo run --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') @args
exit $LASTEXITCODE
