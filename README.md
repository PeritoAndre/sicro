<div align="center">

<img src="public/branding/sicro-logo.png" alt="SICRO 2.0" width="116" />

# SICRO 2.0

**Suíte pericial forense — offline, local e reproduzível.**

Laudos institucionais, croquis, análise de imagem e vídeo, documentoscopia e
central de evidências, num único workspace `.sicro` autocontido.

![status](https://img.shields.io/badge/status-beta-orange)
![plataforma](https://img.shields.io/badge/plataforma-Windows%2010%2F11-1f6feb)
![offline](https://img.shields.io/badge/100%25-offline-2ea043)
![license](https://img.shields.io/badge/license-Apache--2.0-blue)

### ⬇️ [Baixar o instalador (beta)](https://github.com/PeritoAndre/SICRO-desktop/releases/latest)

<sub>Polícia Científica do Amapá</sub>

</div>

---

## O que é

O **SICRO 2.0** é uma ferramenta de **apoio** ao trabalho do perito criminal.
Reúne, num só programa que roda **100% offline**, tudo o que normalmente fica
espalhado por vários softwares: o laudo, os croquis, o tratamento de imagem e
vídeo, a leitura de documentos e o controle de integridade das evidências —
cada caso isolado num arquivo de workspace `.sicro`.

É **determinístico e honesto sobre seus limites**: realça, mede e organiza, mas
**nunca fabrica prova, nunca altera o original e não tira conclusões** — o
perito tem sempre a palavra final.

---

## Instalação (perito / usuário final)

1. Baixe o instalador mais recente em **[Releases](https://github.com/PeritoAndre/SICRO-desktop/releases/latest)** — arquivo `SICRO 2.0_<versão>_x64-setup.exe`.
2. Execute. O instalador pergunta se quer instalar **só para você** (sem
   administrador) ou **para todos os usuários** (requer administrador).
3. Leia e aceite o **Termo de Uso** e conclua. Pronto — atalho **SICRO 2.0** no
   menu Iniciar.

> **Requisitos:** Windows 10/11 (x64) e o runtime **WebView2** (já presente na
> maioria das instalações; o instalador orienta caso falte).
>
> Versão **beta** — em validação. Veja [`KNOWN_LIMITATIONS.md`](./KNOWN_LIMITATIONS.md)
> e relate problemas na aba **Issues**.

---

## Módulos

| Módulo | O que faz |
|---|---|
| **Home** | Central operacional: workspace ativo, ações rápidas (nova / abrir / importar / verificar integridade / backup), atalhos da ocorrência e casos recentes. |
| **Dossiê** | Workspace da ocorrência — metadados, fotos importadas e anotações. |
| **Laudo** | Editor A4 com paginação real, estilos, sumário, comentários e versões; templates institucionais; exportação **PDF / DOCX / HTML**; assinatura **gov.br** e **SIGDOCS**. |
| **Croqui** | Editor 2D de croquis **viário** (com importação do OpenStreetMap), **corporal** (lesões) e **planta baixa**; exportação PNG técnica para o laudo. |
| **Imagem** | Realce e análise **não-destrutiva**: galeria de filtros forenses (Sobel, CLAHE, ELA…), ajustes, zoom a nível de pixel, EXIF, histograma, múltiplos hashes (MD5/SHA-1/SHA-256/SHA-3), correção de perspectiva, anotações, medições e cadeia de custódia. |
| **Áudio** | Aquisição com custódia, realce de escuta, espectrograma e medições objetivas, recorte de trechos e degravação (manual + rascunho por IA local). |
| **Documentoscopia** | Leitura de documentos e PDFs por **OCR offline** + análise de erro de compressão (ELA), com a origem de cada resultado registrada. |
| **Evidências** | Central de integridade: registro de evidências, verificação de hashes, detecção de links quebrados e relatório técnico. |
| **Configurações** | Perfil do perito, instituição/marca, tema, integrações, OCR/IA e **backup geral** do acervo. |

Em evolução neste beta: **Vídeo**, **Mídias** e **Estatísticas**.

---

## Princípio de apoio

O SICRO existe para **dar suporte**, não para substituir o perito:

- **Offline e local** — nada sai da máquina; sem nuvem obrigatória, sem telemetria.
- **Nunca altera o original** — trabalha sobre cópias; a evidência de entrada é preservada.
- **Reproduzível e auditável** — registra a origem de cada resultado (qual ferramenta, quais parâmetros).
- **Sem conclusões automáticas** — realça e mede; a interpretação e a assinatura são do perito.

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
pnpm tauri build      # gera o instalador em src-tauri/target/release/bundle/
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
│   ├── core/                  # commands, errors, formatters
│   ├── design-system/         # tokens, tipografia
│   ├── components/            # UI compartilhada
│   ├── modules/               # home / laudo / croqui / imagem / audio / …
│   ├── stores/                # estado (Zustand)
│   └── types/                 # tipos espelhando structs Rust
├── src-tauri/                 # Backend nativo (Rust)
│   ├── src/commands/          # comandos expostos ao front
│   ├── src/workspace/         # workspace .sicro (criar, abrir, backup)
│   ├── src/database/          # SQLite + migrations + repositórios
│   ├── src/exporters/         # HTML / PDF / DOCX
│   ├── src/image_processing/  # filtros, EXIF, hashes
│   ├── src/evidence/          # registro + verificador de integridade
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
