# Manual do SICRO 4.0 — Suíte Pericial

> **Para quem é este manual:** peritos criminais e equipe técnica que usam o
> SICRO no dia a dia. Ele ensina, módulo por módulo, **o que cada parte faz
> e como usar** — com passos, dicas e os limites honestos de cada ferramenta.
>
> **O que é o SICRO 4.0:** uma suíte pericial **desktop, 100% offline**, para
> Windows e Linux, enxuta e objetiva: **Croquis**, **Vídeo e Áudio** e
> **Imagens**, com a **integridade** de todas as evidências do caso. O laudo
> continua no Word ou no LibreOffice; o SICRO entrega as figuras e as medições.
>
> **Princípio que rege tudo (§13):** o SICRO é uma **ferramenta de apoio**. Ele
> organiza, mede, calcula e documenta — mas **nunca conclui no seu lugar e nunca
> inventa prova**. O original nunca é alterado, tudo é reproduzível, e a palavra
> final é sempre do perito.
>
> **Vindo do 3.x?** Dossiê, Laudos, Documentoscopia e Estatísticas saíram. Os
> casos antigos abrem normalmente; o que esses módulos guardaram continua no
> disco e aparece na Integridade.

---

## Sumário

1. [Conceitos fundamentais](#1-conceitos-fundamentais)
2. [A janela e a navegação](#2-a-janela-e-a-navegação)
3. [Início (Home)](#3-início-home)
4. [Integridade](#4-integridade)
5. [Croquis](#5-croquis)
6. [Imagens](#6-imagens)
7. [Vídeo e Áudio](#7-vídeo-e-áudio)
8. [Configurações](#8-configurações)
9. [Fluxo ponta a ponta](#9-fluxo-ponta-a-ponta)
10. [Limites honestos (§13)](#10-limites-honestos-13)
11. [Glossário e extensões de arquivo](#11-glossário-e-extensões-de-arquivo)

---

## 1. Conceitos fundamentais

Antes de entrar nos módulos, três ideias explicam **como o SICRO pensa**.

### 1.1 Ocorrência = workspace `.sicro`

Cada caso é uma **pasta `.sicro`** autocontida no seu computador. Dentro dela
fica tudo do caso: o banco de dados (SQLite), o manifesto, e subpastas para
croquis, vídeos, áudios, imagens, exportações, backups e relatórios.

- **Autocontido:** mover a pasta `.sicro` move o caso inteiro.
- **A verdade está no disco:** o app é só a janela que lê e edita esses arquivos.
- **Um caso por máquina:** evite abrir o mesmo `.sicro` em dois PCs ao mesmo
  tempo. Para transportar um caso, use o **backup** (item 3.6), não a cópia da
  pasta viva.

### 1.2 Offline por design

O SICRO funciona **sem internet**. Não há servidor central, login na nuvem nem
sincronização automática. As únicas coisas que tocam a rede são opcionais e
explícitas: importar vias do OpenStreetMap (Croqui) e baixar o motor de IA da
degravação (Configurações), só quando você clica.

> 💡 **Por que isso importa:** dado pericial fica sob seu controle, no seu disco.
> A redundância em nuvem é feita por **backup** (um arquivo único), nunca
> sincronizando o banco vivo — sync pode corromper o SQLite.

### 1.3 Integridade e original intacto

- Cada arquivo de evidência tem **hash SHA-256** registrado. Se alguém alterar
  o arquivo por fora, o SICRO detecta (a integridade "não bate").
- **O original nunca é alterado.** Tratar uma foto, aplicar filtro, recortar um
  áudio ou exportar um trecho de vídeo — tudo isso gera **derivados**; o
  original permanece com seu hash, na custódia.
- **Reprodutível:** as operações ficam registradas (pilha de filtros, histórico)
  e podem ser refeitas igual.

---

## 2. A janela e a navegação

### 2.1 Barra de título

No topo, uma **barra escura** com a marca SICRO à esquerda e os botões de
**minimizar / maximizar / fechar** à direita. Você arrasta a janela por ela e
redimensiona pelas bordas, como qualquer janela.

### 2.2 Trilho lateral (esquerda)

A coluna fixa à esquerda é a navegação principal. Em **Módulos**:

- **Início** — central de ocorrências (e, por ele, a **Integridade** do caso).
- **Croquis** — viário, corporal e planta baixa.
- **Vídeo e Áudio** — análise de vídeo e de áudio, em duas abas.
- **Imagens** — editor de imagem pericial.

Abaixo, separados: **Configurações** e **Ajuda** (este manual).

No rodapé do trilho: o **card do perito** (puxado de Configurações → Perfil), o
indicador **Local · Offline** e a **versão** do app.

### 2.3 Barra de status (rodapé)

Mostra o contexto atual (workspace ativo, modo de trabalho, contadores). Em
módulos com tela (imagem, croqui) ela também traz controles de **zoom**.

### 2.4 Barra do topo

Mostra onde você está (módulo e ocorrência ativa). No módulo **Vídeo e Áudio**
ela traz as abas **Vídeos | Áudios** — o módulo lembra a última aba usada.

> ⚠️ **Quase tudo exige uma ocorrência ativa.** Sem um caso aberto, os módulos
> ficam em modo "vazio" pedindo que você crie ou abra uma ocorrência.

---

## 3. Início (Home)

**Para que serve:** é a central de onde você cria, abre e administra ocorrências,
e de onde dispara backup e verificação de integridade.

### 3.1 O que tem na tela

- **Cartão do workspace ativo** (se um caso está aberto): rótulo do caso, caminho,
  status e o botão **Continuar ocorrência**, que volta ao último módulo em que
  você trabalhou.
- **Estado vazio** (sem caso aberto): botões **Nova ocorrência** e **Abrir
  workspace**.
- **Painel de ações:** sempre *Nova ocorrência*, *Abrir workspace* e *Importar
  .sicroapp*. Com um caso aberto, também *Concluir* (ou *Reabrir*) *ocorrência*,
  *Propriedades*, *Verificar integridade*, *Gerar backup*, *Relatório de saúde*,
  *Abrir pasta* e *Fechar ocorrência*.
- **Histórico de ocorrências:** tabela de todos os casos, com busca e filtro de
  data.
- **Feedback** (no topo): abre as formas de relatar um problema ou sugerir algo
  (GitHub ou e-mail).

### 3.2 Criar uma nova ocorrência

1. Clique **Nova ocorrência**.
2. Preencha o diálogo:
   - **Protocolo do ofício (nº do laudo)** — *o campo em destaque no topo*. É o
     coração do caso: o número que o ofício recebeu no protocolo e que
     identifica o laudo.
   - **Tipo de perícia** — escolha na lista ou digite (ex.: *Sinistro de
     Trânsito*, *Perícia Criminal*).
   - **Município** — lista dos municípios do Amapá. **Dica:** se você configurar
     seu *Município de atuação* em Configurações → Perfil, ele já vem
     preenchido.
   - **Número do BO** *(opcional)*.
   - **Peritos** — separados por vírgula.
   - **Pasta** *(opcional)* — onde criar o `.sicro`. Vazio = pasta local padrão.
3. Clique **Criar ocorrência**. O caso é criado e fica ativo.

> ⚠️ **Aviso de pasta sincronizada:** se você escolher uma pasta dentro de
> OneDrive/Google Drive/Dropbox, o SICRO avisa. Prefira pasta **local** e use o
> backup para a nuvem.

### 3.3 Abrir uma ocorrência existente

- **Pelo histórico:** clique no **nome** da ocorrência (é clicável) ou no botão
  **Abrir** da linha.
- **Por pasta:** clique **Abrir workspace** e navegue até a pasta `.sicro`.

### 3.4 Histórico: busca e filtros

- **Busca:** por BO, tipo, natureza, município, bairro ou perito (ignora acento
  e maiúscula/minúscula).
- **De / Até:** filtra por data do fato.
- **Limpar:** zera busca e datas.
- Cada linha traz **Abrir** e um menu **⋯** com *Abrir pasta* e *Excluir
  ocorrência*.

### 3.5 Importar de outro computador (`.sicroapp`)

Casos coletados no **SICRO Operacional** (campo/mobile) chegam como um pacote
`.sicroapp`. As fotos do pacote viram as **Fotos do caso**, usadas pelas Imagens
e como fundo do croqui.

1. **Importar .sicroapp** no painel de ações.
2. Selecione o arquivo (`.sicroapp` ou `.sicrocampo` legado).
3. O SICRO valida o ZIP, confere os hashes das fotos, cria um novo workspace e
   copia tudo. Ao final, mostra um **relatório de importação** (fotos
   importadas, hashes OK/divergentes, avisos).
4. Clique **Abrir ocorrência importada**.

### 3.6 Backup

1. Com um caso aberto, **Gerar backup** no painel de ações.
2. O SICRO compacta todo o workspace num arquivo único **`.sicrobackup`** (ZIP
   com hash), salvo dentro do próprio caso.
3. Esse arquivo é o que você leva para a nuvem ou HD externo — seguro contra
   corrupção por sync.

Para copiar **todos** os casos de uma vez (incremental) e para **restaurar** um
backup, use **Configurações → Backup geral** (item 8).

### 3.7 Verificar integridade / relatório de saúde

- **Verificar integridade** abre a tela de **Integridade** do caso (item 4).
- **Relatório de saúde** (painel de ações) gera um HTML com versão do app,
  dependências (ffmpeg e ffprobe) e estado geral do caso.

> ⚠️ **§13:** excluir uma ocorrência apaga **permanentemente** a pasta `.sicro`
> do disco (croquis, vídeos, áudios, fotos, tudo). É irreversível — só confirme
> com certeza.

---

## 4. Integridade

**Para que serve:** é a camada de **confiança** do caso — a Central de Provas.
Abre pelo **Verificar integridade** do Início. Agrega tudo o que o caso guarda
(fotos, croquis, vídeos, quadros, áudios, imagens tratadas) e confere no disco.

- **Resumo:** contadores por tipo + status geral (íntegro / atenção / crítico).
- **Abas por tipo** (Fotos, Croquis, Vídeos, Frames, Áudios, Imagens; troque com
  Ctrl + PgUp / PgDn): inspeciona
  cada evidência, abre o arquivo, revela na pasta, copia referência.
- **Integridade:** roda a verificação **leve** (existência + caminho) ou
  **profunda** (recalcula SHA-256). Mostra item a item: OK, arquivo ausente,
  sidecar ausente, hash divergente, link quebrado ou caminho inseguro.
- **Gerar relatório de integridade:** salva um HTML auditável na pasta do caso.

> ⚠️ **§13:** a Integridade é **somente leitura** — ela enxerga e verifica,
> nunca altera. É a ferramenta para conferir as provas **antes** de usá-las no
> laudo.
>
> Casos do 3.x que tinham laudos ou documentos continuam com esses arquivos no
> disco: eles aparecem em **Todas** e entram na verificação normalmente.

---

## 5. Croquis

Sob a umbrella **Croquis** há **três tipos**, cada um com seu editor:

| Tipo | Para quê |
|---|---|
| **Viário** | Cena de trânsito: vias, rotatórias, veículos, vestígios, medidas. |
| **Corporal** | Lesões no corpo (entrada/saída de PAF, arma branca, etc.) com legenda. |
| **Planta** | Planta baixa de imóvel/cena: paredes, portas, mobília, vestígios. |

Em todos: você cria pela lista de croquis (cada tipo tem seu botão e um selo de
cor), desenha/anota, e exporta um **PNG técnico** (com título, escala, dados da
ocorrência e data) ou um **PNG limpo** (para colar no corpo do laudo, no Word).

### 5.1 Croqui viário

- **Ferramentas:** Selecionar, Pan, **Medida** (mede em metros pela escala),
  **Definir escala** (2 cliques + distância real), Referenciais (R1/R2), **Via**
  (urbana, avenida, rodovia, terra, estacionamento, **rotatória**), **Veículos**
  (vários tipos), **Vestígios** (ponto de colisão, frenagem, arrasto, sangue…),
  **Mobiliário** (semáforo, placas, poste, faixa de pedestres), **Pessoas**,
  **Anotação** (texto, chamada, seta, trajetória).
- **Fundo da cena:** importe uma foto, use uma das **Fotos do caso**, ou **importe de drone**
  (com correção de lente e recorte). Dá para bloquear, ajustar opacidade e
  centralizar o fundo.
- **Importar OSM:** traz o traçado real das vias do OpenStreetMap por
  coordenada + raio.
- **Inspector:** camadas, propriedades do objeto selecionado e a escala.

> 💡 No editor o OSM aparece como **mapa de referência**; o render final do croqui
> é a geometria técnica (vias, eixos, marcações) — o preview do mapa ≠ o desenho
> final.

#### Passo a passo (viário)
1. Croquis → **Croqui viário**, dê um título.
2. (Opcional) Importe o fundo (foto/drone) ou as vias do OSM.
3. **Definir escala** com uma distância conhecida.
4. Desenhe vias, posicione veículos e marque vestígios.
5. Meça o que precisar.
6. **Exportar PNG técnico** (ou **PNG limpo**) e insira a figura no laudo.

### 5.2 Croqui corporal

- Escolha a prancha (corpo completo, anterior, posterior, cabeça).
- Selecione o **tipo de lesão** e clique no corpo — o marcador é numerado
  automaticamente.
- No inspector, preencha região anatômica, lateralidade, instrumento/meio,
  dimensões e observação.
- A **legenda** é gerada sozinha (numerada). Exporte o PNG (corpo + legenda).

### 5.3 Croqui de planta

- Ferramentas: **Parede** (as paredes se conectam nos nós), **Porta** e
  **Janela** (grudam na parede), **Medir**, **Remover**, mais a camada pericial:
  **vestígios** (com rótulo A/B/C ou 1/2/3 + legenda automática), **trajetória
  balística**, **rosa dos ventos**, **mobiliário**, **texto livre**.
- Exporte o PNG (planta + legenda + cabeçalho).

> ⚠️ **§13 (todos os croquis):** o croqui é o **esquema técnico do perito**. O
> SICRO desenha o que você marca — não infere posições, medidas, ângulos nem
> trajetórias. As medidas/escala valem conforme o seu levantamento.

---

## 6. Imagens

**Para que serve:** editor de imagem **pericial e não-destrutivo** — realça,
mede, anota e analisa, **sem nunca alterar o original**.

### 6.1 Criar uma análise

No módulo **Imagens**, clique **Nova análise** e escolha a origem: uma das
**Fotos do caso**, um **quadro coletado de vídeo**, ou um **arquivo do disco**. O
original é copiado e "hasheado"; todo o trabalho fica numa pilha por cima.

### 6.2 O editor

- **Canvas** com zoom até nível de pixel e réguas ao vivo (em px ou em unidade
  real, se você calibrar a escala).
- **Ferramentas** (à esquerda): seleção (retângulo, elipse, laço, poligonal,
  magnética), anotações, medições, tarja (anonimização), recorte.
- **Painel direito** em modos: **Realçar**, **Filtros**, **Analisar**, **Anotar**
  + **Camadas**.

### 6.3 Realçar e filtros

- **Realçar:** brilho, contraste, gama, saturação, matiz, canais R/G/B, tons de
  cinza, inverter — tudo **só na visualização** (não grava no original).
- **Filtros (galeria buscável):** bordas (Sobel, Laplaciano, Canny), suavização
  (Gaussian, Mediana, Bilateral, Unsharp), realce (CLAHE, equalização,
  auto-níveis, balanço de branco, limiar), morfologia (dilatar, erodir, abrir,
  fechar), geometria, tonal (níveis, curvas), canais, e **forenses** (ELA,
  decorrelation stretch, gradiente de luminância). Cada filtro traz uma **nota**
  explicando para que serve.
- **Pilha de processamento:** os filtros entram numa lista que você liga/desliga,
  reordena e remove, com **preview ao vivo**.

### 6.4 Analisar

- **Histograma** + estatísticas por canal.
- **EXIF** (metadados da câmera, data, GPS).
- **Hashes** (MD5, SHA-1, SHA-256, SHA-3) e metadados de custódia.

### 6.5 Anotar e medir

- Anotações: seta, linha, retângulo, elipse, texto, marcador numerado, polígono,
  ângulo, mão livre, tarja.
- **Definir escala** (2 pontos + distância real) habilita medições em unidade
  real: distância, **área e perímetro** (polígono), **ângulo**.

### 6.6 Relatório e exportação

- **Relatório** gera um HTML/PDF com origem, hash, metadados, anotações e a
  pilha de filtros.
- **Exportar** gera a imagem tratada (com proveniência e hash) para inserir no
  laudo.

> ⚠️ **§13:** o original **nunca** é alterado — tudo é pilha reversível. Mapas e
> realces (ELA, etc.) são **indícios** que exigem exame humano; nada conclui
> sozinho.

---

## 7. Vídeo e Áudio

**Para que serve:** tudo o que é mídia do caso num módulo só. Na aba **Vídeos**:
registrar vídeos com integridade, analisar quadro a quadro, marcar eventos,
coletar quadros, comparar câmeras, exportar trechos e **medir velocidade e
distância**. Na aba **Áudios**: importar ou extrair dos vídeos, **realçar para
escuta**, analisar e **degravar**.

### 7.1 As duas abas

No trilho lateral há uma entrada só, **Vídeo e Áudio**. As abas **Vídeos |
Áudios** ficam na barra do topo, e o módulo volta sempre para a última aba
usada. As duas abas são **irmãs** — o mesmo material visto por dois lados:

- **Vídeo aberto → aba Áudios:** abre o áudio **desse vídeo**, no mesmo instante
  em que o vídeo estava. Se o áudio ainda não foi extraído, o SICRO extrai na
  hora (WAV com hash, ligado ao vídeo de origem). Se o vídeo não tem trilha de
  áudio, a aba avisa.
- **Áudio de um vídeo → aba Vídeos** (ou o botão **"do vídeo …"** no cabeçalho do
  áudio): reabre o vídeo de origem no instante em que o áudio estava.
- O vídeo aberto **continua aberto** ao ir e voltar entre as abas.

O instante leva em conta onde a trilha de áudio começa dentro do arquivo de
vídeo, então vídeo e áudio apontam para o mesmo momento.


### 7.2 Importar e player

- **Adicionar vídeo** importa o arquivo (mp4, mov, mkv, avi, webm, m4v), extrai
  metadados técnicos (codec, resolução, fps, duração) e calcula o **SHA-256**.
  Na lista, clique no cartão (ou em **Abrir**) para analisar.
- **Vídeos do caso em abas:** no topo da análise fica uma aba por vídeo (miniatura,
  nome e duração; a do vídeo na tela fica destacada). Um clique troca de vídeo e
  cada um volta ao instante em que estava; **Ctrl + PgUp / PgDn** alternam pelo
  teclado. **+ Adicionar** (ou **Ctrl + O**) registra um ou vários vídeos sem
  sair da análise e já abre o novo. Botão direito numa aba → **Comparar lado a
  lado com o atual**. A aba **Áudios** sempre leva ao áudio do vídeo que está na
  tela (7.1).
- O player tem timeline, controle de velocidade e atalhos de navegação por frame:
  - **J / K / L** como nos editores: L toca à frente, J em ré, K pausa;
    repetir J ou L acelera **1× → 2× → 4× → 8×**. **↑ / ↓** escolhem de
    **0,1× a 8×**.
  - **Arraste** na linha do tempo (a imagem acompanha). **Ctrl + roda do mouse**
    ou **= / − / 0** aproximam/afastam a linha do tempo — no máximo, um risco
    por quadro; aproximada, aparece uma faixa de visão geral para mover a janela.
  - **Altura da linha do tempo:** arraste o **puxador na borda de cima** dela
    (para cima = mais alta, até ~4× a padrão); duplo clique volta ao padrão. Também
    no botão direito da linha do tempo. A altura fica lembrada e vale também na
    comparação de câmeras.
  - A **legenda de atalhos** sobre o vídeo começa escondida: o botão de teclado
    na barra de controles mostra/esconde.
  - **I / O** marcam entrada e saída de um trecho, que passa a **repetir**
    (Ctrl+L liga/desliga, Alt+X limpa, Shift+I / Shift+O vão às pontas).
  - **Shift + ↑ / ↓** pulam para o evento anterior / seguinte.
  - **Ctrl + G** (ou clique em "tempo atual") abre o **ir para**: digite um tempo
    (`12.48`, `00:12.480`, `1:02:03`) ou um quadro (`#312`). Ao lado aparece o
    **quadro ≈** estimado (mesma conta do storyboard: tempo × fps declarado).
  - O SICRO **lembra onde você parou** em cada vídeo. Vídeo recortado cujo 1º
    quadro vem depois do 0:00 abre direto nele (**Home** volta ao 1º quadro).
  - **F**, duplo clique no vídeo ou o botão ⛶ entram em **tela cheia** com os
    controles e atalhos funcionando (Esc sai).
  - **Lupa:** a roda do mouse sobre o vídeo aproxima no ponto do cursor (até 8×);
    aproximado, arraste para mover. **Ctrl + = / − / 0** também. Só de tela.
  - **Ajustes de tela** (botão de controles deslizantes): brilho, contraste e
    gama para imagem escura. **A** liga/desliga para comparar com o original.
    Não alteram o vídeo, os quadros coletados nem as medições.
  - **Som:** botão de volume e **Ctrl+M** (mudo), **Ctrl + ↑ / ↓** (volume). O
    SICRO lembra o volume. Vídeo sem trilha de áudio deixa o controle apagado.
  - **M** marca um evento no tempo atual (categoria "outro" — renomeie depois).
  - **Ctrl + Shift + C** (ou "copiar") copia o tempo no formato de laudo:
    `00:00:12,480 (quadro ≈ 312) — relógio da câmera: 03:36:08 de 02/08/2026`.
  - **Relógio da câmera:** pare num quadro, clique em "relógio da câmera" e
    digite o horário que a câmera imprime (e a data, se houver). A partir daí o
    SICRO mostra o horário da câmera em qualquer instante, põe esse horário no
    nome dos quadros coletados e dos marcadores. O vínculo fica gravado no caso,
    com registro na trilha de operações.
  - **Sequência de quadros** (**Ctrl+2** ou "Sequência…"): coleta N quadros
    seguidos (ou de k em k) a partir do atual — útil para a aba Velocidade.
  - **Comparar câmeras** (botão no topo): dois vídeos da ocorrência lado a lado,
    cada um com a sua linha do tempo, controles, lupa, ajustes de tela, "Coletar
    frame" e a faixa do seu storyboard. O lado **ativo** (contorno dourado; clique
    nele ou **Tab**) recebe o teclado. Ache o mesmo acontecimento nas duas e
    clique em **Vincular**: o SICRO guarda a diferença e passa a mover as duas
    juntas (ajuste fino ±1 quadro / ±1 s). Vinculadas, **Ctrl+1** coleta o **par**
    (um quadro no storyboard de cada vídeo). **pelo relógio** vincula pelo relógio
    da câmera quando os dois têm.
  - **Exportar trecho** (tesoura na barra, com o trecho I/O marcado): grava uma
    CÓPIA do trecho como vídeo novo do caso, com hash próprio, JSON ao lado e
    registro na trilha — o original não muda. **Sem recompressão** (padrão):
    quadros idênticos ao original, começando no quadro-chave anterior à entrada
    e podendo levar alguns quadros depois da saída (o SICRO informa quanto).
    **Recomprimir**: exatamente os quadros marcados, mas a imagem é recodificada.
    O trecho aparece na lista como "trecho de …" e abre com o link para a origem.
  - **Botão direito:** menus do SICRO no vídeo (tocar, tela cheia, coletar,
    sequência, evento, copiar tempo, trecho, lupa, ajustes, relógio), na linha do
    tempo (ir para / marcar entrada ou saída aqui, zoom) e nos quadros do
    storyboard (ver grande, ir para, copiar tempo, remover). O menu de navegador
    não aparece mais (só nos campos de texto, para copiar e colar).
  - **Storyboard:** miniaturas P / M / G, divisória arrastável para alargar o
    painel da direita (duplo clique volta ao padrão) e **tela grande** (botão ⤢ ou
    duplo clique num quadro): ← / → navegam, Enter leva o player ao quadro, Esc
    fecha.
- A **lista de vídeos** mostra uma miniatura de cada câmera (gerada uma vez e
  guardada no cache do SICRO — o caso não muda).
- **Coletar frame** salva um PNG do quadro que está na tela naquele instante —
  exatamente o mesmo que o player mostra — com o tempo real desse quadro (vira
  "storyboard" e pode ilustrar o laudo).

### 7.3 Eventos

Marque acontecimentos no tempo (colisão, frenagem, impacto, reação, semáforo,
mudança de faixa…), com título e — se quiser — um frame vinculado.

### 7.4 Velocidade e distância (fotogrametria)

1. **Calibre a cena** uma vez: escolha um método (plano de 4 cantos, linha de 2
   pontos, ou razão cruzada), marque os pontos num frame e informe a medida real.
2. **Velocidade:** marque a posição do veículo em vários frames → o SICRO calcula
   km/h. Se você informar as incertezas (σ), ele dá um **intervalo de confiança
   de 95%**.
3. **Distância:** marque 2 pontos num frame calibrado → distância em metros (com
   IC se informar σ).

> ⚠️ **§13:** velocidade e distância são **medições com incerteza**, exibidas de
> forma descritiva — **não são conclusão pericial**. Não há rastreamento nem
> detecção automática: você marca cada ponto; o perito interpreta.

---

### 7.5 Áudios

A aba **Áudios** cuida dos áudios do caso — os importados (WhatsApp, gravador…)
e os extraídos dos vídeos.


### 7.6 Áudios: importar

- **Importar áudio** (WhatsApp, gravador…) ou **Extrair de vídeo**. O original é
  preservado e gera-se um WAV de análise (PCM 16-bit), ambos com hash.
- **Do vídeo do caso:** escolha um vídeo já registrado no caso e clique
  **Extrair** (na tela inicial, logo abaixo dos botões; com áudios na lista, no
  topo à direita). Vídeos **só com imagem** (sem trilha de áudio — comum em
  câmera de segurança) aparecem como "(sem áudio)" e não podem ser escolhidos;
  se um arquivo assim vier pelo "Extrair de vídeo…", o SICRO avisa em vez de
  dar erro do ffmpeg.

### 7.7 Player e abas

- Player com **forma de onda**, marcadores e **loop A–B**.
- **Realçar** gera um novo derivado (o original não muda), com a receita exata
  gravada no caso. Os filtros vêm agrupados e são aplicados sempre na mesma
  ordem, não na ordem em que você marca:
  - **Limpeza:** recuperar saturação (picos estourados), tirar cliques e
    **zumbido da rede** (60 Hz no Brasil, ou 50 Hz, com os harmônicos).
  - **Ruído:** reduzir ruído (FFT, para chiado constante); **redutor de ruído de
    fala (IA local)** — rede neural RNNoise embutida no SICRO, roda offline, boa
    para ruído que varia (trânsito, vento); **ruído por amostra** — marque **A** e
    **B** no player num trecho **só de ruído** (sem fala, ≥ 0,5 s) e o SICRO mede
    esse ruído e o tira do áudio inteiro.
  - **Faixa:** cortar graves (< 80 Hz), cortar agudos (> 8 kHz) ou **banda de voz**
    (300–3400 Hz).
  - **Volume:** normalizar.
- **Analisar:** espectrograma + medições e **ENF** (frequência da rede elétrica
  — indício de continuidade). As **medições** trazem pico, RMS, fator de crista,
  offset DC, clipping e, pelo FFmpeg, o **piso de ruído**, os **bits efetivos**
  (quantos bits o sinal usa de fato — 13 de 16 indica áudio que passou por
  menos resolução), o **loudness integrado EBU R128** (LUFS), a faixa de loudness,
  o **true peak** e o **mapa de silêncios** (< −50 dB por ≥ 0,5 s).
- **Trechos:** recortar um trecho (A–B) e montar uma **compilação rotulada** de
  vários trechos.
- **Ficha:** metadados técnicos e hashes.

### 7.8 Degravação assistida

Abra **Degravar**: toque o áudio, **capture trechos** e digite a transcrição
(com locutor e tempo). Há salvamento automático. Existe um **Rascunho por IA**
(transcrição offline, com o motor baixado em Configurações → IA) que sugere o
texto; **cada linha
precisa ser revisada** antes de ir ao laudo.

> ⚠️ **§13:** realce e análises são determinísticos e offline. O rascunho de IA
> pode errar ou "inventar" texto em ruído/silêncio — é sugestão, não verdade.

---

## 8. Configurações

**Para que serve:** preferências do app e do perito que valem em **todas** as
ocorrências. Em geral as mudanças acumulam e você clica **Salvar** (a Aparência
salva na hora).

Abas:

- **Perfil do perito:** nome, matrícula, cargo, formação, **Município de
  atuação** (pré-preenche novas ocorrências) e foto.
- **Aparência:** tema (escuro / claro / automático), cor de destaque e
  **tamanho da interface** (90% a 175%). Em qualquer tela, **Ctrl + Shift + =**
  aumenta, **Ctrl + Shift + −** diminui e **Ctrl + Shift + 0** volta a 100% —
  sem mexer no zoom do croqui ou da imagem (Ctrl + = / − / 0).
- **Backup geral:** copia **todos** os casos para um destino (HD externo,
  pendrive, rede), um `.sicrobackup` por caso, só recopiando o que mudou
  (*Escolher destino e fazer backup*, *Repetir neste destino*). **Restaurar
  backup…** recria os casos na pasta local padrão — sem sobrescrever os que já
  existem — e traz de volta o perfil.
- **IA (degravação):** baixa o motor de transcrição local (whisper.cpp) e os
  modelos, só quando você clica. Usado pelo Rascunho por IA da degravação.
- **Atalhos de teclado:** customizáveis por ação, organizados por módulo (os
  do grupo **Geral** valem em todas as telas).
- **Diagnóstico:** mostra onde o arquivo de configurações fica no disco.

> 💡 **Primeiro uso:** preencha o **Perfil** (o município já vem nas ocorrências
> novas).

---

## 9. Fluxo ponta a ponta

Um caso típico, do campo à figura no laudo:

1. **Campo (SICRO Operacional):** a coleta gera um `.sicroapp` (opcional).
2. **Início:** *Nova ocorrência* ou *Importar .sicroapp*.
3. **Vídeo e Áudio / Imagens:** trate, meça e analise as evidências (sempre de
   forma não-destrutiva); colete quadros e exporte trechos.
4. **Croquis:** desenhe a cena (viário/corporal/planta) e exporte o PNG.
5. **Integridade:** confira as provas e gere o relatório.
6. **Laudo:** escrito no Word ou LibreOffice, com as figuras exportadas pelo
   SICRO (PNG do croqui, quadros coletados, imagens tratadas) e os tempos
   copiados no formato de laudo.
7. **Backup:** gere o `.sicrobackup` para guardar/transportar.

---

## 10. Limites honestos (§13)

O que o SICRO **faz** e o que **não faz** — para você confiar na ferramenta sem
ilusões:

- **Apoio, não substituição.** Mede, organiza, documenta. **Não conclui** e
  **não interpreta** no seu lugar.
- **Nunca inventa.** Se um arquivo sumiu, ele diz "ausente" — não simula.
- **Original intacto.** Toda edição é derivada/reversível; o original e seu hash
  permanecem.
- **Indícios são indícios.** ELA, ENF, mapas de realce **exigem exame humano**
  e podem dar falso-positivo.
- **Medições têm incerteza.** Velocidade/distância são medição com IC, não
  veredito.
- **Offline.** Sem nuvem nem servidor. A rede só é usada quando você pede: mapa
  e vias do OSM (Croqui) e o download da IA de degravação (Configurações).
- **Reprodutível e auditável.** Pilhas, históricos e hashes permitem refazer e
  verificar.
- **A palavra final é do perito.** Sempre.

---

## 11. Glossário e extensões de arquivo

| Termo / extensão | O que é |
|---|---|
| **Ocorrência / workspace** | O caso, guardado numa pasta `.sicro` autocontida. |
| **`.sicro`** | A pasta do caso (banco, manifesto e subpastas). |
| **`.sicroapp`** | Pacote do SICRO Operacional (campo) para importar no Desktop. |
| **`.sicrobackup`** | Backup compactado (ZIP) de um caso inteiro, com hash. |
| **`.sicrocroqui`** | Croqui viário. |
| **`.sicrocorpo`** | Croqui corporal. |
| **`.sicroplanta`** | Croqui de planta baixa. |
| **`.sicroimage`** | Análise de imagem (pilha não-destrutiva). |
| **SHA-256** | Impressão digital do arquivo, usada para garantir integridade. |
| **Pilha de processamento** | Sequência de filtros aplicada por cima do original (reversível). |
| **Cadeia de custódia** | Registro de origem, hash e operações de cada evidência. |
| **§13** | O princípio que rege o SICRO: apoio, honestidade, original intacto, perito decide. |

---

*Manual do SICRO 4.0 (setembro de 2026), conferido com o código-fonte desta
versão. Algumas telas e rótulos evoluem entre versões; se algo divergir do que
você vê no app, vale o app — e avise (botão **Feedback** no Início) para
atualizar este manual.*
