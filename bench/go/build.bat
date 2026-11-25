@echo off
rem Builds every Go benchmark into bench\go\<name>.exe
cd /d %~dp0
for %%t in (fibb sumb mandel sieve ew trans reduce matmul fill sumb_par mandel_par fill_par) do go build -o %%t.exe .\%%t\main.go
