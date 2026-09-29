<div align="center">

<img src="public/branding/sicro-logo.png" alt="SICRO" width="116" />

# SICRO 3.1

**Suíte pericial forense: offline, local e reproduzível.**

Vídeo, áudio, imagem, croquis, documentoscopia e a custódia das evidências do
laudo, num só programa e num único arquivo de caso `.sicro`.

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

O SICRO é uma ferramenta de **apoio** ao perito criminal. Ele reúne o que
normalmente fica espalhado por vários programas: tratar e medir vídeo, áudio e
imagem, desenhar croquis, examinar documentos e manter a integridade de cada
evidência. Cada caso fica isolado num workspace `.sicro`.

Ele realça, mede e organiza, mas **nunca altera o original, nunca fabrica prova
e não tira conclusões**. A interpretação, a redação e a assinatura são do perito.

- **Offline e local:** nada sai da máquina. Sem nuvem obrigatória, sem telemetria.
- **Original intocado:** tudo é feito sobre cópias, com SHA-256 na entrada.
- **Reproduzível e auditável:** cada resultado registra a ferramenta, os parâmetros e o momento.
- **Honesto sobre limites:** estimativas aparecem como estimativas, com a margem de erro quando existe.

> **O laudo é um `.docx`**, escrito no Word ou no LibreOffice. O SICRO não
> substitui o editor de texto: ele gera o documento com cabeçalho institucional
> e campos da ocorrência, guarda a custódia e entrega as figuras prontas para
> colar. A redação fica onde sempre esteve; o trabalho técnico fica aqui.

---

## Baixar e instalar

Os pacotes ficam em **[Releases](https://github.com/PeritoAndre/sicro/releases/latest)**.

| Sistema | Arquivo | Como instalar |
|---|---|---|
| **Windows 10/11 (64 bits)** | `SICRO-<versão>-windows-x64-setup.exe` | Execute. Instale só para você (sem administrador) ou para todos. O FFmpeg já vem junto. |
| **Linux (qualquer distribuição)** | `SICRO-<versão>-linux-x86_64.AppImage` | `chmod +x` no arquivo e abra. Não instala nada no sistema. |
| **Debian, Ubuntu e derivados** | `sicro_<versão>_amd64.deb` | `sudo apt install ./sicro_<versão>_amd64.deb` |

Confira o download com o `SHA256SUMS.txt` da release: `sha256sum -c SHA256SUMS.txt`
no Linux, ou `Get-FileHash <arquivo>` no PowerShell.

**Windows**
- Requer o runtime **WebView2**, que já vem no Windows 11 e na maioria dos Windows 10. O instalador orienta se faltar.
- O instalador não tem assinatura digital. Se aparecer "O Windows protegeu o computador", clique em **Mais informações → Executar assim mesmo**.

**Linux**
- Distribuição de 2024 em diante: Ubuntu 24.04+, Debian 13+, Fedora 39+, Arch e derivados (o motor de OCR exige glibc 2.38 ou mais nova).
- O AppImage precisa do **FUSE 2**: `sudo pacman -S fuse2` (Arch) ou `sudo apt install libfuse2` (Debian/Ubuntu).
- Vídeo e áudio usam o **ffmpeg do sistema**: `sudo pacman -S ffmpeg` ou `sudo apt install ffmpeg`.
- Com placa **NVIDIA**, se a janela abrir em branco, rode com `WEBKIT_DISABLE_DMABUF_RENDERER=1` (limitação do WebKitGTK com o driver proprietário).

> Versão **beta**, em validação. Veja [`KNOWN_LIMITATIONS.md`](./KNOWN_LIMITATIONS.md)
> e relate problemas na aba **Issues**.

---

## Módulos

| Módulo | O que faz |
|---|---|
| **Vídeo** | Registro com custódia (cópia local, SHA-256, `ffprobe`) e um reprodutor feito para perícia: **J/K/L** até 8×, quadro a quadro, **linha do tempo** com zoom até o quadro e altura ajustável, trecho em repetição, eventos, **lupa** até 8×, brilho/contraste/gama só na tela, **relógio da câmera** e tela cheia. **Coleta de quadros** exatamente iguais ao que aparece no player (PNG + JSON + hash), sequências e storyboard. **Comparar duas câmeras** lado a lado, vinculadas pelo mesmo acontecimento ou pelo relógio. **Exportar trecho** como cópia registrada, sem recompressão. **Calculador de velocidade** (homografia DLT ou linha de tráfego, Monte Carlo reprodutível) e **medidor de distância**. |
| **Áudio** | Aquisição com custódia (original + WAV de análise, ambos com SHA-256), inclusive extraído dos vídeos do caso. Realce para escuta via FFmpeg (ruído, graves/agudos, normalização), **espectrograma**, medições objetivas (pico/RMS, fator de crista, clipping, **ENF**), recorte, compilação rotulada e **degravação** assistida offline (whisper.cpp; rascunho a revisar). |
| **Imagem** | Realce e análise **não destrutivos** (original + derivado reversível + JSON): filtros forenses em Rust (bordas, desfoque, morfologia, CLAHE, níveis, nitidez, perspectiva de 4 pontos), máscaras, zoom no pixel, **EXIF**, medições com escala e múltiplos hashes (MD5, SHA-1, SHA-256, SHA-3). |
| **Croqui** | Editor 2D técnico: **viário** (vias paramétricas, rotatórias, 24+ veículos, vestígios, mapa do **OpenStreetMap**), **corporal** (carta de lesões com numeração automática, 49 regiões) e **planta baixa** (paredes, mobiliário, evidências, trajetórias). Exporta PNG técnico com carimbo ou PNG limpo. |
| **Documentoscopia** | Documentos e PDFs com cópia e hash, **OCR offline**, extração de campos, leitura de QR e código de barras, análise de metadados e **confronto** questionado × padrão. |
| **Dossiê** | O caso em duas lentes: **Operacional** (coleta de campo importada do `.sicroapp`: fotos, checklist, vestígios, medições, linha do tempo) e **Integridade** (confere cada evidência em disco: existência, tamanho, SHA-256, vínculos). |
| **Laudo** | Registro dos laudos `.docx` do caso e **ponte com o Word**: cria o documento-base, abre no editor, registra um `.docx` pronto e copia as figuras do caso para colar com `Ctrl+V`. A assinatura é feita fora do SICRO (gov.br / SIGDOCS). |
| **Estatísticas** | Painel **descritivo** do que o caso guarda, por caso ou geral. Exporta HTML, CSV e JSON. |
| **Ajuda** | O [manual completo](./docs/MANUAL_SICRO.md) dentro do programa, com índice e busca. |
| **Configurações** | Perfil do perito, instituição e brasões, tema, **zoom da interface** (Ctrl+Shift+= / − / 0), atalhos editáveis, credenciais no cofre do sistema (nunca em arquivo), diagnóstico e backup geral. |

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
  `sudo pacman -S --needed webkit2gtk-4.1 gtk3 librsvg patchelf base-devel fuse2 ffmpeg`

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
   release como **rascunho**, com os três pacotes e o `SHA256SUMS.txt`.
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
│   └── modules/                   # vídeo, áudio, imagem, croqui, laudo, …
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
