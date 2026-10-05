# Baixa o ExifTool que vai DENTRO do instalador do Windows (src-tauri/exiftool-win/).
#
# O SICRO procura o exiftool primeiro em <pasta do SICRO>\exiftool\ e só depois no PATH.
# Versão fixa e conferida pelo SHA-256: pacote oficial de 64 bits de Phil Harvey
# (https://exiftool.org), com Perl portátil em exiftool_files\. Mesmos termos do Perl.
#
# Uso (na raiz do repositório): pwsh scripts/fetch-exiftool-windows.ps1
$ErrorActionPreference = "Stop"

$Version = "13.59"
$Sha256 = "44b512b25af500724ba579d0a53c8fc5851628b692dd5e5d94ae4a15c2cba9ec"
# O exiftool.org só guarda a última versão; o SourceForge guarda todas.
$Url = "https://downloads.sourceforge.net/project/exiftool/exiftool-$($Version)_64.zip"

$Dest = Join-Path $PSScriptRoot "..\src-tauri\exiftool-win"
$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) "sicro-exiftool-$Version"
New-Item -ItemType Directory -Force $Tmp | Out-Null
$Zip = Join-Path $Tmp "exiftool.zip"

Write-Host "Baixando ExifTool $Version…"
# Agente de linha de comando: com "Mozilla" o SourceForge devolve a página, não o arquivo.
Invoke-WebRequest -Uri $Url -OutFile $Zip -UserAgent "Wget/1.21"
$Got = (Get-FileHash $Zip -Algorithm SHA256).Hash.ToLower()
if ($Got -ne $Sha256) { throw "SHA-256 do ExifTool não confere: $Got (esperado $Sha256)" }

Expand-Archive $Zip -DestinationPath $Tmp -Force
$Root = Join-Path $Tmp "exiftool-$($Version)_64"

if (Test-Path $Dest) { Remove-Item -Recurse -Force $Dest }
New-Item -ItemType Directory -Force $Dest | Out-Null
# Renomeado: com "(-k)" no nome ele espera uma tecla ao terminar.
Copy-Item -LiteralPath (Join-Path $Root "exiftool(-k).exe") (Join-Path $Dest "exiftool.exe") -Force
Copy-Item -Recurse (Join-Path $Root "exiftool_files") (Join-Path $Dest "exiftool_files") -Force
if (Test-Path (Join-Path $Root "README.txt")) { Copy-Item (Join-Path $Root "README.txt") (Join-Path $Dest "README-EXIFTOOL.txt") -Force }
@"
ExifTool $Version de Phil Harvey - https://exiftool.org
Distribuido junto com o SICRO sem modificacoes (so o executavel foi renomeado de
"exiftool(-k).exe" para "exiftool.exe"), sob os mesmos termos do Perl
(Artistic License ou GNU GPL). Inclui um Perl portatil em exiftool_files\.
Pacote original: $Url
SHA-256 do pacote: $Sha256
"@ | Set-Content -Encoding UTF8 (Join-Path $Dest "LEIA-ME-EXIFTOOL.txt")

Write-Host "ExifTool $Version pronto em $Dest"
