#!/usr/bin/env bash
# Gera os pacotes Linux (AppImage + .deb) e publica a release no GitHub.
#
# Uso (na máquina que compila — o desktop), na raiz do repositório:
#   scripts/release-linux.sh
#
# A versão vem do package.json; as notas, de docs/releases/v<versão>.md.
# Requer: mise (rust + pnpm) e o gh logado (gh auth status).
set -euo pipefail
cd "$(dirname "$0")/.."

V=$(node -p "require('./package.json').version")
TAG="v$V"
NOTES="docs/releases/$TAG.md"

die() { echo "✗ $*" >&2; exit 1; }

# ---- conferências antes de gastar 10 min compilando ----------------------
[ -f "$NOTES" ] || die "faltam as notas da versão: $NOTES"
grep -q "\"version\": \"$V\"" src-tauri/tauri.conf.json || die "tauri.conf.json não está em $V"
grep -q "^version = \"$V\"" src-tauri/Cargo.toml || die "Cargo.toml não está em $V"
[ -z "$(git status --porcelain)" ] || die "há mudanças não commitadas — commite antes"
git fetch -q origin
[ "$(git rev-parse HEAD)" = "$(git rev-parse '@{u}')" ] || die "o commit local não é o mesmo do GitHub — pull/push antes"
gh auth status >/dev/null 2>&1 || die "gh não está logado: gh auth login"
! gh release view "$TAG" >/dev/null 2>&1 || die "a release $TAG já existe"

# ---- build ---------------------------------------------------------------
mise exec -- pnpm install --frozen-lockfile
# NO_STRIP: o strip do linuxdeploy falha com binários de distros novas (Arch).
# APPIMAGE_EXTRACT_AND_RUN: o linuxdeploy roda mesmo sem o fuse2 instalado.
NO_STRIP=true APPIMAGE_EXTRACT_AND_RUN=1 mise exec -- pnpm tauri build --bundles appimage,deb

# ---- nomes limpos (o Tauri usa o productName, "SICRO 3.0", com espaço) ----
B=src-tauri/target/release/bundle
OUT="$B/release-$V"
rm -rf "$OUT" && mkdir -p "$OUT"
cp "$(ls "$B"/appimage/*_"$V"_*.AppImage)" "$OUT/SICRO-$V-x86_64.AppImage"
cp "$(ls "$B"/deb/*_"$V"_*.deb)" "$OUT/sicro_${V}_amd64.deb"
(cd "$OUT" && sha256sum SICRO-*.AppImage sicro_*.deb > SHA256SUMS.txt)
ls -lh "$OUT"
cat "$OUT/SHA256SUMS.txt"

# ---- publica -----------------------------------------------------------------
gh release create "$TAG" \
  --title "SICRO $V" \
  --notes-file "$NOTES" \
  --target "$(git rev-parse HEAD)" \
  "$OUT/SICRO-$V-x86_64.AppImage" "$OUT/sicro_${V}_amd64.deb" "$OUT/SHA256SUMS.txt"

echo "✓ publicada: $(gh release view "$TAG" --json url -q .url)"
