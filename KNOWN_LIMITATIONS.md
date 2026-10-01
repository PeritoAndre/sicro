# Limitações conhecidas — SICRO 4.0

O que o SICRO 4.0 **não faz** ou faz com ressalvas. Não é lista de bugs: é o
contrato honesto com quem usa.

## Instalação

- **Windows: instalador sem assinatura digital.** O SmartScreen pode mostrar "O
  Windows protegeu o computador" — **Mais informações → Executar assim mesmo**.
- **Linux: só `.deb` (Ubuntu 22.04+, Debian 12+).** Ainda não há AppImage: as
  bibliotecas que ele leva embutidas (WebKit/GStreamer do Ubuntu) quebram em
  distribuições mais novas. Para o Arch, o caminho previsto é um pacote no AUR.
- **Linux com placa NVIDIA:** se a janela abrir em branco, rode com
  `WEBKIT_DISABLE_DMABUF_RENDERER=1` (limitação do WebKitGTK com o driver
  proprietário).

## Vídeo e Áudio

- **Quadro ≈ é estimado** pelo fps declarado do arquivo. Em vídeo com fps
  variável, o número do quadro é aproximado; cada quadro coletado registra o
  tempo real entregue pelo FFmpeg.
- **Exportar trecho sem recompressão** começa no quadro-chave anterior à entrada
  e pode levar alguns quadros depois da saída (o SICRO informa quanto).
  **Recomprimir** corta exato, mas os pixels deixam de ser os do original.
- **H.265 (HEVC) no Windows** depende do computador; se não tocar, instale as
  "Extensões de Vídeo HEVC" da Microsoft Store.
- **Esc com um menu aberto em tela cheia** também sai da tela cheia no Linux (o
  WebKit trata o Esc antes da página).
- **Velocidade e distância são medições com incerteza**: os pontos são marcados
  pelo perito, não há rastreamento automático; o resultado é descritivo, com
  intervalo de confiança quando as incertezas são informadas.
- **Degravação por IA é rascunho**: pode errar ou "inventar" texto em ruído e
  silêncio. Cada linha precisa ser revisada antes de ir ao laudo.
- **ENF e realces de áudio** são apoio à escuta e indícios, não conclusão.

## Imagens

- Filtros forenses (ELA, realces) são **indícios** que exigem exame humano;
  bordas e alto contraste podem dar falso-positivo.

## Casos e custódia

- **Casos do 3.x:** o que Dossiê, Laudos, Documentoscopia e Estatísticas
  guardaram continua na pasta do caso e aparece na **Integridade** (em
  *Todas*), mas não há mais tela para abrir esses dados. A 3.1 segue disponível
  para isso.
- **Um caso por máquina:** não abra o mesmo `.sicro` em dois computadores ao
  mesmo tempo; para levar um caso, use o backup (`.sicrobackup`).
- **Pasta sincronizada** (OneDrive, Google Drive, Dropbox) pode corromper um caso
  aberto; o SICRO avisa. Mantenha os casos em pasta local.
- **Registro de operações** (aba Logs da Integridade) cobre vídeo e importação;
  croqui e imagem ainda não registram log estruturado.

## Fora do escopo

- O SICRO **não escreve nem assina laudo**: o laudo fica no Word ou no
  LibreOffice, e a assinatura nos portais (SIGDOCS, gov.br).
- Não há nuvem, conta ou sincronização: tudo é local e offline.
