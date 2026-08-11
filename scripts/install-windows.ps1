$ErrorActionPreference = 'Stop'

cargo build --release

$InstallDir = Join-Path $env:LOCALAPPDATA 'BlinkView'
$Exe = Join-Path $InstallDir 'blinkview.exe'
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Force '.\target\release\blinkview.exe' $Exe

$AppKey = 'HKCU:\Software\Classes\Applications\blinkview.exe'
New-Item -Force -Path $AppKey | Out-Null
New-ItemProperty -Path $AppKey -Name 'FriendlyAppName' -PropertyType String -Value 'BlinkView' -Force | Out-Null
New-Item -Force -Path "$AppKey\shell\open\command" | Out-Null
Set-Item -Path "$AppKey\shell\open\command" -Value ('"{0}" "%1"' -f $Exe)
New-Item -Force -Path "$AppKey\SupportedTypes" | Out-Null

$Extensions = @(
  '.png','.jpg','.jpeg','.gif','.bmp','.webp','.tif','.tiff','.ico','.pnm','.ppm','.pgm','.pbm','.qoi',
  '.mp4','.mkv','.webm','.mov','.avi','.m4v','.mpg','.mpeg','.wmv','.flv','.ts','.mts','.m2ts'
)
foreach ($Ext in $Extensions) {
  New-ItemProperty -Path "$AppKey\SupportedTypes" -Name $Ext -PropertyType String -Value '' -Force | Out-Null
}

# Do not register a custom Explorer thumbnail COM handler here. Windows already
# has its own thumbnail providers for common media; replacing those just because
# BlinkView becomes the default app would be slower and more fragile. The core
# --thumbnail command remains available for unsupported shell integrations.
& $Exe --startup enable
Start-Process -FilePath $Exe -ArgumentList '--background' -WindowStyle Hidden

Write-Host "Installed: $Exe"
Write-Host 'BlinkView is registered in Open with and background startup is enabled.'
Write-Host 'Existing Windows Explorer image/video thumbnail providers are preserved.'
Write-Host 'Windows may still ask you to choose BlinkView manually as the default app.'
