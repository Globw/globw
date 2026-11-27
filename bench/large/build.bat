@echo off
rem Builds the Go side of the large benchmarks into bench\large\go\<name>.exe
cd /d %~dp0go
for %%t in (sumb sumb_par mandel mandel_par fill fill_par) do go build -o %%t.exe .\%%t\main.go
