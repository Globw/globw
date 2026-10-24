# Packages this folder as a .vsix and installs it into VS Code.
#   powershell -File vscode-globw\install.ps1
$ErrorActionPreference = "Stop"
$src = $PSScriptRoot
$version = (Get-Content -Raw "$src\package.json" | ConvertFrom-Json).version
$stage = Join-Path $env:TEMP "globw-vsix-$version-$PID"
New-Item -ItemType Directory -Force "$stage\extension\syntaxes" | Out-Null
New-Item -ItemType Directory -Force "$stage\extension\images" | Out-Null
$utf8 = New-Object System.Text.UTF8Encoding $false
# Copy text files stripping any UTF-8 BOM: VS Code's manifest reader rejects a
# BOM in package.json ("package.json is not a JSON file").
foreach ($f in "package.json", "language-configuration.json", "extension.js", "parse.js") {
    [IO.File]::WriteAllText("$stage\extension\$f", [IO.File]::ReadAllText("$src\$f"), $utf8)
}
[IO.File]::WriteAllText("$stage\extension\syntaxes\globw.tmLanguage.json", [IO.File]::ReadAllText("$src\syntaxes\globw.tmLanguage.json"), $utf8)
Copy-Item "$src\images\*" "$stage\extension\images" -Force
[IO.File]::WriteAllText("$stage\[Content_Types].xml", @'
<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension=".json" ContentType="application/json"/><Default Extension=".js" ContentType="application/javascript"/><Default Extension=".vsixmanifest" ContentType="text/xml"/><Default Extension=".png" ContentType="image/png"/><Default Extension=".svg" ContentType="image/svg+xml"/></Types>
'@, $utf8)
[IO.File]::WriteAllText("$stage\extension.vsixmanifest", @"
<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011">
  <Metadata>
    <Identity Language="en-US" Id="globw" Version="$version" Publisher="local"/>
    <DisplayName>Globw</DisplayName>
    <Description xml:space="preserve">Syntax colors, run button and live errors for the Globw language (.gw)</Description>
    <Categories>Programming Languages</Categories>
    <Properties><Property Id="Microsoft.VisualStudio.Code.Engine" Value="^1.80.0"/></Properties>
  </Metadata>
  <Installation><InstallationTarget Id="Microsoft.VisualStudio.Code"/></Installation>
  <Dependencies/>
  <Assets><Asset Type="Microsoft.VisualStudio.Code.Manifest" Path="extension/package.json" Addressable="true"/><Asset Type="Microsoft.VisualStudio.Services.Icons.Default" Path="extension/images/icon.png" Addressable="true"/></Assets>
</PackageManifest>
"@, $utf8)

$vsix = "$src\globw-$version.vsix"
if (Test-Path $vsix) { Remove-Item $vsix -Force }
# Build the zip entry by entry with forward-slash names: on Windows PowerShell
# (.NET Framework) CreateFromDirectory writes backslashes, which VS Code rejects
# ("package.json is not a JSON file") because it looks for "extension/package.json".
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::Open($vsix, "Create")
try {
    foreach ($file in Get-ChildItem -Recurse -File $stage) {
        $rel = $file.FullName.Substring($stage.Length + 1).Replace("\", "/")
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $file.FullName, $rel) | Out-Null
    }
} finally {
    $zip.Dispose()
}
Remove-Item $stage -Recurse -Force
code --install-extension $vsix --force
