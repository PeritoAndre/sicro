<div align="center">

<img src="public/branding/sicro-logo.png" alt="SICRO" width="116" />

# SICRO

**Suíte pericial: croqui, vídeo e áudio, imagem. Offline, local, auditável.**

![versão](https://img.shields.io/github/v/release/PeritoAndre/sicro?label=vers%C3%A3o&color=d7a84f)
![plataforma](https://img.shields.io/badge/plataforma-Windows%20%7C%20Linux-1f6feb)
![offline](https://img.shields.io/badge/100%25-offline-2ea043)
![licença](https://img.shields.io/badge/licen%C3%A7a-Apache--2.0-blue)

### ⬇️ [Baixar a versão mais recente](https://github.com/PeritoAndre/sicro/releases/latest)

<sub>Polícia Científica do Amapá</sub>

</div>

---

## O que é

Ferramenta de apoio ao perito criminal. Cada caso é uma pasta `.sicro` com tudo
dentro; nada sai do computador. O SICRO realça, mede e organiza — **nunca altera
o original e não tira conclusões**. O laudo continua no Word ou no LibreOffice;
o SICRO entrega o que vai dentro dele, com hash e proveniência.

Limitações conhecidas: [`KNOWN_LIMITATIONS.md`](./KNOWN_LIMITATIONS.md).

## Instalar

| Sistema | Arquivo | Como |
|---|---|---|
| **Windows 10/11** | `SICRO-<versão>-windows-x64-setup.exe` | Execute. O FFmpeg vem junto. |
| **Ubuntu 22.04+ / Debian 12+** | `sicro_<versão>_amd64.deb` | `sudo apt install ./sicro_<versão>_amd64.deb` |

Confira o download com o `SHA256SUMS.txt` da release.

- **Windows:** o instalador não tem assinatura digital — em "O Windows protegeu o computador", clique **Mais informações → Executar assim mesmo**.
- **Arch e outras distribuições:** sem pacote por enquanto; compile do código (abaixo).
- **NVIDIA no Linux:** janela em branco → rode com `WEBKIT_DISABLE_DMABUF_RENDERER=1`.

## Módulos

| | |
|---|---|
| **Croqui** | Viário (vias, rotatórias, veículos, vestígios, OpenStreetMap, foto ou drone de fundo), corporal (carta de lesões) e planta baixa. Exporta PNG com carimbo. |
| **Vídeo e Áudio** | Player pericial (J/K/L, quadro a quadro, lupa, relógio da câmera), coleta de quadros com hash, comparação de câmeras, exportar trecho, velocidade e distância. Áudio: realce, espectrograma, medições, ENF, autenticidade, locutores e degravação offline. |
| **Imagem** | Realce e análise não destrutivos: filtros forenses, máscaras, medições com escala, EXIF, hashes. |

No **Início** você dá um nome ao caso e entra no módulo. Pelo menu do caso:
**Integridade** (tudo o que o caso guarda, conferido no disco) e **backup**
(`.sicrobackup` do caso; em Configurações, backup geral e restauração).
O manual completo fica na **Ajuda**: [`docs/MANUAL_SICRO.md`](./docs/MANUAL_SICRO.md).

## Desenvolvimento

Tauri 2 (Rust) + React 18 + TypeScript + SQLite. Node 22, pnpm 9, Rust stable,
FFmpeg no PATH.

```bash
pnpm install
pnpm tauri dev                     # abre o app
pnpm tauri build                   # pacotes em src-tauri/target/release/bundle/
pnpm typecheck && pnpm test        # front
cd src-tauri && cargo test --lib   # backend
```

Windows: VS Build Tools (C++) e WebView2; antes do primeiro build,
`pwsh scripts/fetch-ffmpeg-windows.ps1`. Linux (Arch):
`sudo pacman -S --needed webkit2gtk-4.1 gtk3 librsvg patchelf base-devel ffmpeg gst-plugins-good gst-libav`.

### Publicar

1. Suba a versão em `package.json`, `src-tauri/Cargo.toml` e `src-tauri/tauri.conf.json`.
2. Escreva `docs/releases/v<versão>.md`, commite, dê push.
3. `scripts/release.sh` dispara o GitHub Actions (Linux + Windows), que deixa a release como rascunho.
4. `gh release edit v<versão> --draft=false`.

Não mude `.github/workflows/` enquanto o Actions compila.

## Estrutura

| Pasta | O que é |
|---|---|
| `src/` | Front em React: `app/` (janela e trilho), `modules/` (croqui, video, audio, imagem, home, integridade, configuracoes, ajuda), `core/` (comandos, atalhos), `components/`, `stores/`, `types/` |
| `src-tauri/` | Backend em Rust: `commands/` (API para o front), `database/` (SQLite e migrações), `workspace/` (pasta `.sicro`, backup), `video/`, `audio/`, `image_editor/`, `importer/` |
| `docs/` | Manual (exibido na Ajuda) e notas de cada versão |
| `scripts/` | Release e FFmpeg do instalador do Windows |
| `public/` | Logo |
| `.github/workflows/` | Compilação e release |

## Licença

Apache 2.0 — [`LICENSE`](./LICENSE). © 2026 André Ricardo Barroso.
O instalador do Windows inclui o FFmpeg (GPLv3) como programa separado;
licença e origem em `ffmpeg\LEIA-ME-FFMPEG.txt`.
