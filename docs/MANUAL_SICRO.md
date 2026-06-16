# Manual do SICRO 2.0 — Suíte Pericial

> **Para quem é este manual:** peritos criminais e equipe técnica que usam o
> SICRO 2.0 no dia a dia. Ele ensina, módulo por módulo, **o que cada parte faz
> e como usar** — com passos, dicas e os limites honestos de cada ferramenta.
>
> **O que é o SICRO 2.0:** uma suíte pericial **desktop, 100% offline**, que
> reúne num só lugar a gestão da ocorrência, a elaboração de laudos, croquis,
> análise de imagem/vídeo/áudio, documentoscopia e estatísticas.
>
> **Princípio que rege tudo (§13):** o SICRO é uma **ferramenta de apoio**. Ele
> organiza, mede, calcula e documenta — mas **nunca conclui no seu lugar e nunca
> inventa prova**. O original nunca é alterado, tudo é reproduzível, e a palavra
> final é sempre do perito.

---

## Sumário

1. [Conceitos fundamentais](#1-conceitos-fundamentais)
2. [A janela e a navegação](#2-a-janela-e-a-navegação)
3. [Início (Home)](#3-início-home)
4. [Dossiê](#4-dossiê)
5. [Laudos](#5-laudos)
6. [Croquis](#6-croquis)
7. [Imagens](#7-imagens)
8. [Vídeos](#8-vídeos)
9. [Áudios](#9-áudios)
10. [Documentoscopia](#10-documentoscopia)
11. [Estatísticas](#11-estatísticas)
12. [Configurações](#12-configurações)
13. [Assinatura digital](#13-assinatura-digital)
14. [Fluxo ponta a ponta](#14-fluxo-ponta-a-ponta)
15. [Limites honestos (§13)](#15-limites-honestos-13)
16. [Glossário e extensões de arquivo](#16-glossário-e-extensões-de-arquivo)

---

## 1. Conceitos fundamentais

Antes de entrar nos módulos, três ideias explicam **como o SICRO pensa**.

### 1.1 Ocorrência = workspace `.sicro`

Cada caso é uma **pasta `.sicro`** autocontida no seu computador. Dentro dela
fica tudo do caso: o banco de dados (SQLite), o manifesto, e subpastas para
laudos, croquis, vídeos, imagens, exportações, backups e relatórios.

- **Autocontido:** mover a pasta `.sicro` move o caso inteiro.
- **A verdade está no disco:** o app é só a janela que lê e edita esses arquivos.
- **Um caso por máquina:** evite abrir o mesmo `.sicro` em dois PCs ao mesmo
  tempo. Para transportar um caso, use o **backup** (item 3.6), não a cópia da
  pasta viva.

### 1.2 Offline por design

O SICRO funciona **sem internet**. Não há servidor central, login na nuvem nem
sincronização automática. As únicas coisas que tocam a rede são opcionais e
explícitas: importar vias do OpenStreetMap (Croqui) e assinar no portal gov.br
ou SIGDOCS.

> 💡 **Por que isso importa:** dado pericial fica sob seu controle, no seu disco.
> A redundância em nuvem é feita por **backup** (um arquivo único), nunca
> sincronizando o banco vivo — sync pode corromper o SQLite.

### 1.3 Integridade e original intacto

- Cada arquivo de evidência tem **hash SHA-256** registrado. Se alguém alterar
  o arquivo por fora, o SICRO detecta (a integridade "não bate").
- **O original nunca é alterado.** Tratar uma foto, aplicar filtro, recortar um
  áudio, marcar um documento — tudo isso gera **derivados**; o original
  permanece com seu hash, na custódia.
- **Reprodutível:** as operações ficam registradas (pilha de filtros, histórico)
  e podem ser refeitas igual.

---

## 2. A janela e a navegação

### 2.1 Barra de título

No topo, uma **barra escura** com a marca SICRO à esquerda e os botões de
**minimizar / maximizar / fechar** à direita. Você arrasta a janela por ela e
redimensiona pelas bordas, como qualquer janela do Windows.

### 2.2 Trilho lateral (esquerda)

A coluna fixa à esquerda é a navegação principal. Em **Módulos**:

- **Início** — central de ocorrências.
- **Dossiê** — dados e provas do caso.
- **Laudos** — elaboração de laudos.
- **Croquis** — viário, corporal e planta baixa.
- **Vídeos** — análise de vídeo.
- **Áudios** — análise de áudio e degravação.
- **Imagens** — editor de imagem pericial.
- **Documentoscopia** — análise de documentos.
- **Estatísticas** — painéis do caso e gerais.

No rodapé do trilho: o **card do perito** (puxado de Configurações → Perfil), o
indicador **Local · Offline** e a **versão** do app.

### 2.3 Barra de status (rodapé)

Mostra o contexto atual (workspace ativo, modo de trabalho, contadores). Em
módulos com tela (laudo, imagem, croqui) ela também traz controles de **zoom**.

> ⚠️ **Quase tudo exige uma ocorrência ativa.** Sem um caso aberto, os módulos
> ficam em modo "vazio" pedindo que você crie ou abra uma ocorrência.

---

## 3. Início (Home)

**Para que serve:** é a central de onde você cria, abre e administra ocorrências,
e de onde dispara backup e verificação de integridade.

### 3.1 O que tem na tela

- **Cartão do workspace ativo** (se um caso está aberto): rótulo do caso, caminho,
  status, última abertura, e os botões **Continuar ocorrência**, **Abrir
  workspace**, **Propriedades** e um menu **⋯** (abrir pasta no Explorer, gerar
  relatório de saúde).
- **Estado vazio** (sem caso aberto): botões **Nova ocorrência** e **Abrir
  workspace**.
- **Ações rápidas:** Nova ocorrência · Abrir workspace · Importar .sicroapp ·
  Verificar integridade · Gerar backup.
- **Histórico de ocorrências:** tabela de todos os casos, com busca e filtro de
  data.

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
`.sicroapp`.

1. **Ações rápidas → Importar .sicroapp**.
2. Selecione o arquivo (`.sicroapp` ou `.sicrocampo` legado).
3. O SICRO valida o ZIP, confere os hashes das fotos, cria um novo workspace e
   copia tudo. Ao final, mostra um **relatório de importação** (fotos
   importadas, hashes OK/divergentes, avisos).
4. Clique **Abrir ocorrência importada**.

### 3.6 Backup

1. Com um caso aberto, **Ações rápidas → Gerar backup**.
2. O SICRO compacta todo o workspace num arquivo único **`.sicrobackup`** (ZIP
   com hash), salvo dentro do próprio caso.
3. Esse arquivo é o que você leva para a nuvem ou HD externo — seguro contra
   corrupção por sync.

### 3.7 Verificar integridade / relatório de saúde

- **Verificar integridade** leva você ao Dossiê na lente **Central de Provas**
  (ver item 4).
- **Relatório de saúde** (menu ⋯ do cartão) gera um HTML com versão do app,
  dependências e estado geral do caso.

> ⚠️ **§13:** excluir uma ocorrência apaga **permanentemente** a pasta `.sicro`
> do disco (laudos, croquis, fotos, tudo). É irreversível — só confirme com
> certeza.

---

## 4. Dossiê

**Para que serve:** é a visão central do caso. Tem duas "lentes":

- **SICRO Operacional** — os dados que vieram do campo (pacote `.sicroapp`).
- **Central de Provas** — todas as provas produzidas no SICRO + custódia e
  integridade.

No topo fica a **Identificação do caso** (BO, protocolo, tipo, natureza,
resultado, local, coordenadas, status), que você pode **editar** (botão
*Editar* → *Salvar*). Casos de expediente (áudio/vídeo) podem nascer aqui mesmo,
sem coleta de campo.

### 4.1 Lente "SICRO Operacional"

Abas com o que veio do campo: **Resumo**, **Fotos**, **Checklist**,
**Entidades** (veículos/vítimas), **Vestígios**, **Medições**, **Observações**,
**Timeline** e **Importação**. São dados de visualização — clique numa foto para
ampliar; use *Copiar referência* para citar no laudo.

### 4.2 Lente "Central de Provas"

É a camada de **confiança**. Agrega tudo (fotos, croquis, vídeos, frames,
áudios, análises de imagem, documentoscopia, laudos e vínculos) e verifica a
saúde no disco.

- **Resumo:** contadores por tipo + status geral (íntegro / atenção / crítico).
- **Abas por tipo** (Fotos, Croquis, Vídeos, Frames, Áudios, etc.): inspeciona
  cada evidência, abre o arquivo, revela na pasta, copia referência.
- **Integridade:** roda a verificação **leve** (existência + caminho) ou
  **profunda** (recalcula SHA-256). Mostra item a item: OK, arquivo ausente,
  sidecar ausente, hash divergente, link quebrado ou caminho inseguro.
- **Gerar relatório de integridade:** salva um HTML auditável na pasta do caso.

> ⚠️ **§13:** a Central de Provas é **somente leitura** — ela enxerga e verifica,
> nunca altera. É a ferramenta para conferir as provas **antes** de usá-las no
> laudo.

---

## 5. Laudos

O módulo mais completo: um **editor de página A4** profissional (estilo Word)
para redigir, formatar, ilustrar, exportar e assinar laudos.

> ⚠️ **§13:** o editor é **apoio à redação**. O conteúdo técnico, as análises e
> as conclusões são seus. O SICRO não escreve nem conclui por você.

### 5.1 Lista e criação

No módulo **Laudos** você vê os laudos do caso (com status e selo de assinatura).

- **Novo laudo:** escolha um título e um modelo (em branco ou institucional).
- **Importar do Word (.docx):** traz um documento existente (conversão best-effort).

### 5.2 A página e a edição

- A página A4 aparece com **paginação real** (o conteúdo quebra em páginas como
  no papel) e réguas. Você arrasta as **margens** pela régua.
- **Cabeçalho editável:** dê dois cliques na faixa do cabeçalho para editar
  (texto e imagens, como logos). Ele se repete em todas as páginas no
  PDF/DOCX.
- **Zoom:** Ctrl+scroll ou o controle de zoom na barra de status. Reduzindo,
  você vê várias páginas empilhadas.

### 5.3 Barra de ferramentas

Os controles principais:

- **Fonte e tamanho.**
- **Negrito / itálico / sublinhado / tachado**, subscrito/sobrescrito.
- **Cor do texto** e **realce**.
- **Alinhamento** (esquerda, centro, direita, justificado) e **recuo**.
- **Listas** com marcador e numeradas.
- **Espaçamento:** entrelinhas (*Linhas*), espaço antes/depois do parágrafo (pt),
  e o botão **Compactar ¶** — que remove linhas em branco (Enter duplo vindo de
  .docx) e aplica um espaçamento real de 6pt entre parágrafos.
- **Inserções:** tabela, imagem (do disco, colar com Ctrl+V, ou arrastar),
  figura/foto, formas, caixa de texto, fórmula matemática, quesito, assinatura.
- **Localizar/Substituir**, prévia, **Exportar** e **assinar (SIGDOCS)**.

### 5.4 Fotos, croquis e figuras

- Insira fotos pelo botão, **arrastando** o arquivo para a página, ou **colando**
  (Ctrl+V).
- Ao selecionar uma figura, aparecem alças para **redimensionar**, **girar** e
  **mover**, e modos de posição (alinhada ao texto, à frente, atrás).
- Você pode **inserir um croqui** ou uma **análise de imagem** já feita no caso
  (eles entram com a legenda e ficam registrados na cadeia de custódia).

### 5.5 Fórmulas, quesitos e campos automáticos

- **Fórmula matemática:** abre um editor visual; o SICRO renderiza a equação na
  página.
- **Quesitos:** marque um parágrafo como *Quesito* (numera sozinho) e a *Resposta*
  logo abaixo.
- **Campos automáticos** (ex.: `{{numero_laudo}}`, `{{municipio}}`,
  `{{data_hoje}}`): inserem dados da ocorrência que se atualizam sozinhos.
- **Sumário, lista de figuras e de tabelas** dinâmicos: numeram e se atualizam
  conforme o documento muda.

### 5.6 Revisão e versões

- **Comentários** ancorados em trechos do texto (abrir/resolver).
- **Versões/snapshots** do documento, para voltar a um estado anterior.
- **Validação:** um painel aponta o que falta (seções, campos obrigatórios,
  fotos sem legenda, comentários abertos) antes de finalizar.
- **Status:** rascunho → em revisão → final.

### 5.7 Exportar

Pelo menu **Exportar**:

- **PDF** — se o LibreOffice estiver instalado, a diagramação fica mais fiel ao
  Word; senão, o SICRO usa o motor interno.
- **DOCX** — editável no Word.
- **HTML** — para visualização.

Ao terminar, o SICRO abre a pasta da exportação.

### 5.8 Assinar

Ver o item [13. Assinatura digital](#13-assinatura-digital).

---

## 6. Croquis

Sob a umbrella **Croquis** há **três tipos**, cada um com seu editor:

| Tipo | Para quê |
|---|---|
| **Viário** | Cena de trânsito: vias, rotatórias, veículos, vestígios, medidas. |
| **Corporal** | Lesões no corpo (entrada/saída de PAF, arma branca, etc.) com legenda. |
| **Planta** | Planta baixa de imóvel/cena: paredes, portas, mobília, vestígios. |

Em todos: você cria pela lista de croquis (cada tipo tem seu botão e um selo de
cor), desenha/anota, e exporta um **PNG técnico** (com cabeçalho institucional,
título, escala e data) ou um **PNG limpo** (para colar no corpo do laudo). O
botão **Abrir Laudo** garante que o PNG inserido esteja sempre atualizado.

### 6.1 Croqui viário

- **Ferramentas:** Selecionar, Pan, **Medida** (mede em metros pela escala),
  **Definir escala** (2 cliques + distância real), Referenciais (R1/R2), **Via**
  (urbana, avenida, rodovia, terra, estacionamento, **rotatória**), **Veículos**
  (vários tipos), **Vestígios** (ponto de colisão, frenagem, arrasto, sangue…),
  **Mobiliário** (semáforo, placas, poste, faixa de pedestres), **Pessoas**,
  **Anotação** (texto, chamada, seta, trajetória).
- **Fundo da cena:** importe uma foto, pegue do Dossiê, ou **importe de drone**
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
6. **Exportar PNG técnico** e/ou **Abrir Laudo** para inserir.

### 6.2 Croqui corporal

- Escolha a prancha (corpo completo, anterior, posterior, cabeça).
- Selecione o **tipo de lesão** e clique no corpo — o marcador é numerado
  automaticamente.
- No inspector, preencha região anatômica, lateralidade, instrumento/meio,
  dimensões e observação.
- A **legenda** é gerada sozinha (numerada). Exporte o PNG (corpo + legenda).

### 6.3 Croqui de planta

- Ferramentas: **Parede** (as paredes se conectam nos nós), **Porta** e
  **Janela** (grudam na parede), **Medir**, **Remover**, mais a camada pericial:
  **vestígios** (com rótulo A/B/C ou 1/2/3 + legenda automática), **trajetória
  balística**, **rosa dos ventos**, **mobiliário**, **texto livre**.
- Exporte o PNG (planta + legenda + cabeçalho).

> ⚠️ **§13 (todos os croquis):** o croqui é o **esquema técnico do perito**. O
> SICRO desenha o que você marca — não infere posições, medidas, ângulos nem
> trajetórias. As medidas/escala valem conforme o seu levantamento.

---

## 7. Imagens

**Para que serve:** editor de imagem **pericial e não-destrutivo** — realça,
mede, anota e analisa, **sem nunca alterar o original**.

### 7.1 Criar uma análise

No módulo **Imagens**, clique **Nova análise** e escolha a origem: uma foto do
**Dossiê**, um **frame de vídeo**, ou um **arquivo do disco**. O original é
copiado e "hasheado"; todo o trabalho fica numa pilha por cima.

### 7.2 O editor

- **Canvas** com zoom até nível de pixel e réguas ao vivo (em px ou em unidade
  real, se você calibrar a escala).
- **Ferramentas** (à esquerda): seleção (retângulo, elipse, laço, poligonal,
  magnética), anotações, medições, tarja (anonimização), recorte.
- **Painel direito** em modos: **Realçar**, **Filtros**, **Analisar**, **Anotar**
  + **Camadas**.

### 7.3 Realçar e filtros

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

### 7.4 Analisar

- **Histograma** + estatísticas por canal.
- **EXIF** (metadados da câmera, data, GPS).
- **Hashes** (MD5, SHA-1, SHA-256, SHA-3) e metadados de custódia.

### 7.5 Anotar e medir

- Anotações: seta, linha, retângulo, elipse, texto, marcador numerado, polígono,
  ângulo, mão livre, tarja.
- **Definir escala** (2 pontos + distância real) habilita medições em unidade
  real: distância, **área e perímetro** (polígono), **ângulo**.

### 7.6 Relatório e laudo

- **Relatório** gera um HTML/PDF com origem, hash, metadados, anotações e a
  pilha de filtros.
- Você pode **editar uma foto do laudo** aqui e voltar — a foto tratada substitui
  a do laudo, com o original preservado.

> ⚠️ **§13:** o original **nunca** é alterado — tudo é pilha reversível. Mapas e
> realces (ELA, etc.) são **indícios** que exigem exame humano; nada conclui
> sozinho.

---

## 8. Vídeos

**Para que serve:** registrar vídeos com integridade, marcar eventos, coletar
frames e **medir velocidade e distância** por fotogrametria.

### 8.1 Importar e player

- **Adicionar vídeo** importa o arquivo (mp4, mov, mkv, avi, webm, m4v), extrai
  metadados técnicos (codec, resolução, fps, duração) e calcula o **SHA-256**.
- O player tem timeline, controle de velocidade e atalhos de navegação por frame.
- **Coletar frame** salva um PNG no timecode exato (vira "storyboard" e pode
  ilustrar o laudo).

### 8.2 Eventos

Marque acontecimentos no tempo (colisão, frenagem, impacto, reação, semáforo,
mudança de faixa…), com título e — se quiser — um frame vinculado.

### 8.3 Velocidade e distância (fotogrametria)

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

## 9. Áudios

**Para que serve:** importar áudios (ou extrair de vídeo), preservar o original,
**realçar para escuta**, analisar e **degravar** (transcrever).

### 9.1 Importar

- **Importar áudio** (WhatsApp, gravador…) ou **Extrair de vídeo**. O original é
  preservado e gera-se um WAV de análise (PCM 16-bit), ambos com hash.

### 9.2 Player e abas

- Player com **forma de onda**, marcadores e **loop A–B**.
- **Realçar:** reduzir ruído, cortar graves/agudos, normalizar — gera um novo
  derivado (não altera o original).
- **Analisar:** espectrograma + medições (pico, RMS, clipping) e **ENF**
  (frequência da rede elétrica — indício de continuidade).
- **Trechos:** recortar um trecho (A–B) e montar uma **compilação rotulada** de
  vários trechos.
- **Ficha:** metadados técnicos e hashes.

### 9.3 Degravação assistida

Abra **Degravar**: toque o áudio, **capture trechos** e digite a transcrição
(com locutor e tempo). Há salvamento automático. Existe um **Rascunho por IA**
(transcrição offline) — em desenvolvimento — que sugere o texto; **cada linha
precisa ser revisada** antes de ir ao laudo.

> ⚠️ **§13:** realce e análises são determinísticos e offline. O rascunho de IA
> pode errar ou "inventar" texto em ruído/silêncio — é sugestão, não verdade.

---

## 10. Documentoscopia

**Para que serve:** apoio à análise documentoscópica — OCR, extração de campos,
indícios de manipulação e confronto visual.

### 10.1 Importar e a banca de trabalho

- **Importar PDF** ou **Importar imagem** — o SICRO guarda uma **cópia** com
  hash; o original não é tocado.
- A banca (DocWorkbench) tem o visualizador no centro e abas técnicas à direita.

### 10.2 Leitura e extração

- **Texto (OCR):** extrai o texto (de imagem, de PDF com texto, ou rasterizando
  PDF escaneado). Você revisa e corrige os blocos.
- **Campos:** detecta por heurística CPF, CNPJ, placa, chassi, processo, datas,
  valores, e-mail, etc. **Tudo exige revisão** — marque cada campo como revisado.
- **Layout:** marca regiões (assinatura, carimbo, tabela, QR, código de barras)
  e tenta decodificar QR/barras.
- **Realce:** pré-processa a imagem (tons de cinza, endireitar, CLAHE,
  binarizar) para facilitar o OCR — sem alterar o original.

### 10.3 Indícios digitais (forense)

- **ELA (Error Level Analysis):** destaca regiões recomprimidas — indício de
  edição.
- **Mapa de ruído:** saltos de textura podem sugerir composição.
- **Copy-move:** procura regiões clonadas na mesma imagem.

Cada um gera um mapa que você pode **exportar** ou **enviar ao laudo** (entra em
Evidências → Indícios).

### 10.4 Confronto e relatório

- **Confronto:** documento questionado × padrão, lado a lado ou em sobreposição,
  com zoom/pan sincronizados, marcadores correspondentes, medições e calibração.
- **Relatório:** gera um quadro técnico (proveniência, hash, campos revisados,
  regiões) em **linguagem indiciária** para colar no laudo.

> ⚠️ **§13 (crítico):** ELA, ruído e copy-move são **indicativos, não prova** —
> bordas e alto contraste dão falso-positivo; sem histórico de compressão JPEG o
> ELA é pouco informativo. O confronto **alinha e mede** — não calcula índice de
> similaridade nem conclui autoria/autenticidade. A conclusão é **exclusivamente
> do perito**.

---

## 11. Estatísticas

**Para que serve:** painéis que **descrevem** o caso (ou o conjunto de casos) em
números e gráficos — sem interpretar.

- **Por caso** (precisa de um caso aberto): páginas de Visão geral, Evidências,
  Dossiê operacional, Laudos & produção, Vídeo & medições e Linha do tempo —
  com KPIs, gráficos de barra/rosca/linha e histogramas.
- **Geral** (entre casos): usa o índice de casos para mostrar distribuição por
  tipo, produção ao longo do tempo, etc. Use **Reindexar** se faltar caso.
- **Exportar:** HTML, CSV ou JSON.

> ⚠️ **§13:** os painéis **só contam o que existe** — nenhuma classificação,
> sugestão ou inferência. Velocidades/distâncias aparecem como medição
> descritiva, não conclusão.

---

## 12. Configurações

**Para que serve:** preferências do app e do perito que valem em **todas** as
ocorrências. Em geral as mudanças acumulam e você clica **Salvar** (a Aparência
salva na hora).

Abas:

- **Perfil do perito:** nome, matrícula, cargo, formação, **Município de
  atuação** (pré-preenche novas ocorrências) e caminho da imagem de assinatura.
- **Instituição & marca:** órgão, unidade, endereço, texto de rodapé e caminhos
  dos brasões/logo (alimentam o cabeçalho do laudo).
- **Aparência:** tema (escuro / claro / automático) e cor de destaque.
- **Integrações (SIGDOC):** e-mail e senha do SIGDOCS para autopreenchimento. A
  **senha fica no Gerenciador de Credenciais do Windows** (criptografada), nunca
  em texto claro.
- **Caminhos padrão:** pasta padrão de workspaces e de exportação.
- **Backup geral:** cópia de todos os casos.
- **Dependências:** status e instalação de LibreOffice (PDF fiel), IA (Whisper,
  degravação) e OCR (Tesseract).
- **Atalhos de teclado:** customizáveis por ação, organizados por módulo.
- **Diagnóstico:** mostra onde o arquivo de configurações fica no disco.

> 💡 **Primeiro uso:** preencha o **Perfil** e, se for assinar via SIGDOCS,
> guarde as credenciais em **Integrações**.

---

## 13. Assinatura digital

O SICRO **não assina por você** — ele exporta o PDF, leva você ao portal e
**arquiva** o PDF assinado de volta, com hash. Há duas vias prontas:

### 13.1 SIGDOCS (institucional — Estado do Amapá)

1. No laudo **finalizado**, abra **Assinatura digital → SIGDOCS**.
2. **Exportar PDF e abrir SIGDOCS:** o SICRO gera o PDF, copia o caminho,
   abre o Explorer com o arquivo selecionado e abre o SIGDOCS por cima do app.
3. No SIGDOCS: entre, anexe o PDF (arraste do Explorer), assine e baixe o
   assinado.
4. **Importar PDF assinado:** selecione o arquivo baixado (pode informar
   *Pasta SIGDOCS* e *Protocolo*). O SICRO arquiva em `…/assinados/`, calcula o
   hash e marca o laudo como **Assinado SIGDOCS**.

> 💡 No SIGDOCS o **Ctrl+V não funciona** — por isso o SICRO já abre o Explorer
> na pasta certa para você **arrastar** o PDF.

### 13.2 gov.br (federal — ITI)

Igual ao fluxo acima, mas o botão abre `assinador.iti.gov.br` no navegador.
Você faz login gov.br, anexa o PDF, confirma com 2FA, baixa o assinado e
**Importa** no SICRO. O laudo fica **Assinado gov.br**.

### 13.3 Onde fica e como aparece

- O PDF assinado fica em `laudos/<id>/assinados/`.
- Na lista de laudos, um **selo** indica *Assinado gov.br* ou *Assinado SIGDOCS*.

> ⚠️ **§13:** o SICRO **não valida** a assinatura (isso é do portal) e **não
> assina automaticamente**. A assinatura é sempre um ato do perito. Os tipos
> A1/A3 com certificado local ainda **não** estão implementados.

---

## 14. Fluxo ponta a ponta

Um caso típico, do campo ao laudo assinado:

1. **Campo (SICRO Operacional):** coleta gera um `.sicroapp`.
2. **Início:** *Importar .sicroapp* → cria a ocorrência no Desktop.
3. **Dossiê:** confira os dados (Operacional) e verifique as provas (Central de
   Provas).
4. **Imagens / Vídeos / Áudios / Documentoscopia:** trate, meça e analise as
   evidências (sempre de forma não-destrutiva).
5. **Croquis:** desenhe a cena (viário/corporal/planta) e exporte o PNG.
6. **Laudos:** redija, insira fotos/croquis/análises, responda quesitos, valide.
7. **Exportar PDF** e **Assinar** (SIGDOCS ou gov.br).
8. **Backup:** gere o `.sicrobackup` para guardar/transportar.
9. **Estatísticas:** acompanhe a produção do caso e do conjunto.

---

## 15. Limites honestos (§13)

O que o SICRO **faz** e o que **não faz** — para você confiar na ferramenta sem
ilusões:

- **Apoio, não substituição.** Mede, organiza, documenta. **Não conclui** e
  **não interpreta** no seu lugar.
- **Nunca inventa.** Se um arquivo sumiu, ele diz "ausente" — não simula.
- **Original intacto.** Toda edição é derivada/reversível; o original e seu hash
  permanecem.
- **Indícios são indícios.** ELA, ruído, copy-move, ENF, mapas de realce
  **exigem exame humano** e podem dar falso-positivo.
- **Medições têm incerteza.** Velocidade/distância são medição com IC, não
  veredito.
- **Offline.** Sem nuvem nem servidor; rede só no OSM e nos portais de
  assinatura, sempre explícita.
- **Reprodutível e auditável.** Pilhas, históricos e hashes permitem refazer e
  verificar.
- **A palavra final é do perito.** Sempre.

---

## 16. Glossário e extensões de arquivo

| Termo / extensão | O que é |
|---|---|
| **Ocorrência / workspace** | O caso, guardado numa pasta `.sicro` autocontida. |
| **`.sicro`** | A pasta do caso (banco, manifesto e subpastas). |
| **`.sicroapp`** | Pacote do SICRO Operacional (campo) para importar no Desktop. |
| **`.sicrobackup`** | Backup compactado (ZIP) de um caso inteiro, com hash. |
| **`.sicrodoc`** | Documento do laudo (conteúdo estruturado + cabeçalho + metadados). |
| **`.sicrocroqui`** | Croqui viário. |
| **`.sicrocorpo`** | Croqui corporal. |
| **`.sicroplanta`** | Croqui de planta baixa. |
| **`.sicroimage`** | Análise de imagem (pilha não-destrutiva). |
| **SHA-256** | Impressão digital do arquivo, usada para garantir integridade. |
| **Pilha de processamento** | Sequência de filtros aplicada por cima do original (reversível). |
| **Cadeia de custódia** | Registro de origem, hash e operações de cada evidência. |
| **§13** | O princípio que rege o SICRO: apoio, honestidade, original intacto, perito decide. |

---

*Manual gerado a partir da inspeção do código-fonte do SICRO 2.0. Algumas telas e
rótulos evoluem entre versões; se algo divergir do que você vê no app, vale o
app — e me avise para atualizar este manual.*
