$ErrorActionPreference = "Stop"

$compiler = (Get-Command clang-cl.exe -ErrorAction Stop).Source
$version = (& $compiler --version | Select-Object -First 1)
if ($version -notmatch 'clang version (\d+)\.' -or [int]$Matches[1] -lt 22) {
  # The MSVC STL on current runners requires Clang 22 or newer.
  $llvmInstaller = Join-Path $env:RUNNER_TEMP 'LLVM-22.1.8-win64.exe'
  $llvmRoot = Join-Path $env:RUNNER_TEMP 'llvm-22.1.8'
  Invoke-WebRequest -MaximumRetryCount 3 -Uri 'https://github.com/llvm/llvm-project/releases/download/llvmorg-22.1.8/LLVM-22.1.8-win64.exe' -OutFile $llvmInstaller
  if ((Get-FileHash $llvmInstaller -Algorithm SHA256).Hash -ne '16e5709785fef73c854646241c4a92c5cd574318d1b33c63330dd7721903e55c') {
    throw 'LLVM installer checksum mismatch'
  }
  $install = Start-Process $llvmInstaller -ArgumentList '/S', "/D=$llvmRoot" -PassThru -Wait
  if ($install.ExitCode -ne 0) { throw "LLVM installation failed: $($install.ExitCode)" }
  $llvmBin = Join-Path $llvmRoot 'bin'
  $compiler = (Get-Item (Join-Path $llvmBin 'clang-cl.exe')).FullName
  "LIBCLANG_PATH=$llvmBin" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
}
& $compiler --version
"CXX=$compiler" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"CXXFLAGS=-DNOMINMAX -D_WINSOCKAPI_ -D_CRT_SECURE_NO_WARNINGS -D_CRT_DECLARE_NONSTDC_NAMES=1" |
  Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
