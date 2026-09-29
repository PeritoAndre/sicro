# Baixa o FFmpeg que vai DENTRO do instalador do Windows (src-tauri/ffmpeg-win/).
#
# O SICRO procura o ffmpeg/ffprobe primeiro em <pasta do SICRO>\ffmpeg\ e só
# depois no PATH — quase nenhum Windows tem ffmpeg instalado.
#
# Versão fixa e conferida pelo SHA-256: build "essentials" do Gyan Doshi
# (https://www.gyan.dev/ffmpeg/builds/, espelho no GitHub GyanD/codexffmpeg),
# GPLv3, com libx264 (usado em "Exportar trecho → Recomprimir").
#
# Uso (na raiz do repositório): pwsh scripts/fetch-ffmpeg-windows.ps1
$ErrorActionPreference = "Stop"

$Version = "9.0.2"
$Sha256 = "60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba"
$Url = "https://github.com/GyanD/codexffmpeg/releases/download/$Version/ffmpeg-$Version-essentials_build.zip"

$Dest = Join-Path $PSScriptRoot "..\src-tauri\ffmpeg-win"
$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) "sicro-ffmpeg-$Version"
New-Item -ItemType Directory -Force $Tmp | Out-Null
$Zip = Join-Path $Tmp "ffmpeg.zip"

Write-Host "Baixando FFmpeg $Version…"
Invoke-WebRequest -Uri $Url -OutFile $Zip
$Got = (Get-FileHash $Zip -Algorithm SHA256).Hash.ToLower()
if ($Got -ne $Sha256) { throw "SHA-256 do FFmpeg não confere: $Got (esperado $Sha256)" }

Expand-Archive $Zip -DestinationPath $Tmp -Force
$Root = Join-Path $Tmp "ffmpeg-$Version-essentials_build"

New-Item -ItemType Directory -Force $Dest | Out-Null
Copy-Item (Join-Path $Root "bin\ffmpeg.exe"), (Join-Path $Root "bin\ffprobe.exe") $Dest -Force
if (Test-Path (Join-Path $Root "LICENSE")) { Copy-Item (Join-Path $Root "LICENSE") (Join-Path $Dest "LICENSE.txt") -Force }
@"
FFmpeg $Version (build "essentials" de Gyan Doshi - https://www.gyan.dev/ffmpeg/builds/)
Distribuido junto com o SICRO sem modificacoes, sob a GPLv3 (ver LICENSE.txt).
Codigo-fonte do FFmpeg: https://ffmpeg.org/download.html
Pacote original: $Url
SHA-256 do pacote: $Sha256
"@ | Set-Content -Encoding UTF8 (Join-Path $Dest "LEIA-ME-FFMPEG.txt")

Write-Host "FFmpeg $Version pronto em $Dest"
Get-ChildItem $Dest | Format-Table Name, Length
