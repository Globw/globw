# Packages this folder as a .vsix and installs it into VS Code.
#   powershell -File vscode-easier\install.ps1
$ErrorActionPreference = "Stop"
$src = $PSScriptRoot
$version = (Get-Content -Raw "$src\package.json" | ConvertFrom-Json).version
$stage = Join-Path $env:TEMP "easier-vsix-$version-$PID"
New-Item -ItemType Directory -Force "$stage\extension\syntaxes" | Out-Null
foreach ($f in "package.json", "language-configuration.json", "extension.js", "parse.js") {
    Copy-Item "$src\$f" "$stage\extension" -Force
}
Copy-Item "$src\syntaxes\easier.tmLanguage.json" "$stage\extension\syntaxes" -Force

$utf8 = New-Object System.Text.UTF8Encoding $false
[IO.File]::WriteAllText("$stage\[Content_Types].xml", @'
<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension=".json" ContentType="application/json"/><Default Extension=".js" ContentType="application/javascript"/><Default Extension=".vsixmanifest" ContentType="text/xml"/></Types>
'@, $utf8)
[IO.File]::WriteAllText("$stage\extension.vsixmanifest", @"
<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011">
  <Metadata>
    <Identity Language="en-US" Id="easier" Version="$version" Publisher="local"/>
    <DisplayName>Easier</DisplayName>
    <Description xml:space="preserve">Syntax colors, run button and live errors for the Easier pseudocode language (.ez)</Description>
    <Categories>Programming Languages</Categories>
    <Properties><Property Id="Microsoft.VisualStudio.Code.Engine" Value="^1.80.0"/></Properties>
  </Metadata>
  <Installation><InstallationTarget Id="Microsoft.VisualStudio.Code"/></Installation>
  <Dependencies/>
  <Assets><Asset Type="Microsoft.VisualStudio.Code.Manifest" Path="extension/package.json" Addressable="true"/></Assets>
</PackageManifest>
"@, $utf8)

$vsix = "$src\easier-$version.vsix"
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($stage, $vsix)
code --install-extension $vsix --force
