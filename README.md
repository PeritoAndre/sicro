<div align="center">

<img src="public/branding/sicro-logo.png" alt="SICRO 3.0" width="116" />

# SICRO 3.0

**Suíte pericial forense — offline, local e reproduzível.**

Croquis, análise de imagem, áudio e vídeo, documentoscopia, central de
evidências e custódia do laudo, num único workspace `.sicro` autocontido.

![status](https://img.shields.io/badge/status-beta-orange)
![plataforma](https://img.shields.io/badge/plataforma-Windows%2010%2F11-1f6feb)
![offline](https://img.shields.io/badge/100%25-offline-2ea043)
![license](https://img.shields.io/badge/license-Apache--2.0-blue)

### ⬇️ [Baixar o instalador (beta)](https://github.com/PeritoAndre/sicro/releases/latest)

<sub>Polícia Científica do Amapá</sub>

</div>

---

## O que é

O **SICRO 3.0** é uma ferramenta de **apoio** ao trabalho do perito criminal.
Reúne, num só programa que roda **100% offline**, tudo o que normalmente fica
espalhado por vários softwares: os croquis, o tratamento de imagem, áudio e
vídeo, a leitura de documentos e o controle de integridade das evidências —
cada caso isolado num arquivo de workspace `.sicro`.

É **determinístico e honesto sobre seus limites**: realça, mede e organiza, mas
**nunca fabrica prova, nunca altera o original e não tira conclusões** — o
perito tem sempre a palavra final.

> **O laudo, no SICRO 3.0, é um documento `.docx`** escrito no Word ou no
> LibreOffice — editores robustos, que o perito já domina. O SICRO não substitui
> esse editor: ele **entrega o cabeçalho institucional, os campos da ocorrência
> e as figuras**, **mantém a custódia** dos arquivos e serve de **ponte** entre o
> caso e o documento. A redação fica onde sempre esteve; o trabalho técnico fica
> aqui.

---

## Instalação (perito / usuário final)

1. Baixe o instalador mais recente em **[Releases](https://github.com/PeritoAndre/sicro/releases/latest)** — arquivo `SICRO 3.0_<versão>_x64-setup.exe`.
2. Execute. O instalador pergunta se quer instalar **só para você** (sem
   administrador) ou **para todos os usuários** (requer administrador).
3. Leia e aceite o **Termo de Uso** e conclua. Pronto — atalho **SICRO 3.0** no
   menu Iniciar.

> **Requisitos:** Windows 10/11 (x64) e o runtime **WebView2** (já presente na
> maioria das instalações; o instalador orienta caso falte).
>
> Versão **beta** — em validação (ainda não testada em produção após o redesenho
> do módulo de laudo). Veja [`KNOWN_LIMITATIONS.md`](./KNOWN_LIMITATIONS.md)
> e relate problemas na aba **Issues**.

---

## Módulos

| Módulo | O que faz |
|---|---|
| **Home** | Central operacional: workspace ativo, ações rápidas (nova / abrir / importar / verificar integridade / backup), atalhos da ocorrência e casos recentes. |
| **Dossiê** | Workspace da ocorrência em duas lentes: **Operacional** (coleta de campo importada do pacote `.sicroapp` — resumo, fotos, checklist, entidades, vestígios, medições, timeline) e **Integridade / Central de Provas** (agrega toda a evidência do caso e confere em disco: existência, tamanho, SHA-256, links quebrados). |
| **Laudo** | O laudo é um `.docx` editado no Word/LibreOffice — **não há editor de texto no app**. O módulo é um **registro** de laudos por ocorrência + **ponte SICRO ↔ Word**: cria um `.docx` base (cabeçalho institucional + campos da ocorrência + esqueleto de seções), **abre no Word**, ou **registra um `.docx` já pronto**. Com o laudo selecionado, a **ponte** mostra dois painéis: **Consulta** (dados da ocorrência, só-leitura) e **Produção** (artefatos de imagem/croqui da Central de Evidências, cada um com **"Copiar pro laudo"** → coloca a imagem na área de transferência para colar com `Ctrl+V` no documento aberto). |
| **Croqui** | Editor 2D técnico em três frentes: **viário** (vias paramétricas via Parity Engine, rotatórias, 24+ silhuetas de veículo, vestígios, importação do **OpenStreetMap** via Overpass), **corporal** (carta de lesões com numeração POP automática — Anexo 1 SENASP, 49 regiões frente/costas — e 10 tipos de lesão) e **planta baixa** (paredes/mobiliário via Pixi + camada **pericial**: 10 tipos de evidência, trajetórias balísticas, estruturas, rosa dos ventos). Exporta **PNG técnico** (com carimbo: BO, município, escala, timestamp, rodapé §13) ou **PNG limpo**. |
| **Imagem** | Realce e análise **não-destrutiva** (original preservado + derivado reversível + sidecar JSON): galeria de filtros forenses em Rust puro (Sobel/Laplacian/Canny, Gaussian/Median/Bilateral, morfologia, CLAHE/equalização/auto-levels/white balance, unsharp/threshold/pixelize, perspectiva 4-pontos), seleção/máscaras, zoom a nível de pixel, **EXIF**, ajustes, medições com escala, múltiplos hashes (MD5/SHA-1/SHA-256/SHA-3-256) e cadeia de custódia. |
| **Áudio** | Aquisição com custódia (original preservado + WAV de análise, ambos com SHA-256), realce determinístico via FFmpeg (redução de ruído, corte de graves/agudos, normalização), **espectrograma** e medições objetivas (pico/RMS, fator de crista, clipping, ENF), recorte de trechos, compilação rotulada com manifesto e **degravação** assistida offline (whisper.cpp, rascunho a revisar). |
| **Vídeo** | Registro de vídeos com custódia (cópia local + SHA-256 + `ffprobe`), **coleta de frames** (PNG + sidecar), **Calculador de Velocidade** (calibração por homografia DLT 4-pontos ou linha de tráfego, regressão e Monte Carlo reprodutível) e **Medidor de Distância** (projeção pixel→mundo pela homografia calibrada). |
| **Documentoscopia** | Leitura de documentos e PDFs (cópia + hash) com **OCR offline**, layout, extração de campos, análise de integridade/metadados e **confronto** questionado × padrão, em três abas (Exame / Confronto / Relatório). A origem de cada resultado fica registrada. |
| **Estatísticas** | Painel **descritivo** (nunca interpretativo): contagens e distribuições do que o caso armazena, por caso ou geral, em gráficos próprios. Exporta HTML/CSV/JSON. |
| **Ajuda** | Manual do SICRO renderizado in-app a partir de `docs/MANUAL_SICRO.md`, com índice navegável e busca. |
| **Configurações** | Perfil do perito, instituição/marca (cabeçalho, brasões), tema, integrações (credenciais salvas no Gerenciador de Credenciais do Windows, nunca em JSON), caminhos padrão, atalhos, diagnóstico e **backup geral** do acervo. |

> **Assinatura digital.** O laudo é assinado **fora do SICRO** — no documento
> `.docx`/PDF, via Word + gov.br / SIGDOCS. O módulo Laudo apenas exibe um
> *badge* do tipo de assinatura (lido dos metadados) e prevê a **reimportação**
> do PDF assinado ao workspace (fluxo futuro). Não há fluxo de assinatura dentro
> do app.

---

## Princípio de apoio

O SICRO existe para **dar suporte**, não para substituir o perito:

- **Offline e local** — nada sai da máquina; sem nuvem obrigatória, sem telemetria.
- **Nunca altera o original** — trabalha sobre cópias; a evidência de entrada é preservada.
- **Reproduzível e auditável** — registra a origem de cada resultado (qual ferramenta, quais parâmetros).
- **Sem conclusões automáticas** — realça e mede; a interpretação, a redação e a assinatura são do perito.

---

## Backup e recuperação

Regra de ouro: **dados vivos ficam locais; a nuvem recebe backups estáticos.**
Um `.sicro` "vivo" dentro de uma pasta sincronizada (OneDrive/Drive) pode
corromper — por isso a pasta padrão de casos é local (`~\SICRO\Casos`) e o app
avisa se você apontar um caso para pasta sincronizada.

- **Por caso** — ZIP `.sicrobackup` do workspace inteiro, com manifesto + SHA-256.
- **Geral (incremental)** — copia todos os casos do acervo + o perfil/cabeçalhos
  para um destino (HD externo, pendrive, nuvem, rede); só recopia o que mudou.
  Pode rodar sozinho ao fechar a ocorrência.
- **Restaurar** — aponta a pasta de backup e o app recria os casos + a
  configuração. Cenário "trocou de PC → instala o SICRO → restaura → tudo de volta".

---

## 🛠️ Build do código-fonte (desenvolvedores)

Stack: **Tauri 2 + React 18 + TypeScript + Rust + SQLite**.

| Ferramenta | Versão | Onde |
|---|---|---|
| Node.js | 20.x (rec. 22+) | https://nodejs.org/ |
| pnpm | 9.x | `npm install -g pnpm` |
| Rust (stable) | 1.77+ | https://rustup.rs/ |
| WebView2 | (já vem no Windows 11) | https://developer.microsoft.com/microsoft-edge/webview2/ |
| VS Build Tools | "Desktop development with C++" | https://visualstudio.microsoft.com/visual-cpp-build-tools/ |

```powershell
pnpm install          # dependências do front
pnpm tauri dev        # app nativo com hot reload
pnpm tauri build      # gera o instalador em src-tauri/target/release/bundle/nsis/
```

Validações:

```powershell
pnpm typecheck        # type-check do front
pnpm test             # vitest (front)
cargo check           # type-check do backend (em src-tauri/)
cargo test            # testes do backend (em src-tauri/)
```

Pré-requisitos completos do Tauri: https://tauri.app/start/prerequisites/.

---

## Estrutura

```
sicro-desktop/
├── docs/MANUAL_SICRO.md       # Manual do usuário (exibido na tela Ajuda)
├── public/branding/           # Marca (logo, brasões)
├── src/                       # Frontend React + TypeScript
│   ├── app/                   # AppShell, ActivityRail, TopBar, StatusBar
│   ├── core/                  # commands, errors, formatters, keymap
│   ├── design-system/         # tokens, tipografia, reset
│   ├── components/            # UI compartilhada
│   ├── modules/               # home / dossie / laudo / croqui / imagem /
│   │   │                      #   audio / video / documentoscopia /
│   │   │                      #   estatisticas / evidencias / ajuda / configuracoes
│   │   └── laudo/             # registro de .docx + BridgeView (Consulta/Produção)
│   ├── stores/                # estado (Zustand)
│   └── types/                 # tipos espelhando structs Rust
├── src-tauri/                 # Backend nativo (Rust)
│   ├── src/commands/          # comandos expostos ao front (24 routers)
│   ├── src/workspace/         # workspace .sicro (criar, abrir, backup)
│   ├── src/database/          # SQLite + migrations + repositórios
│   ├── src/exporters/         # docx (base do laudo) / html / pdf / paths
│   ├── src/image_editor/      # filtros forenses, EXIF, máscaras, hashes
│   ├── src/audio/             # realce, espectrograma, medições, degravação
│   ├── src/video/             # frames, velocidade (homografia), distância
│   ├── src/registry/          # registro + verificador de integridade
│   ├── src/ocr/               # OCR offline
│   ├── installer/             # arte e Termo de Uso do instalador
│   └── tauri.conf.json
├── KNOWN_LIMITATIONS.md
└── package.json
```

---

## Convenções

- **Domínio em português** (`ocorrencia`, `laudo`, `croqui`, `vestigio`) — termos periciais sem equivalente direto.
- **Infraestrutura em inglês** (variáveis, utilitários, comandos) · **commits** em Conventional Commits.

---

## Licença

**Apache License 2.0** — ver [`LICENSE`](./LICENSE). Copyright © 2026 André Ricardo Barroso.

Livre para usar, copiar, modificar e redistribuir, mantendo o aviso de copyright
e a licença. Inclui concessão expressa de patentes com cláusula de represália defensiva.