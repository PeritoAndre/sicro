#!/usr/bin/env bash
# Dispara a compilação da release (Linux + Windows) no GitHub Actions.
#
# Uso, na raiz do repositório, depois de subir a versão e escrever as notas:
#   scripts/release.sh
#
# O Actions compila nos dois sistemas e deixa a release como RASCUNHO, com os
# pacotes e o SHA256SUMS. Conferido, publica-se com:
#   gh release edit v<versão> --draft=false
set -euo pipefail
cd "$(dirname "$0")/.."

V=$(node -p "require('./package.json').version")
TAG="v$V"
SERIES="${V%.*}"

die() { echo "✗ $*" >&2; exit 1; }

[ -f "docs/releases/$TAG.md" ] || die "faltam as notas da versão: docs/releases/$TAG.md"
grep -q "\"version\": \"$V\"" src-tauri/tauri.conf.json || die "tauri.conf.json não está em $V"
grep -q "^version = \"$V\"" src-tauri/Cargo.toml || die "Cargo.toml não está em $V"
grep -q "\"title\": \"SICRO $SERIES\"" src-tauri/tauri.conf.json || die "título da janela não é \"SICRO $SERIES\" (tauri.conf.json)"
grep -q "<title>SICRO $SERIES</title>" index.html || die "título do index.html não é \"SICRO $SERIES\""
[ -z "$(git status --porcelain)" ] || die "há mudanças não commitadas — commite antes"
git fetch -q origin
[ "$(git rev-parse HEAD)" = "$(git rev-parse '@{u}')" ] || die "o commit local não é o mesmo do GitHub — pull/push antes"
gh auth status >/dev/null 2>&1 || die "gh não está logado: gh auth login"
if gh release view "$TAG" >/dev/null 2>&1 && [ "$(gh release view "$TAG" --json isDraft -q .isDraft)" != "true" ]; then
  die "a release $TAG já está publicada — suba a versão"
fi

gh workflow run release.yml --ref "$(git rev-parse --abbrev-ref HEAD)"
sleep 3
echo "✓ compilação disparada ($TAG). Acompanhe:"
echo "  gh run watch \$(gh run list --workflow release.yml -L1 --json databaseId -q '.[0].databaseId')"
echo "Depois confira o rascunho e publique: gh release edit $TAG --draft=false"
