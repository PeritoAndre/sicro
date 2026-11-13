#!/usr/bin/env bash
# Baixa o FFmpeg que vai DENTRO do AppImage (src-tauri/ffmpeg-linux/).
#
# O SICRO procura ffmpeg/ffprobe primeiro nos recursos do AppImage (usr/lib/SICRO/ffmpeg/)
# e só depois no PATH. O .deb e o .rpm não levam isto: declaram o ffmpeg da distro.
#
# Build estático de John Van Sickle (https://johnvansickle.com/ffmpeg/), GPLv3,
# com libx264 e vidstab. O endereço é rolante (sem versão no nome), por isso o
# SHA-256 é obrigatório: se mudar, atualize Version e Sha256 aqui.
#
# Uso (na raiz do repositório): scripts/fetch-ffmpeg-linux.sh
set -euo pipefail

Version="7.0.2"
Sha256="abda8d77ce8309141f83ab8edf0596834087c52467f6badf376a6a2a4c87cf67"
Url="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"

Dest="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/ffmpeg-linux"
Tmp="${TMPDIR:-/tmp}/sicro-ffmpeg-linux-$Version"
mkdir -p "$Tmp"
Tar="$Tmp/ffmpeg.tar.xz"

echo "Baixando FFmpeg $Version (estático, Linux x86_64)…"
curl -fsSL -o "$Tar" "$Url"
echo "$Sha256  $Tar" | sha256sum -c - >/dev/null || { echo "SHA-256 do FFmpeg não confere" >&2; exit 1; }

rm -rf "$Dest"; mkdir -p "$Dest"
tar -xJf "$Tar" -C "$Dest" --strip-components=1 --wildcards \
  "ffmpeg-$Version-amd64-static/ffmpeg" "ffmpeg-$Version-amd64-static/ffprobe" "ffmpeg-$Version-amd64-static/GPLv3.txt"
mv "$Dest/GPLv3.txt" "$Dest/LICENSE.txt"
chmod 755 "$Dest/ffmpeg" "$Dest/ffprobe"
cat > "$Dest/LEIA-ME-FFMPEG.txt" <<TXT
FFmpeg $Version (build estático de John Van Sickle - https://johnvansickle.com/ffmpeg/)
Distribuído junto com o SICRO sem modificações, sob a GPLv3 (ver LICENSE.txt).
Código-fonte do FFmpeg: https://ffmpeg.org/download.html
Pacote original: $Url
SHA-256 do pacote: $Sha256
TXT

echo "FFmpeg $Version pronto em $Dest"
ls -l "$Dest"
