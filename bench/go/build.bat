@echo off
rem Builds every Go benchmark into bench\go\<name>.exe
cd /d %~dp0
for %%t in (fibb sumb mandel sieve ew trans reduce matmul) do go build -o %%t.exe .\%%t\main.go
