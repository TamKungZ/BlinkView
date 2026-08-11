$ErrorActionPreference = 'Stop'

$Root = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $Root

$Name = 'blinkview'
$Version = (Select-String -Path 'Cargo.toml' -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$Arch = if ([Environment]::Is64BitOperatingSystem) { 'x64' } else { 'x86' }
$Dist = Join-Path $Root 'dist'
$Build = Join-Path $Root 'target\package'
$Stage = Join-Path $Build "$Name-$Version-windows-$Arch"

cargo build --release

Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item 'target\release\blinkview.exe' (Join-Path $Stage 'blinkview.exe')
Copy-Item 'assets\blinkview.ico' (Join-Path $Stage 'blinkview.ico')
Copy-Item 'README.md','CHANGELOG.md','LICENSE','scripts\install-windows.ps1' $Stage

New-Item -ItemType Directory -Force -Path $Dist | Out-Null
$Zip = Join-Path $Dist "$Name-$Version-windows-$Arch.zip"
Compress-Archive -Path (Join-Path $Stage '*') -DestinationPath $Zip -Force
Write-Host "Built zip: $Zip"

$Wxs = Join-Path $Build 'BlinkView.wxs'
$Msi = Join-Path $Dist "$Name-$Version-windows-$Arch.msi"
@"
<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package Name="BlinkView" Manufacturer="TamKungZ_" Version="$Version" UpgradeCode="E7F03D8F-C19D-4C87-906A-7CB2A1B8F971">
    <MajorUpgrade DowngradeErrorMessage="A newer version of BlinkView is already installed." />
    <MediaTemplate EmbedCab="yes" />
    <StandardDirectory Id="LocalAppDataFolder">
      <Directory Id="INSTALLFOLDER" Name="BlinkView">
        <Component Id="BlinkViewExe" Guid="4E4E09F9-24C0-4A74-A93A-F068B75B1C3D">
          <File Id="BlinkViewExeFile" Source="$Stage\blinkview.exe" KeyPath="yes" />
          <RegistryValue Root="HKCU" Key="Software\Classes\Applications\blinkview.exe" Name="FriendlyAppName" Type="string" Value="BlinkView" />
          <RegistryValue Root="HKCU" Key="Software\Classes\Applications\blinkview.exe" Name="DefaultIcon" Type="string" Value="[INSTALLFOLDER]blinkview.ico" />
          <RegistryValue Root="HKCU" Key="Software\Classes\Applications\blinkview.exe\shell\open\command" Type="string" Value="&quot;[INSTALLFOLDER]blinkview.exe&quot; &quot;%1&quot;" />
        </Component>
        <Component Id="BlinkViewIcon" Guid="5BB10D86-B8E6-4F75-814E-93364B54AA72">
          <File Id="BlinkViewIconFile" Source="$Stage\blinkview.ico" KeyPath="yes" />
        </Component>
      </Directory>
    </StandardDirectory>
    <Feature Id="Main" Title="BlinkView" Level="1">
      <ComponentRef Id="BlinkViewExe" />
      <ComponentRef Id="BlinkViewIcon" />
    </Feature>
  </Package>
</Wix>
"@ | Set-Content -Encoding UTF8 $Wxs

$Wix = Get-Command wix -ErrorAction SilentlyContinue
if ($Wix) {
  & $Wix.Source build $Wxs -arch $Arch -o $Msi
  Write-Host "Built msi: $Msi"
} else {
  Write-Host 'Skipped msi: WiX Toolset v4 wix command not found.'
}

$Iss = Join-Path $Build 'BlinkView.iss'
$SetupExe = Join-Path $Dist "$Name-$Version-windows-$Arch-setup.exe"
@"
[Setup]
AppId={{7B28996C-A044-4E5A-8749-D90B6D588025}
AppName=BlinkView
AppVersion=$Version
AppPublisher=TamKungZ_
DefaultDirName={localappdata}\BlinkView
DisableProgramGroupPage=yes
OutputDir=$Dist
OutputBaseFilename=$Name-$Version-windows-$Arch-setup
SetupIconFile=$Stage\blinkview.ico
Compression=lzma
SolidCompression=yes
PrivilegesRequired=lowest

[Files]
Source: "$Stage\blinkview.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "$Stage\blinkview.ico"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
Root: HKCU; Subkey: "Software\Classes\Applications\blinkview.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "BlinkView"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\blinkview.exe"; ValueType: string; ValueName: "DefaultIcon"; ValueData: "{app}\blinkview.ico"
Root: HKCU; Subkey: "Software\Classes\Applications\blinkview.exe\shell\open\command"; ValueType: string; ValueData: """{app}\blinkview.exe"" ""%1"""

[Run]
Filename: "{app}\blinkview.exe"; Parameters: "--startup enable"; Flags: runhidden
"@ | Set-Content -Encoding UTF8 $Iss

$Iscc = Get-Command ISCC.exe -ErrorAction SilentlyContinue
if ($Iscc) {
  & $Iscc.Source $Iss
  Write-Host "Built setup exe: $SetupExe"
} else {
  Write-Host 'Skipped setup exe: Inno Setup ISCC.exe not found.'
}
