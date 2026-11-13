#!/usr/bin/env bash
# Baixa o ExifTool que vai DENTRO do AppImage (src-tauri/exiftool-linux/).
#
# É Perl puro (script + lib/); o Perl do sistema existe em qualquer distro.
# O SICRO procura o exiftool primeiro nos recursos do pacote e só depois no PATH.
# Mesma versão do Windows (scripts/fetch-exiftool-windows.ps1).
#
# Uso (na raiz do repositório): scripts/fetch-exiftool-linux.sh
set -euo pipefail

Version="13.59"
Sha256="668ea3acececb7235fbd0f4900e72d5f12c9b07e5c778fd36cb1e9b5828fd65a"
# O exiftool.org só guarda a última versão; o SourceForge guarda todas.
Url="https://downloads.sourceforge.net/project/exiftool/Image-ExifTool-$Version.tar.gz"

Dest="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/exiftool-linux"
Tmp="${TMPDIR:-/tmp}/sicro-exiftool-linux-$Version"
mkdir -p "$Tmp"
Tar="$Tmp/exiftool.tar.gz"

echo "Baixando ExifTool $Version…"
# Agente de linha de comando: com navegador o SourceForge devolve a página, não o arquivo.
curl -fsSL -A "Wget/1.21" -o "$Tar" "$Url"
echo "$Sha256  $Tar" | sha256sum -c - >/dev/null || { echo "SHA-256 do ExifTool não confere" >&2; exit 1; }

rm -rf "$Dest"; mkdir -p "$Dest"
tar -xzf "$Tar" -C "$Dest" --strip-components=1 \
  "Image-ExifTool-$Version/exiftool" "Image-ExifTool-$Version/lib" "Image-ExifTool-$Version/README"
mv "$Dest/README" "$Dest/README-EXIFTOOL.txt"
chmod 755 "$Dest/exiftool"
cat > "$Dest/LEIA-ME-EXIFTOOL.txt" <<TXT
ExifTool $Version de Phil Harvey - https://exiftool.org
Distribuído junto com o SICRO sem modificações, sob os mesmos termos do Perl
(Artistic License ou GNU GPL). Usa o Perl do sistema.
Pacote original: $Url
SHA-256 do pacote: $Sha256
TXT

echo "ExifTool $Version pronto em $Dest"
