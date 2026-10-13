# Compares Easier with CPython on the benchmark programs. Run from anywhere:
#   powershell -File bench\run.ps1
param([string]$Python = "python")
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$easier = Join-Path $root "target\release\easier.exe"
if (-not (Test-Path $easier)) { throw "build first: cargo build --release" }
foreach ($name in "fib", "sieve", "sum") {
    $ez = Join-Path $PSScriptRoot "$name.ez"
    $py = Join-Path $PSScriptRoot "$name.py"
    $outEz = & $easier $ez
    $outPy = & $Python $py
    if ("$outEz" -ne "$outPy") { throw "${name}: easier printed '$outEz', python printed '$outPy'" }
    $tEz = (Measure-Command { & $easier $ez | Out-Null }).TotalSeconds
    $tPy = (Measure-Command { & $Python $py | Out-Null }).TotalSeconds
    "{0,-6} easier {1,7:N2}s   python {2,7:N2}s   {3,5:N1}x faster" -f $name, $tEz, $tPy, ($tPy / $tEz)
}
