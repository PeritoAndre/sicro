# Manual do SICRO 5.0 — Suíte Pericial

> **Para quem é este manual:** peritos criminais e equipe técnica que usam o
> SICRO no dia a dia. Ele ensina, módulo por módulo, **o que cada parte faz
> e como usar** — com passos, dicas e os limites honestos de cada ferramenta.
>
> **O que é o SICRO 5.0:** uma suíte pericial **desktop, 100% offline**, para
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

A coluna à esquerda é a navegação principal. Ela fica visível no Início e nas
listas; com um croqui, vídeo, áudio ou imagem aberto ela se recolhe para sobrar
tela, e volta quando o mouse encosta na borda esquerda (dá para trocar de módulo
dali, sem voltar). Em **Módulos**:

- **Início** — nome do caso e os três módulos; abre ou cria um caso e já entra
  (e, pelo menu ⋯ do caso, a **Integridade**).
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

**Para que serve:** responder uma pergunta só — *o que você vai fazer agora?*
Um nome para o caso, os três módulos e os casos recentes. O SICRO não é um
cadastro: é a bancada.

### 3.1 O que tem na tela

- **Sem caso aberto:** o campo **Nome do caso** e, abaixo, os três módulos
  (**Croqui**, **Vídeo e Áudio**, **Imagem**). Clicar num módulo **cria o caso e
  já entra nele**. **Enter** no nome abre no Croqui.
- **Com caso aberto:** o **nome do caso** no topo (clique nele para editar os
  dados), a data de criação e a pasta; os três módulos mostram quantos croquis,
  vídeos, áudios e imagens o caso tem — clicar entra direto.
- **Menu ⋯** ao lado do nome: *Dados do caso*, *Abrir pasta*, *Integridade*,
  *Gerar backup*, *Concluir* (ou *Reabrir*) *caso*, *Fechar caso* e *Excluir do
  disco*.
- **Casos recentes** (ou **Outros casos**, com um aberto): uma linha por caso,
  com a última abertura. Clicar **abre o caso e volta ao módulo em que ele foi
  trabalhado por último** (se nunca foi, fica no Início com os módulos). A busca
  só aparece quando a lista passa de 8 casos. O **⋯** da linha traz *Abrir
  pasta*, *Tirar da lista* e *Excluir do disco*.
- **Rodapé:** *Abrir pasta .sicro…* e *Importar .sicroapp…*.

### 3.2 Criar um caso

1. Digite um nome (ex.: *Laudo 63404/26, Km 09 Duca Serra*). É opcional — sem
   nome, o caso se chama *Caso de dd/mm/aaaa* até você dar um.
2. Clique no módulo por onde quer começar (ou **Enter** para o Croqui).

O caso é criado na pasta local padrão (`SICRO/Casos`) e o nome vira o nome da
pasta `.sicro`. Protocolo, ofício, BO, tipo de perícia, município e peritos são
**opcionais**: entram depois, em **⋯ → Dados do caso** (ou clicando no nome).

### 3.3 Abrir um caso

- **Pela lista:** clique no nome do caso.
- **Por pasta:** *Abrir pasta .sicro…* no rodapé e navegue até a pasta `.sicro`
  (serve para casos em outra pasta ou vindos de outro computador).

### 3.4 Importar de outro computador (`.sicroapp`)

Casos coletados no **SICRO Operacional** (campo/mobile) chegam como um pacote
`.sicroapp`. As fotos do pacote viram as **Fotos do caso**, usadas pelas Imagens
e como fundo do croqui.

1. *Importar .sicroapp…* no rodapé do Início.
2. Selecione o arquivo (`.sicroapp` ou `.sicrocampo` legado).
3. O SICRO valida o ZIP, confere os hashes das fotos, cria um novo workspace e
   copia tudo. Ao final, mostra um **relatório de importação** (fotos
   importadas, hashes OK/divergentes, avisos).
4. Clique **Abrir ocorrência importada**.

### 3.5 Backup

1. Com um caso aberto, **⋯ → Gerar backup**.
2. O SICRO compacta todo o workspace num arquivo único **`.sicrobackup`** (ZIP
   com hash), salvo dentro do próprio caso.
3. Esse arquivo é o que você leva para a nuvem ou HD externo — seguro contra
   corrupção por sync.

Para copiar **todos** os casos de uma vez (incremental) e para **restaurar** um
backup, use **Configurações → Backup geral** (item 8).

### 3.6 Integridade

**⋯ → Integridade** abre a tela de **Integridade** do caso (item 4).

> ⚠️ **§13:** *Excluir do disco* apaga **permanentemente** a pasta `.sicro`
> (croquis, vídeos, áudios, fotos, tudo). É irreversível — só confirme com
> certeza. *Tirar da lista* só esconde o caso da lista; a pasta continua no
> disco e volta ao ser aberta de novo.

---

## 4. Integridade

**Para que serve:** é a camada de **confiança** do caso — a Central de Provas.
Abre pelo menu **⋯ → Integridade** do Início. Agrega tudo o que o caso guarda
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
cor), desenha/anota, e exporta um **PNG técnico** (com o tipo do croqui, escala, dados da
ocorrência e data; o título do croqui não sai no PNG) ou um **PNG limpo** (para colar no corpo do laudo, no Word).

### 5.1 Croqui viário

- **Trilho e prateleira:** à esquerda, um trilho com ícone e nome. Em cima, as
  ferramentas diretas: Selecionar, Pan, **Cota**, **Escala**, **R1** (vermelho)
  e **R2** (azul). Abaixo, os grupos: **Via** (urbana, avenida, rodovia, terra,
  estacionamento, rotatória), **Veículos**, **Viaturas**, **Vestígios**,
  **Mobiliário**, **Pessoas** e **Anotação**. Clicar num grupo abre a
  **prateleira** com miniaturas reais; **arraste** uma para a cena e solte onde
  quiser, ou **clique** nela e depois na cena. A prateleira fecha ao inserir
  (o alfinete a mantém aberta; Esc fecha). **Imagem** e **Editar** reúnem fundo,
  importações, desfazer/refazer, duplicar e excluir.
- **Linhas, cota e vias** desenham-se **arrastando** na cena (aperte onde começa,
  solte onde termina) ou com dois cliques; soltas da prateleira, nascem com
  10 m e as pontas prontas para ajuste.
- **Exportar:** um botão, duas opções. **PNG técnico** (com carimbo, para
  anexar) ou **PNG limpo** (só o desenho, para o corpo do laudo). Ao terminar,
  a pasta abre com o arquivo exportado.
- **Fundo da cena:** importe uma foto, use uma das **Fotos do caso**, ou **importe de drone**
  (com correção de lente e recorte). Dá para bloquear, ajustar opacidade e
  centralizar o fundo.
- **Importar OSM:** trechos da mesma rua que seguem em linha viram uma via só.
  Traz o traçado real das vias do OpenStreetMap por
  coordenada + raio.
- **Inspector:** uma coisa por vez. Com um objeto selecionado, só as
  propriedades dele; sem seleção, a **folha e grade**, o **estilo das vias**,
  a escala e as camadas.
- **Rótulos soltos:** o nome do veículo (V1), do referencial (R1), do vestígio
  e o valor da cota são arrastáveis para onde ficar legível, com tamanho, cor e
  rotação próprios no Inspector (**Reto** deixa na horizontal, **Na linha** faz o
  valor da cota acompanhar a inclinação, **Voltar ao lugar** desfaz o arrasto).
- **Folha e grade:** a folha (retângulo com a grade) é exatamente o que vai
  para o PNG, independente do zoom. Com a escala definida, tamanho em metros
  (presets de 25 × 18 a 200 × 141 m), grade em metros, **Folha aqui** (centraliza
  a folha no que está na tela) e **Ajustar à cena**. A resolução do PNG (A4 a 200/300 dpi, A3, A2) fica no mesmo lugar.
- **Estilo das vias** (Inspector): o croqui nasce em **planta técnica** —
  asfalto claro, meio-fio preto, calçada hachurada, sinalização conforme o
  CONTRAN (amarela separa sentidos, branca separa faixas do mesmo sentido),
  tracejado em metros. Dá para trocar para **P&B** (impressão) ou **Escuro**
  (o visual antigo) e ajustar cores, espessuras, calçada e cadência do traço.
  Vale para o croqui inteiro e para o PNG exportado.
- **Cada via** tem: largura da pista, **eixo** (amarela dupla, tracejada,
  contínua + tracejada, branca ou sem), **faixas por sentido** (automático ≈
  3,5 m ou fixo), **acostamento** (com linha de bordo branca) e **calçada**
  (padrão do croqui ou própria). A rotatória ganha ilha, faixas do anel e a
  linha de **dê a preferência** nas entradas, sozinha.

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
- A **legenda** é gerada sozinha (numerada).
- **Exportar** oferece dois PNGs, e a pasta abre com o arquivo marcado:
  **técnico**, com cabeçalho, numeração do POP, listas de regiões e legenda; ou
  **limpo**, só a imagem com as lesões marcadas, para o corpo do laudo.

### 5.3 Croqui de planta

O editor foi refeito na 5.0, no mesmo jeito do croqui viário: paleta à
esquerda, inspetor à direita (uma coisa por vez), a roda move a vista e
Ctrl com a roda dá zoom no ponteiro. Tudo é medido em metros.

- **Cômodo:** arraste um retângulo e nascem as quatro paredes, com nome e
  área. A medida que aparece enquanto você arrasta já é a interna. Cômodos
  vizinhos dividem a mesma parede. Duplo clique num espaço fechado por paredes
  cria o cômodo; duplo clique num cômodo renomeia.
- **Parede:** clique ponto a ponto, em cadeia. Digite a medida interna e Enter
  para a parede sair no tamanho certo. Shift trava em 0, 45 e 90 graus; Alt
  desliga o ímã. Duplo clique ou Esc termina. Paredes se juntam nos cantos e
  em T sozinhas. A parede selecionada mostra a medida interna: clique nela
  para digitar outra, e a parede vizinha anda junto.
- **Aberturas:** porta, porta dupla, de correr, janela, basculante e vão.
  Leve até a parede: ela encaixa, recorta a parede e mostra as distâncias até
  os cantos. Abre para o lado em que está o ponteiro; Espaço inverte.
- **Mobília, estrutura e externo:** símbolos de arquitetura. Perto de uma
  parede a peça encosta e gira junto (Alt solta). R gira 90 graus.
- **Vestígio:** marcadores A, B, C (ou 1, 2, 3) em sequência. Cada um é medido
  até as duas paredes mais próximas, ou até dois cantos, e a medida entra na
  legenda com o nome da parede pela rosa dos ventos (norte, sul, leste,
  oeste).
- **Pessoa** (em pé ou caída), **Trajetória**, **Cota** e **Texto**.
- **Fundo:** foto do croqui feito no local ou planta do imóvel para decalcar,
  com escala por dois pontos. Não sai no PNG.
- **Folha:** A4 ou A3, deitada ou em pé, na escala escolhida (1:50 a 1:200),
  ou de tamanho livre. "Folha em volta da planta" escolhe a menor escala que
  cabe e centraliza.
- **Exportar:** **PNG técnico** com cabeçalho (sem título), cotas externas,
  legenda dos vestígios, tabela de áreas e escala gráfica; ou **PNG limpo**,
  só o desenho. A pasta abre com o arquivo marcado.
- Paredes, portas e janelas guardam altura e peitoril, para um 3D no futuro.
- Plantas feitas antes da 5.0 não abrem no editor novo; o arquivo continua no
  caso.

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
- **Ferramentas** (trilha da esquerda, com nome embaixo do ícone): navegar,
  seleção (retângulo, elipse, laço, poligonal, magnética), anotar, medir,
  proteger (tarja) e recortar.
- **Painel direito** de altura inteira, com um trilho de ícones na borda:
  **Filtros** (padrão), **Camadas** (camadas e objetos), **Análise** e
  **Histórico**. Um painel por vez.
- No topo: **Comparar** (original × filtrado, com divisória arrastável; segurar
  a tecla **\\** mostra o original inteiro) e **Metadados**.

### 6.3 Bancada de filtros

- **Pilha de filtros:** o painel direito é a pilha, aplicada de cima para baixo.
  O primeiro cartão é **Ajustes** (brilho, contraste, gama, saturação, matiz,
  canais R/G/B, tons de cinza, inverter); cada filtro é um cartão com os
  controles ali mesmo, para ligar/desligar, mudar de ordem, remover e aplicar
  **na imagem inteira ou só na seleção**. O original nunca muda.
- **+ Filtro** abre a galeria por cima da imagem, com **miniaturas da sua
  imagem já filtrada** (miniaturas grandes, com a descrição inteira) por cada um dos filtros (bordas, suavização, realce,
  morfologia, geometria, tonal, canais e **forenses**: ELA, decorrelation
  stretch, gradiente de luminância), com busca e uma nota de para que serve.
- **Receitas:** "Placa no escuro", "Bordas e marcas" e "Adulteração (ELA)"
  montam pilhas prontas; **Salvar como receita** guarda a pilha atual com um
  nome, disponível em qualquer caso.

### 6.4 Análise e metadados

- **Histograma** + estatísticas por canal, resumo dos metadados e custódia.
- **Metadados** (botão no topo): todos os campos que o **exiftool** lê (numa
  foto de drone, mais de 150), em português, por grupo, com filtro e cópia em
  texto; cartões de resumo de captura, câmera, local, voo (DJI) e integridade
  (MD5, SHA-1, SHA-256, SHA3-256). O exiftool vem com o SICRO no Windows e é
  dependência do pacote no Linux.

### 6.5 Anotar e medir

- Anotações: seta, linha, retângulo, elipse, texto, marcador numerado, polígono,
  ângulo, mão livre.
- **Proteger (tarja):** desfoque, pixelização ou tarja preta, em elipse,
  retângulo ou **forma livre** (arraste para desenhar o contorno). Com a tarja
  selecionada, **Tornar moldável** transforma elipse/retângulo em dezenas de
  pontos, e cada ponto branco pode ser puxado para ajustar o contorno. A tarja
  moldável não tem alças de canto: arraste pelo meio para mudar de lugar e pelos
  pontos para mudar a forma. A prévia na tela é aproximada; o arquivo exportado
  aplica o efeito em resolução cheia.
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
- **Espectrograma interativo** logo abaixo do player (o botão "Mostrar/Esconder
  espectrograma" fica lembrado): tempo × frequência, intensidade em cor.
  - A linha branca acompanha o player; um **clique** leva o player ao ponto.
  - **Ctrl + roda** dá zoom no tempo (no ponto do cursor); com zoom, **arrastar**
    ou a roda andam; **duplo clique** volta ao áudio inteiro.
  - **Shift + arrastar** marca o trecho **A–B** no player — para ouvir em loop,
    recortar ou usar como perfil de ruído no realce.
  - Passando o mouse, a barra mostra **tempo · frequência · nível (dB)** do ponto.
  - Ajustes: escala **logarítmica** (boa para voz) ou **linear**; **resolução** da
    FFT (1024 separa melhor os instantes, 8192 as frequências); frequência
    máxima (4 kHz para voz, 8 kHz ou tudo); **contraste** (o nível abaixo do qual
    fica preto).
  - Afastado, cada coluna guarda o pico do seu trecho: um clique curto não some.
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
- **ENF** (em Analisar): a frequência da rede elétrica que fica gravada como
  zumbido. **Rede automático** escolhe 50 ou 60 Hz pelo zumbido; o SICRO usa os
  harmônicos (1º ao 4º) pesados pela força de cada um, em janelas de 8 s a cada
  1 s, e mostra a **curva** no tempo, a **confiança** (em quanto do áudio há
  ENF utilizável), **variações bruscas** e **trechos sem ENF** (clique leva o
  player).
  - **Comparar com a referência da rede:** importe no caso uma gravação da rede
    elétrica do período (mais longa que o áudio) e escolha-a na lista. O SICRO
    acha onde o áudio se encaixa nela pela diferença de frequência (mHz) e
    mostra a correlação, o segundo melhor lugar (se for quase tão bom, o
    encaixe é pouco único) e o **encaixe por partes de ~30 s**: num áudio
    montado, partes caem em pontos **diferentes** da referência.
  - **Copiar como texto** gera o resumo para o laudo.
  - Pouco zumbido (aparelho a bateria, filtro) = ENF pouco confiável; o SICRO
    avisa.
- **Autenticidade** (em Analisar): examina o arquivo **original** (o importado,
  ou o vídeo de onde o áudio foi extraído) e o sinal, e lista **indícios** com o
  instante de cada um (clique leva o player):
  - **Arquivo:** contêiner, codec (com/sem perdas), taxa, metadados (p. ex.
    `encoder=Lavf…` indica conversão por programa), pacotes com **buracos na
    linha de tempo** e **erros de decodificação**.
  - **Banda:** corte **em degrau** (marca de codec com perdas); se o arquivo é
    sem perdas mas tem esse corte, o conteúdo já foi comprimido antes; se a banda
    **muda no meio**, há trechos de origens diferentes.
  - **Silêncio digital** (amostras exatamente zero) no meio de som, **saltos ≥ 10
    dB no ruído de fundo** (por faixa: grave, médio, agudo, muito agudo) e
    **cliques** isolados.
  - **Copiar como texto** gera o resumo para o laudo (vírgula decimal).
  - Nenhum teste conclui que houve ou não edição: são pontos para ouvir e
    conferir (com o ENF).
- **Trechos:** recortar um trecho (A–B) e montar uma **compilação rotulada** de
  vários trechos.
- **Ficha:** metadados técnicos e hashes.

### 7.8 Degravação assistida

Abra **Degravar**: toque o áudio, **capture trechos** e digite a transcrição
(com locutor e tempo). Há salvamento automático. Existe um **Rascunho por IA**
(transcrição offline, com o motor baixado em Configurações → IA) que sugere o
texto; **cada linha
precisa ser revisada** antes de ir ao laudo.

- Cada trecho da IA leva a etiqueta **IA** e a **confiança média** (%). Editar o
  trecho marca-o como revisado. Isso fica gravado no caso: ao reabrir, o que
  ainda é rascunho continua marcado.
- **Ouvir de novo:** embaixo do trecho aparecem as **palavras em que a IA teve
  menos de 50% de confiança** (amarelo; vermelho abaixo de 30%). Um clique toca
  o áudio a partir de 0,7 s antes da palavra (o tempo por palavra do whisper é
  aproximado, ±0,3 s). **Conferido** tira a marcação daquele trecho.
- Re-rodar o rascunho substitui só o que ainda não foi revisado.

**Locutores (quem fala quando).** Com o separador instalado (Configurações →
IA), clique **Locutores**. Em **pessoas**, informe quantas falam se souber — é
bem mais confiável que o **automático**, que é uma estimativa e pode juntar ou
separar vozes.

- Aparece uma **faixa colorida** sob o player com os turnos de fala de cada
  voz; um clique num turno leva o player até ele.
- Na **legenda**, dê nome a cada voz ("Entrevistador", "Vítima"…): o nome vale
  para todos os trechos que a separação preencheu.
- O campo **Locutor** dos trechos é preenchido pela voz que mais fala neles, só
  onde estava vazio ou foi a própria separação que preencheu — o que você
  escreveu fica. **2+ vozes** avisa que o trecho tem fala de mais de uma
  pessoa (talvez seja o caso de dividi-lo).
- Separa vozes **diferentes**; **não identifica** quem é a pessoa.

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
  modelos, só quando você clica. Usado pelo Rascunho por IA da degravação. Ali
  também fica o **Separador de locutores** (sherpa-onnx, ≈ 58 MB, um clique):
  programa e modelos oficiais com hash conferido — se o arquivo não conferir,
  nada é instalado. Depois de baixado, roda offline.
- **Atalhos de teclado:** customizáveis por ação, organizados por módulo (os
  do grupo **Geral** valem em todas as telas).
- **Diagnóstico:** mostra onde o arquivo de configurações fica no disco.

> 💡 **Primeiro uso:** preencha o **Perfil** (o município já vem nas ocorrências
> novas).

---

## 9. Fluxo ponta a ponta

Um caso típico, do campo à figura no laudo:

1. **Campo (SICRO Operacional):** a coleta gera um `.sicroapp` (opcional).
2. **Início:** dê um nome ao caso e clique no módulo (ou *Importar
   .sicroapp…*).
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

*Manual do SICRO 5.0 (outubro de 2026), conferido com o código-fonte desta
versão. Algumas telas e rótulos evoluem entre versões; se algo divergir do que
você vê no app, vale o app — e avise (botão **Feedback**, no rodapé do índice
desta Ajuda) para atualizar este manual.*
