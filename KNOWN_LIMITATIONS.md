# Limitações conhecidas — SICRO 5.2

O que o SICRO 5.2 **não faz** ou faz com ressalvas. Não é lista de bugs: é o
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
  pelo perito (o rastreamento da aba Rastreio não alimenta essas contas); o
  resultado é descritivo, com intervalo de confiança quando as incertezas são
  informadas.
- **Rastreio no vídeo (detecção, numeração e mancha) é apoio, não conclusão.**
  O perito confere e corrige antes de usar no laudo.
  - **Cruzamentos trocam números:** quando duas pessoas se cruzam ou andam coladas,
    o acompanhamento pode trocar os números ou criar uma pessoa nova ao reaparecer.
    Use *juntar* e *separar* na lista.
  - **Oclusão longa:** a pessoa escondida por mais de ~1 s (poste, veículo, grupo)
    volta com outro número, e some da imagem enquanto não é vista.
  - **Noite, contraluz e baixa resolução:** pessoas pequenas (câmera alta e
    distante), borradas ou no escuro podem não ser detectadas, e sombras ou objetos
    podem virar falso positivo. A detecção usa o modelo YOLOX-S em 640 px.
  - **Vídeo com fps variável:** a caixa de cada quadro usa o tempo real do quadro,
    mas o MP4 exportado sai em taxa constante (duração preservada; pode haver
    pequena diferença de sincronia em relação ao original).
  - **Outras classes (veículos, objetos):** o YOLOX-S reconhece as 80 classes do COCO,
    mas objetos pequenos de mão (celular, faca, garrafa) são detectados com pouca
    confiança e muitos falsos positivos; carro, moto e ônibus vistos de longe ou
    parcialmente cobertos também falham. Cada classe é acompanhada separadamente
    (um carro e uma pessoa nunca trocam de número entre si), mas duas classes podem
    marcar o mesmo objeto (ex.: carro e caminhão). Classes fora do COCO (arma, saco
    de lixo) não existem na rede: use a marcação à mão.
  - **Caixas na borda do quadro:** a caixa é recortada ao quadro e descartada se mais
    da metade dela cai fora; um objeto que acabou de entrar só ganha número depois de
    aparecer o suficiente.
  - **Mancha marcada à mão:** é correlação de pixels (sem rede neural). Não acompanha
    mudança de escala nem de ângulo; para quando algo parecido cruza na frente, quando
    a imagem fica muito diferente da marcada ou quando sai do quadro, e avisa onde
    parou, de propósito, em vez de arriscar seguir a coisa errada. Em pessoas andando,
    de longe e em vídeo comprimido, costuma parar em poucos quadros (recoloque a caixa
    e continue); veículos e objetos rígidos são seguidos por muito mais tempo. Caixa
    lisa (céu, parede) não pode ser seguida. Dentro da trilha manual não há rede
    conferindo: confira quadro a quadro.
  - **Só CPU:** a detecção roda no processador; não há aceleração por placa de vídeo
    nesta versão. O Windows não foi testado nesta versão (compila no Actions).
- **Realce de imagem no vídeo (aba Realce, 5.3) é apoio de leitura, não prova.** Nada
  substitui o original; cada saída guarda SHA-256, parâmetros e aviso.
  - **A média de quadros só tira ruído:** exige câmera e objeto quase parados na região;
    reflexo, movimento e compressão forte não somem. Quadros que não casam são
    descartados (a tela mostra quantos entraram). Rotação e escala grandes pedem o alinhamento
    por homografia e podem falhar em região lisa.
  - **A super-resolução inventa detalhe.** A rede Real-ESRGAN (general-x4v3) completa o que
    não existe na imagem: o resultado é verossímil, não necessariamente fiel. Não use para
    identificar pessoa ou placa sem o original ao lado. Só CPU: um quadro inteiro de 1080p
    leva alguns minutos; marque uma região. O arquivo do modelo vem de um repositório
    comunitário (hash fixo e conferido), não do lançamento oficial do projeto.
  - **A estabilização corta ou preenche a borda e recomprime o vídeo.** A cópia muda os
    pixels e o movimento da câmera: serve para ler a cena, não para medir (velocidade e
    distância usam o vídeo original). Sem o vidstab no ffmpeg, o `deshake` é mais fraco.
  - **Lente e perspectiva** supõem um modelo simples (distorção radial k1/k2; plano). A
    vista de cima só vale para o que está no plano marcado e depende das medidas reais
    informadas. A cópia do vídeo com a lente corrigida também é recomprimida.
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
