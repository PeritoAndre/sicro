<div align="center">

<img src="public/branding/sicro-logo.png" alt="SICRO" width="116" />

# SICRO 4.0

**Suíte pericial forense: offline, local e reproduzível.**

Croquis, vídeo e áudio e imagem — com a custódia de cada evidência — num
programa enxuto e num único arquivo de caso `.sicro`.

![versão](https://img.shields.io/github/v/release/PeritoAndre/sicro?label=vers%C3%A3o&color=d7a84f)
![plataforma](https://img.shields.io/badge/plataforma-Windows%20%7C%20Linux-1f6feb)
![offline](https://img.shields.io/badge/100%25-offline-2ea043)
![status](https://img.shields.io/badge/status-beta-orange)
![licença](https://img.shields.io/badge/licen%C3%A7a-Apache--2.0-blue)

### ⬇️ [Baixar a versão mais recente](https://github.com/PeritoAndre/sicro/releases/latest)

<sub>Polícia Científica do Amapá</sub>

</div>

---

## O que é

O SICRO é uma ferramenta de **apoio** ao perito criminal, objetiva: **três
módulos** para o trabalho técnico — desenhar o croqui, analisar vídeo e áudio,
tratar e medir imagem — e a **integridade** de cada evidência do caso. Cada caso
fica isolado num workspace `.sicro`.

Ele realça, mede e organiza, mas **nunca altera o original, nunca fabrica prova
e não tira conclusões**. A interpretação, a redação e a assinatura são do perito.

- **Offline e local:** nada sai da máquina. Sem nuvem obrigatória, sem telemetria.
- **Original intocado:** tudo é feito sobre cópias, com SHA-256 na entrada.
- **Reproduzível e auditável:** cada resultado registra a ferramenta, os parâmetros e o momento.
- **Honesto sobre limites:** estimativas aparecem como estimativas, com a margem de erro quando existe.

> **O laudo fica no Word ou no LibreOffice.** O SICRO não escreve laudo: ele
> entrega o que vai dentro dele — o PNG do croqui, os quadros coletados dos
> vídeos, as imagens tratadas, os tempos no formato de laudo — com hash e
> proveniência. A redação fica onde sempre esteve; o trabalho técnico fica aqui.

---

## Baixar e instalar

Os pacotes ficam em **[Releases](https://github.com/PeritoAndre/sicro/releases/latest)**.

| Sistema | Arquivo | Como instalar |
|---|---|---|
| **Windows 10/11 (64 bits)** | `SICRO-<versão>-windows-x64-setup.exe` | Execute. Instale só para você (sem administrador) ou para todos. O FFmpeg já vem junto. |
| **Ubuntu 22.04+, Debian 12+ e derivados** | `sicro_<versão>_amd64.deb` | `sudo apt install ./sicro_<versão>_amd64.deb` (o apt já instala o ffmpeg e os plugins de vídeo) |

Confira o download com o `SHA256SUMS.txt` da release: `sha256sum -c SHA256SUMS.txt`
no Linux, ou `Get-FileHash <arquivo>` no PowerShell.

**Windows**
- Requer o runtime **WebView2**, que já vem no Windows 11 e na maioria dos Windows 10. O instalador orienta se faltar.
- O instalador não tem assinatura digital. Se aparecer "O Windows protegeu o computador", clique em **Mais informações → Executar assim mesmo**.

**Linux**
- Vídeo e áudio usam o **ffmpeg** e o **GStreamer** do sistema; o `.deb` os declara como dependência.
- **Arch e outras distribuições:** ainda sem pacote pronto. O AppImage ficou de fora porque as bibliotecas que ele leva embutidas quebram fora do Ubuntu. Para o Arch, o caminho previsto é um pacote no AUR; por enquanto, compile do código (abaixo).
- Com placa **NVIDIA**, se a janela abrir em branco, rode com `WEBKIT_DISABLE_DMABUF_RENDERER=1` (limitação do WebKitGTK com o driver proprietário).

> Versão **beta**, em validação. Veja [`KNOWN_LIMITATIONS.md`](./KNOWN_LIMITATIONS.md)
> e relate problemas na aba **Issues**.

---

## Módulos

| Módulo | O que faz |
|---|---|
| **Croquis** | Editor 2D técnico: **viário** (vias paramétricas, rotatórias, 24+ veículos, vestígios, mapa do **OpenStreetMap**, fundo de foto ou drone), **corporal** (carta de lesões com numeração automática, 49 regiões) e **planta baixa** (paredes, mobiliário, evidências, trajetórias). Exporta PNG técnico com carimbo ou PNG limpo. |
| **Vídeo e Áudio** | **Vídeos:** registro com custódia (cópia local, SHA-256, `ffprobe`) e um reprodutor feito para perícia — **J/K/L** até 8×, quadro a quadro, **linha do tempo** com zoom até o quadro, trecho em repetição, eventos, **lupa** até 8×, brilho/contraste/gama só na tela, **relógio da câmera**, tela cheia. **Coleta de quadros** idênticos ao que aparece no player (PNG + JSON + hash), sequências, storyboard, **comparar duas câmeras**, **exportar trecho** sem recompressão, **velocidade** (homografia DLT, Monte Carlo reprodutível) e **distância**. **Áudios:** original + WAV de análise com hash, extração dos vídeos do caso, realce para escuta, **espectrograma**, medições (pico/RMS, clipping, **ENF**), recortes, compilação rotulada e **degravação** assistida offline (whisper.cpp; rascunho a revisar). |
| **Imagens** | Realce e análise **não destrutivos** (original + derivado reversível + JSON) de fotos do caso, quadros de vídeo ou arquivos: filtros forenses em Rust (bordas, desfoque, morfologia, CLAHE, níveis, nitidez, perspectiva de 4 pontos), máscaras, zoom no pixel, **EXIF**, medições com escala e múltiplos hashes (MD5, SHA-1, SHA-256, SHA-3). |
| **Integridade** | Pelo Início: tudo o que o caso guarda, conferido no disco (existência, SHA-256, vínculos), com relatório HTML auditável. Só leitura. |
| **Ajuda** | O [manual completo](./docs/MANUAL_SICRO.md) dentro do programa, com índice e busca. |
| **Configurações** | Perfil do perito, tema, **zoom da interface** (Ctrl+Shift+= / − / 0), backup geral e restauração, IA de degravação, atalhos editáveis e diagnóstico. |

> **Do 3.x para o 4.0:** Dossiê, Laudos, Documentoscopia e Estatísticas saíram —
> o SICRO ficou com o que é trabalho técnico. Os casos antigos abrem normalmente;
> o que esses módulos guardaram continua no disco e aparece na Integridade. A
> [3.1](https://github.com/PeritoAndre/sicro/releases/tag/v3.1.0) segue disponível.

---

## Backup

Regra de ouro: **caso vivo fica no disco local; a nuvem recebe só cópias
fechadas.** Um `.sicro` aberto dentro de pasta sincronizada pode corromper, e o
SICRO avisa se você tentar.

- **Por caso:** um `.sicrobackup` (ZIP) do workspace inteiro, com manifesto e SHA-256.
- **Geral, incremental:** todos os casos e a configuração para um HD externo, pendrive ou rede; só recopia o que mudou. Pode rodar sozinho ao fechar a ocorrência.
- **Restaurar:** aponte a pasta do backup e o SICRO recria os casos e a configuração. Trocou de computador? Instale, restaure e está tudo de volta.

---

## Desenvolvimento

**Tauri 2** (Rust) + **React 18** + TypeScript + **SQLite**. O front fica em
`src/`, o backend em `src-tauri/`.

| Ferramenta | Versão |
|---|---|
| [Node.js](https://nodejs.org/) | 22 |
| [pnpm](https://pnpm.io/) | 9 (`corepack enable`) |
| [Rust](https://rustup.rs/) | stable |
| FFmpeg (ffmpeg + ffprobe) | no PATH, para vídeo e áudio |

- **Windows:** [VS Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) ("Desktop development with C++") e WebView2. Antes do primeiro `tauri build`, rode `pwsh scripts/fetch-ffmpeg-windows.ps1` e `pwsh scripts/bundle-vcruntime-windows.ps1`: eles preparam o FFmpeg e o runtime do Visual C++ que vão dentro do instalador.
- **Linux:** `webkit2gtk-4.1`, `gtk3`, `librsvg`, `patchelf` e as ferramentas de compilação C. No Arch:
  `sudo pacman -S --needed webkit2gtk-4.1 gtk3 librsvg patchelf base-devel ffmpeg gst-plugins-good gst-libav`

```bash
pnpm install          # dependências do front
pnpm tauri dev        # abre o app com recarga automática
pnpm tauri build      # pacotes em src-tauri/target/release/bundle/

pnpm typecheck        # tipos do front
pnpm test             # testes do front (vitest)
cd src-tauri && cargo test --lib   # testes do backend
```

### Publicar uma versão

As releases são compiladas pelo **GitHub Actions**
([`.github/workflows/release.yml`](./.github/workflows/release.yml)), em Linux
(Ubuntu 24.04) e Windows.
Ninguém precisa compilar na própria máquina.

1. Suba a versão em `package.json`, `src-tauri/Cargo.toml` e `src-tauri/tauri.conf.json`
   (e o título "SICRO x.y" no `tauri.conf.json` e no `index.html`, se mudar a série).
2. Escreva as notas em `docs/releases/v<versão>.md`, commite e dê push.
3. Rode `scripts/release.sh`. Ele confere tudo e dispara o Actions, que deixa a
   release como **rascunho**, com o instalador do Windows, o `.deb` e o `SHA256SUMS.txt`.
4. Conferido o rascunho, publique: `gh release edit v<versão> --draft=false`.

---

## Estrutura

```
sicro/
├── .github/workflows/release.yml  # compila Linux + Windows e monta a release
├── docs/
│   ├── MANUAL_SICRO.md            # manual do usuário (exibido na Ajuda)
│   └── releases/                  # notas de cada versão
├── scripts/                       # release.sh e o preparo do instalador do Windows
├── src/                           # front: React + TypeScript
│   ├── app/                       # janela, barra lateral, título
│   ├── core/                      # comandos, atalhos, formatação
│   ├── components/                # interface compartilhada
│   └── modules/                   # croqui, vídeo, áudio, imagem, integridade, …
└── src-tauri/                     # backend: Rust
    ├── src/video/                 # probe, quadros, trechos, velocidade, distância
    ├── src/audio/                 # realce, espectrograma, medições, degravação
    ├── src/image_editor/          # filtros forenses, EXIF, hashes
    ├── src/database/              # SQLite, migrações, repositórios
    ├── src/workspace/             # workspace .sicro, backup, saúde
    ├── installer/                 # arte e Termo de Uso do instalador
    └── tauri.conf.json
```

Convenções: o **domínio em português** (`ocorrencia`, `laudo`, `vestigio`),
termos periciais sem equivalente direto; a **infraestrutura em inglês**.

---

## Licença

**Apache License 2.0**. Ver [`LICENSE`](./LICENSE). Copyright © 2026 André Ricardo Barroso.

O instalador do Windows inclui o **FFmpeg** (GPLv3) sem modificações, como
programa separado; licença e origem ficam em `ffmpeg\LEIA-ME-FFMPEG.txt`, na
pasta de instalação.
