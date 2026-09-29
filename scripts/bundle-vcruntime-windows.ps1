# Copia o runtime do Visual C++ (msvcp140*.dll, vcruntime140*.dll) para
# src-tauri/vcruntime-win/, de onde o instalador o põe na pasta do SICRO.
#
# O motor de OCR (ONNX Runtime, embutido no executável) usa a biblioteca C++
# da Microsoft; num Windows sem o "Visual C++ Redistributable" o SICRO nem
# abriria ("MSVCP140.dll não foi encontrado"). A Microsoft permite distribuir
# essas DLLs junto do programa ("app-local").
#
# Requer o Visual Studio / Build Tools com C++. Uso: pwsh scripts/bundle-vcruntime-windows.ps1
$ErrorActionPreference = "Stop"

$Vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$Vs = & $Vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $Vs) { throw "Visual Studio com C++ não encontrado (vswhere)" }

# VC\Redist\MSVC\<versão>\x64\Microsoft.VC14x.CRT\ — a versão mais nova.
$Crt = Get-ChildItem (Join-Path $Vs "VC\Redist\MSVC") -Directory |
  Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
  Sort-Object { [version]$_.Name } -Descending |
  ForEach-Object { Get-ChildItem (Join-Path $_.FullName "x64") -Directory -Filter "Microsoft.VC*.CRT" -ErrorAction SilentlyContinue } |
  Select-Object -First 1
if (-not $Crt) { throw "pasta Microsoft.VC*.CRT (x64) não encontrada em $Vs" }

$Dest = Join-Path $PSScriptRoot "..\src-tauri\vcruntime-win"
New-Item -ItemType Directory -Force $Dest | Out-Null
foreach ($f in "msvcp140.dll", "msvcp140_1.dll", "vcruntime140.dll", "vcruntime140_1.dll") {
  Copy-Item (Join-Path $Crt.FullName $f) $Dest -Force
}
Write-Host "Runtime do Visual C++ copiado de $($Crt.FullName)"
Get-ChildItem $Dest | Format-Table Name, Length
