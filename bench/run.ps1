# Compares Globw with CPython on the benchmark programs. Run from anywhere:
#   powershell -File bench\run.ps1
param([string]$Python = "python")
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$globw = Join-Path $root "target\release\globw.exe"
if (-not (Test-Path $globw)) { throw "build first: cargo build --release" }
foreach ($name in "fib", "sieve", "sum") {
    $ez = Join-Path $PSScriptRoot "$name.gw"
    $py = Join-Path $PSScriptRoot "$name.py"
    $outEz = & $globw $ez
    $outPy = & $Python $py
    if ("$outEz" -ne "$outPy") { throw "${name}: globw printed '$outEz', python printed '$outPy'" }
    $tEz = (Measure-Command { & $globw $ez | Out-Null }).TotalSeconds
    $tPy = (Measure-Command { & $Python $py | Out-Null }).TotalSeconds
    "{0,-6} globw  {1,7:N2}s   python {2,7:N2}s   {3,5:N1}x faster" -f $name, $tEz, $tPy, ($tPy / $tEz)
}
