@echo off
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
for %%f in (fibb sumb mandel sieve ew trans reduce matmul) do cl /nologo /O2 /EHsc %%f.cpp /Fe%%f.exe >nul || exit /b 1
echo built
