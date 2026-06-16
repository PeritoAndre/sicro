## SICRO 2.0 — versão 2.0.2

Atualização de correções e qualidade de vida, concentrada no **editor de laudos**.
100% offline e local, como sempre.

### Novidades

**Editor de laudo — tabelas reformuladas**
- Criar tabela em qualquer região, **inclusive no cabeçalho/rodapé**.
- **Redimensionar** colunas e linhas arrastando as bordas.
- **Barra flutuante** sobre a tabela + **menu do botão direito** (mesclar/dividir
  células, inserir/remover linha e coluna).
- **Mover** a tabela reordenando no fluxo do texto.
- **Legenda editável e removível** (estilo Word): clique no texto pra editar;
  remova pelo ✕, pelo Backspace na legenda vazia ou pelo botão direito —
  e recrie depois por "Adicionar legenda". Sem legenda, a tabela não consome
  número e nada vaza pro PDF/DOCX.
- **Sem negrito forçado**: tabela nova nasce com células comuns (a linha de
  cabeçalho é opcional, e o peso da fonte é sempre do perito).
- **Palavra maior que a coluna quebra dentro da célula** (editor, cabeçalho e
  exportação).
- **Cor de fundo por célula** e fundo uniforme entre as células.

**Editor de laudo — fonte e cor**
- **Arial é a fonte padrão** do laudo (editor, cabeçalho, PDF e DOCX) — e a
  barra mostra sempre a fonte real do ponto clicado, nunca "fonte padrão".
- **Detecção automática**: ao clicar num trecho, a barra mostra a fonte e o
  tamanho daquele ponto.
- Campo de **tamanho** aceita qualquer número digitado, com lista até 72.
- **Seletor de cor** padrão, ao lado do tamanho.

**Editor de laudo — desfazer/refazer**
- **Ctrl+Z / Ctrl+Y** agora cobrem objetos no **cabeçalho e rodapé** (tabelas,
  caixas de texto, formas e fotos), um passo por ação.

**Editor de laudo — outros ajustes**
- **Margem superior estável em documentos longos**: a paginação não "comia"
  mais a margem página após página.
- **Texto apagado não volta**: apagar conteúdo e clicar no cabeçalho não
  ressuscita mais o que foi apagado.
- **Digitação rápida em tabela no cabeçalho** não joga mais o cursor pra fora
  da tabela.
- Caixa de texto **gira corretamente** (as alças acompanham o texto).
- **Formas geométricas** funcionam dentro do cabeçalho e são exportadas.
- A **régua de margem** do texto não invade mais o cabeçalho, e **reduzir o
  cabeçalho** não arrasta mais a margem.
- Abrir o cabeçalho com **clique duplo** não seleciona mais todo o conteúdo.
- **Colar foto** (Ctrl+V) não duplica mais a imagem.

**Ocorrências**
- **Concluir / reabrir / arquivar** ocorrência, com data de encerramento.
- Novo campo **Número do Ofício** (distinto do Número do Protocolo).
- Nome sugerido do laudo no formato
  `TIPO - Laudo Nº {protocolo} - BO {nº BO} - Ofício nº {nº ofício}`.

**Tela inicial**
- Painel **"Ações do workspace"** reúne todas as ações da ocorrência ativa
  (antes escondidas num menu).

### Instalação (Windows 10/11 · x64)
1. Baixe o `SICRO 2.0_2.0.2_x64-setup.exe` em **Assets**, abaixo.
2. Execute. O instalador pergunta se deseja instalar **só para você** ou **para
   todos os usuários** (esta última opção pede privilégio de administrador).
3. Leia e aceite o **Termo de Uso** e conclua.

> Requer o runtime WebView2 — já presente na maioria das instalações do Windows.

### Princípio
Ferramenta de **apoio**: determinística, offline, **nunca altera a evidência
original** e **não produz conclusões periciais** — o perito tem sempre a palavra final.
