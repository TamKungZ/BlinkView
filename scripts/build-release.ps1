$ErrorActionPreference = 'Stop'
cargo build --release
Write-Host "Built: $PWD\target\release\blinkview.exe"
