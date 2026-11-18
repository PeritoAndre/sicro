// Relatório do pacote do júri (contrato sicro.relatorio_juri/2). Dados em dados.json, já em texto pronto para
// imprimir; nada aqui depende do relógio ou do sistema.
#let d = json("dados.json")
#assert(d.esquema == "sicro.relatorio_juri/2", message: "dados de outro contrato: " + d.esquema)

// Identidade do SICRO: o marinho do app (--sicro-surface-2), o dourado de destaque (--sicro-accent) só em filetes e o
// logotipo em vetor. Em preto e branco o dourado vira cinza claro e nada se perde.
#let marinho = rgb("#111f35")
#let ouro = rgb("#d7a84f")
#let cinza = luma(95)
#let linha = luma(190)
#let fina = luma(222)
#let fundo = rgb("#f5f6f8")
#let sans = "Inter"

// Logotipo: o "S" de blocos do ícone do app (geometria do ícone, em unidades de 1/1024 do quadrado) e o traço acima.
#let s-logo(u, cor) = polygon(fill: cor, stroke: none,
  (0 * u, 0 * u), (560 * u, 0 * u), (560 * u, 128 * u), (128 * u, 128 * u), (128 * u, 188 * u), (560 * u, 188 * u),
  (560 * u, 504 * u), (0 * u, 504 * u), (0 * u, 376 * u), (432 * u, 376 * u), (432 * u, 316 * u), (0 * u, 316 * u))
#let logo(tam) = {
  let u = tam / 1024
  box(width: tam, height: tam, fill: rgb("#1b1f26"), radius: tam * 0.16, {
    place(top + left, dx: 220 * u, dy: 196 * u, rect(width: 320 * u, height: 14 * u, fill: rgb("#c36a3f")))
    place(top + left, dx: 232 * u, dy: 260 * u, s-logo(u, rgb("#ecece8")))
  })
}
// Marca pequena, sem o quadrado: o "S" no marinho com o traço dourado.
#let marca(alt) = {
  let u = alt / 564
  box(width: 572 * u, height: alt, baseline: 12%, {
    place(top + left, rect(width: 320 * u, height: 24 * u, fill: ouro))
    place(top + left, dx: 12 * u, dy: 60 * u, s-logo(u, marinho))
  })
}

#let mono(t) = text(font: "DejaVu Sans Mono", size: 0.78em, t)
// Hash sempre em uma linha só e sem caractere invisível: copiar e colar tem de funcionar.
#let hash(h) = box(mono(h))
#let codigo(h) = block(above: 0.5em, inset: (left: 6pt), {
  text(fill: cinza)[Código de verificação (SHA-256)]
  linebreak()
  hash(h)
})
#let curto(h) = if h.len() == 64 { h.slice(0, 8) + "…" + h.slice(56) } else { h }

// Nada passa da margem. Palavra sem pausa mais larga que `w` (nome de arquivo, caminho, código) sai em pedaços que
// cabem, de preferência depois de / _ - . , : =; a quebra é do leiaute: nenhum caractere é acrescentado.
#let partir(p, w) = {
  let linhas = ()
  let atual = ""
  let sep = none
  for c in p.clusters() {
    if atual != "" and measure(atual + c).width > w {
      if sep != none and measure(atual.slice(sep) + c).width <= w {
        linhas.push(atual.slice(0, sep))
        atual = atual.slice(sep)
      } else {
        linhas.push(atual)
        atual = ""
      }
      sep = none
    }
    atual += c
    if c in ("/", "\\", "_", "-", ".", ",", ":", "=") { sep = atual.len() }
  }
  linhas.push(atual)
  linhas.join(linebreak())
}
#let palavras(s, w) = s.split(" ").map(p => if p.clusters().len() <= 12 or measure(p).width <= w { p } else { partir(p, w) }).join(" ")
// Texto em bloco (célula, ficha) com a largura de onde está; `cabe-linha` no meio de uma frase.
#let cabe(s) = if type(s) == str { layout(t => palavras(s, t.width)) } else { s }
#let cabe-linha(s) = context palavras(s, 400pt)
// Nome de arquivo, caminho, endereço: sem a regra do português que repete o hífen na quebra.
#let ident(s) = text(lang: "en", hyphenate: false, cabe(s))
// Comando e argumentos: quebra só entre argumentos (nunca depois do "-" de "-crf").
#let tecnico(s) = layout(t => text(lang: "en", hyphenate: false, s.split(" ").map(p => if measure(p).width <= t.width { box(p) } else { partir(p, t.width) }).join(" ")))
#let comando(c) = block(above: 0.4em, below: 0.6em, inset: (left: 6pt), mono(tecnico(c)))

#let caso = (d.caso.numero, d.caso.referencia).filter(x => x != "").join(" · ")
#set document(
  title: "Relatório da edição demonstrativa de vídeo" + if caso != "" { " — " + caso } else { "" },
  author: if d.perito.nome != "" { d.perito.nome } else { () },
  keywords: ("SICRO", "vídeo", "edição demonstrativa"),
)
#set text(lang: "pt", region: "BR", font: "Libertinus Serif", size: 10.5pt)
#set par(justify: true, leading: 0.62em, spacing: 0.9em)
#set page(
  paper: "a4",
  margin: (top: 2.8cm, bottom: 2.4cm, x: 2.2cm),
  // a capa tem a marca grande; das outras páginas em diante, a pequena
  header: context if here().page() > 1 {
    set text(font: sans, size: 7.5pt, fill: cinza)
    set par(justify: false)
    grid(
      columns: (auto, auto, 1fr),
      column-gutter: 6pt,
      align: (left + horizon, left + horizon, right + horizon),
      marca(8pt),
      [#text(weight: "bold", fill: marinho, tracking: 0.08em)[SICRO] #h(4pt) Edição demonstrativa de vídeo],
      cabe(caso),
    )
    v(-3pt)
    line(length: 100%, stroke: 0.6pt + ouro)
  },
  footer: context {
    set text(font: sans, size: 7.5pt, fill: cinza)
    set par(justify: false)
    line(length: 100%, stroke: 0.4pt + linha)
    v(-3pt)
    grid(
      columns: (1fr, auto),
      column-gutter: 10pt,
      ident("Vídeo " + d.saida.arquivo + " · código " + curto(d.saida.sha256)),
      [Gerado pelo #d.programa #h(6pt) Página #counter(page).display() de #counter(page).final().first()],
    )
  },
)
#set heading(numbering: "1.1")
// título de seção: o traço dourado acima, como o do logotipo
#show heading.where(level: 1): it => block(above: 1.7em, below: 0.8em, sticky: true, {
  block(below: 0.55em, width: 1.9em, height: 1.6pt, fill: ouro)
  set text(font: sans, size: 11.5pt, weight: "bold", fill: marinho)
  if it.numbering != none { counter(heading).display(it.numbering); h(0.55em) }
  it.body
})
#show heading.where(level: 2): it => block(above: 1.3em, below: 0.6em, sticky: true, {
  set text(font: sans, size: 9.5pt, weight: "bold", fill: marinho)
  if it.numbering != none { counter(heading).display(it.numbering); h(0.5em) }
  it.body
})
#set table(stroke: none, inset: (x: 6pt, y: 5pt))
#show table.cell: set par(justify: false)

#let tabela(colunas, cabecalho, ..celulas) = table(
  columns: colunas,
  align: left + top,
  // filete claro entre as linhas: tabela longa se lê sem perder a linha
  stroke: (_, y) => if y > 1 { (top: 0.3pt + fina) },
  table.hline(stroke: 0.8pt + marinho),
  table.header(..cabecalho.map(c => text(font: sans, weight: "bold", size: 0.8em, fill: marinho, c))),
  table.hline(stroke: 0.4pt + marinho),
  ..celulas.pos().map(cabe),
  table.hline(stroke: 0.8pt + marinho),
)
// Lista vazia vira uma frase em vez de tabela só com cabeçalho.
#let tabela-ou(vazio, linhas, colunas, cabecalho) = if linhas.len() == 0 { par(vazio) } else { tabela(colunas, cabecalho, ..linhas.flatten()) }
// Chave e valor; o que não existe (nada, texto vazio) não vira linha.
#let ficha(pares) = {
  let p = pares.filter(((k, v)) => v != none and v != "")
  if p.len() > 0 {
    table(
      columns: (auto, 1fr),
      align: left + top,
      inset: (x: 6pt, y: 3.5pt),
      ..p.map(((k, v)) => (text(fill: cinza, k), cabe(v))).flatten(),
    )
  }
}
#let aviso(corpo) = block(
  width: 100%, fill: fundo, inset: (left: 12pt, rest: 10pt), radius: 2pt,
  stroke: (left: 2.5pt + ouro), corpo,
)
// Amostra da cor de uma marcação (o nome da cor vem escrito ao lado; o espaço que não quebra a mantém junto dele).
#let amostra(cor) = box(width: 0.62em, height: 0.62em, baseline: 0.04em, radius: 1pt, fill: rgb(cor), stroke: 0.4pt + luma(70))
#let efeito(e, w) = if e.marcas.len() == 0 { palavras(e.texto, w) } else {
  [#e.texto: #e.marcas.map(m => if m.cor != none [#amostra(m.cor)~#palavras(m.texto, w)] else [#palavras(m.texto, w)]).join(", ")]
}
// Nome, cargo e matrícula: só o que foi preenchido.
#let quem = (d.perito.nome, d.perito.cargo, if d.perito.matricula != "" { "matrícula " + d.perito.matricula } else { "" }).filter(x => x != "")

// ---------- Abertura ----------
#grid(
  columns: (auto, 1fr),
  column-gutter: 9pt,
  align: horizon,
  logo(1.15cm),
  {
    set text(font: sans)
    set par(justify: false, leading: 0.4em)
    text(size: 13pt, weight: "bold", fill: marinho, tracking: 0.12em)[SICRO]
    linebreak()
    text(size: 7.5pt, fill: cinza, tracking: 0.04em)[Suíte Pericial · gerado pelo #d.programa]
  },
)
#v(1.1em)
#if d.at("exemplo", default: false) {
  block(below: 0.8em, box(stroke: 0.6pt + rgb("#a33"), inset: (x: 5pt, y: 3pt), radius: 2pt, text(size: 8pt, fill: rgb("#a33"))[DOCUMENTO DE EXEMPLO · DADOS FICTÍCIOS]))
}
#block(below: 0.7em, width: 2.2cm, height: 2pt, fill: ouro)
#block(below: 0.75em, text(font: sans, size: 17pt, weight: "bold", fill: marinho)[Relatório da edição demonstrativa de vídeo])
#if caso != "" { block(below: 0.6em, text(size: 11.5pt, cabe-linha(caso))) }
#ficha((
  ("Órgão", d.caso.orgao),
  ("Requisitante", d.caso.requisitante),
  ("Perito", quem.join(", ")),
  ("Emitido em", d.gerado_em),
) + if d.revisado_por != none { (("Revisado por", d.revisado_por),) } else { () })

#let n-trechos = d.roteiro.len()
#let n-orig = d.originais.len()
#aviso[
  #text(font: sans, size: 0.9em, weight: "bold", fill: marinho)[Em poucas palavras]
  #set par(justify: false)
  - O vídeo #emph(cabe-linha(d.saida.arquivo)) é uma *montagem* feita pela perícia para facilitar a compreensão dos fatos. Ele tem #d.saida.duracao e reúne #n-trechos #if n-trechos == 1 [parte] else [partes], #if n-orig == 0 [sem uso de gravação] else [#if n-trechos == 1 [feita] else [feitas] a partir de #if n-orig == 1 [1 arquivo recebido] else [#n-orig arquivos recebidos]].
  - Os arquivos recebidos pela perícia *não foram alterados*. A montagem é um arquivo novo, feito a partir de cópias.
  - Tudo o que foi acrescentado ou modificado (textos, marcações, ampliação, câmera lenta, ocultações) está listado neste relatório, item por item.
  - Cada arquivo tem um *código de verificação*. Se um único ponto da imagem mudar, o código muda. É assim que qualquer pessoa pode conferir se o arquivo é o mesmo.
]

= O que é este documento
Este relatório acompanha o vídeo apresentado e explica, em linguagem simples, de onde vieram as imagens, em que ordem aparecem e o que a perícia fez com elas. A parte final, chamada *Anexo técnico*, traz os mesmos dados com o detalhe necessário para que outro perito refaça o trabalho e confira o resultado.

A montagem é um material de *demonstração*: serve para mostrar. A prova são as gravações originais, que continuam guardadas e podem ser vistas inteiras, sem cortes.

= O vídeo entregue
#ficha((
  ("Arquivo", ident(d.saida.arquivo)),
  ("Duração", d.saida.duracao),
  ("Tamanho da imagem", d.saida.imagem),
  ("Ritmo", d.saida.ritmo),
  ("Formato", [#d.saida.formato · #d.saida.tamanho]),
))
#codigo(d.saida.sha256)

= As gravações usadas
#if n-orig == 0 [Nenhuma gravação foi usada: o vídeo só tem telas de texto.]
#for o in d.originais [
  #block(breakable: false, width: 100%, above: 1.1em)[
    #grid(columns: (auto, 1fr), column-gutter: 0.4em, text(font: sans, size: 0.9em, weight: "bold", fill: marinho)[#o.especie #o.id —], ident(o.rotulo))
    #ficha((
      ("Arquivo", ident(o.arquivo)),
      ("Duração", if o.duracao != "—" { o.duracao }),
      ("Imagem", o.imagem),
      ("Ritmo", o.ritmo),
      ("Formato", (o.formato, o.tamanho).filter(x => x != none).join(" · ")),
      ("Como foi recebida", o.recebido),
      ("Relógio do gravador", o.relogio),
    ) + if o.cobertura != none { (("Parte usada", [usados #o.cobertura.usado de #o.cobertura.total #if o.cobertura.intervalos.len() > 0 [(#o.cobertura.intervalos.join("; "))]]),) } else { () })
    #codigo(o.sha256)
    #if o.derivacao != none [
      #par(text(size: 0.95em)[#o.derivacao.texto])
      #block(above: 0.7em, inset: (left: 6pt), [#text(fill: cinza)[Código da gravação de origem] \ #hash(o.derivacao.origem_sha256)])
    ]
    #if o.taxa_variavel [
      #aviso[#set par(justify: false); *Atenção ao ritmo desta gravação.* A câmera não registrou as imagens em intervalos iguais. Na montagem, cada imagem aparece no instante em que foi registrada, mas movimentos podem parecer mais rápidos ou mais lentos do que foram.]
    ]
    #if o.formato_nativo == false [
      #par(text(size: 0.95em, fill: cinza)[Esta gravação foi recebida já convertida, e não no formato próprio do aparelho gravador.])
    ]
  ]
]

= O que aparece no vídeo, na ordem
#for it in d.roteiro {
  block(breakable: false, width: 100%, above: 1em, below: 1em, grid(
    columns: (4.2cm, 1fr),
    column-gutter: 12pt,
    if it.miniatura != none {
      box(stroke: 0.4pt + linha, image(it.miniatura, width: 100%, alt: "Imagem do item " + str(it.n) + " da montagem"))
    } else {
      box(width: 100%, height: 2.36cm, fill: fundo, stroke: 0.4pt + linha, align(center + horizon, text(size: 8.5pt, fill: cinza)[sem imagem]))
    },
    layout(coluna => {
      set par(justify: false)
      [*#it.n. #it.tipo* #h(0.4em) #text(fill: cinza)[no vídeo: #it.na_saida]]
      if it.descricao != "" {
        parbreak()
        palavras(it.descricao, coluna.width)
      }
      // o texto da tela, como aparece no vídeo
      if it.textos.len() > 0 {
        block(above: 0.55em, below: 0.55em, inset: (left: 7pt, y: 1pt), stroke: (left: 1.2pt + ouro), {
          set par(spacing: 0.45em)
          for t in it.textos {
            par[#text(font: sans, size: 7pt, fill: cinza, tracking: 0.04em, upper(t.papel)) \ #palavras(t.texto, coluna.width - 8pt)]
          }
        })
      }
      if it.origem != none {
        parbreak()
        text(size: 0.95em)[Origem: #it.origem.#if it.velocidade != none [ Velocidade: #it.velocidade.]]
      }
      if it.efeitos.len() > 0 {
        parbreak()
        text(size: 0.95em)[Acrescentado ou modificado: #it.efeitos.map(e => efeito(e, coluna.width)).join("; ").]
      }
      if it.selos.len() > 0 {
        parbreak()
        text(size: 0.95em)[Aviso escrito na imagem: #it.selos.join("; ").]
      }
    }),
  ))
}

= O que foi acrescentado e o que foi modificado
== Acrescentado pela perícia (não faz parte das gravações)
#tabela-ou([Nada foi acrescentado às imagens ou ao som.], d.acrescimos.map(a => (a.o_que, a.onde, a.explicacao)), (25%, 20%, 1fr), ([O quê], [Onde], [Explicação]))
== Modificado na forma de mostrar
#tabela-ou([As gravações são mostradas como foram registradas, sem mudança de velocidade, tamanho ou aparência.], d.mudancas.map(a => (a.o_que, a.onde, a.explicacao)), (25%, 20%, 1fr), ([O quê], [Onde], [Explicação]))
== Etapas feitas por inteligência artificial
#for l in d.ia_resumo [- #l]

= Tempo, velocidade e câmeras alinhadas
#for l in d.relogios [#par(l)]
Trechos em câmera lenta, acelerados ou parados estão indicados no roteiro acima.
#if d.medidas.len() > 0 [
  #par[*Medidas mostradas na imagem.*]
  #for l in d.medidas [- #l]
]

#for s in d.sincronizacao [
  #block(breakable: false, width: 100%)[
    *#s.par.* #s.explicacao
    #ficha((
      ("Como foi alinhado", s.metodo),
      ("Diferença entre os relógios", s.deslocamento),
      ("Diferença que cresce com o tempo", s.deriva),
      ("Margem de erro", s.incerteza),
      ("Conferência", s.conferencia),
    ))
  ]
]

= Limites deste vídeo
#if d.limitacoes.len() == 0 [Nenhuma limitação específica foi registrada para este vídeo.]
#for l in d.limitacoes [- #l]
#if d.selos_desligados.len() > 0 [
  #par[Avisos na imagem desligados pelo perito: #d.selos_desligados.join("; ").]
]

= Como conferir
+ *O arquivo é o mesmo?* Calcule o código SHA-256 do arquivo recebido e compare com o código impresso neste relatório. #d.conferir.prova_arquivo
  #par(text(fill: cinza)[No Windows:])
  #comando(d.conferir.windows)
  #par(text(fill: cinza)[No Linux:])
  #comando(d.conferir.linux)
+ *As imagens são as mesmas?* #d.conferir.prova_quadros #if d.conferir.lista != none [A lista do pacote está no arquivo #emph(d.conferir.lista).]
  #comando(d.conferir.quadros)
+ *As gravações são as mesmas?* Calcule o código de cada gravação original, do mesmo modo, e compare com os códigos da seção 3.
+ *A montagem pode ser refeita?* Sim. O projeto da edição (#emph(d.reproducao.projeto)) acompanha o pacote. #d.reproducao.observacao

#if quem.len() > 0 {
  v(1.6cm)
  align(center, block(width: 8cm, {
    line(length: 100%, stroke: 0.5pt)
    v(-0.4em)
    quem.join(linebreak())
  }))
}

// ---------- Anexo técnico ----------
#pagebreak()
#counter(heading).update(0)
#set heading(numbering: "A.1")
#block(below: 0.7em, width: 2.2cm, height: 2pt, fill: ouro)
#text(font: sans, size: 15pt, weight: "bold", fill: marinho)[Anexo técnico]
#par(text(fill: cinza)[Destinado a peritos e assistentes técnicos. Os tempos são tempos de apresentação (pts) do arquivo indicado.])
#set text(size: 9.5pt)

= Identificação dos arquivos
#tabela((auto, 1fr), ([Id], [Arquivo, tamanho e SHA-256]),
  [Saída], [#ident(d.saida.arquivo + " · " + d.saida.tamanho)#hash(d.saida.sha256)],
  ..d.originais.map(o => (o.id, [#ident(o.arquivo + if o.tamanho != none { " · " + o.tamanho } else { "" })#hash(o.sha256)])).flatten(),
  [Projeto], [#ident(d.reproducao.projeto)#hash(d.reproducao.projeto_sha256)],
)
#if d.pacote.len() > 0 [
  #par[*Arquivos do pacote.* Este relatório é a raiz de confiança do pacote: traz o SHA-256 de todos os outros arquivos.]
  #tabela((1fr,), ([Arquivo e SHA-256],), ..d.pacote.map(a => ([#ident(a.arquivo)#hash(a.sha256)],)).flatten())
]

= Propriedades das gravações
#if n-orig == 0 [Nenhuma gravação usada.]
#for o in d.originais [
  #block(breakable: false, width: 100%, above: 0.9em)[
    #grid(columns: (auto, 1fr), column-gutter: 0.4em, text(font: sans, size: 0.9em, weight: "bold", fill: marinho)[#o.id —], ident(o.arquivo))
    #ficha((
      ("Contêiner (leitor do FFmpeg)", ident(o.tecnico.conteiner)),
      ("Codec", o.tecnico.codec),
      ("Pixel e cor", [#o.tecnico.pix_fmt · #o.tecnico.cor]),
      ("Proporção do pixel (PAR)", o.tecnico.par),
      ("Quadros", o.tecnico.quadros),
      ("Taxa declarada", o.tecnico.fps_nominal),
      ("Intervalo medido entre quadros", o.tecnico.intervalo_quadros),
      ("Intervalos nos trechos usados", o.tecnico.intervalos_usados),
      ("Observações", o.tecnico.observacoes),
      ("Como foi feita", o.tecnico.derivacao),
      ("Comando registrado", if o.tecnico.comando != none { mono(tecnico(o.tecnico.comando)) }),
    ))
  ]
]

= Processamentos, na ordem de aplicação
#par(text(fill: cinza)[Medidas em % são frações da altura da imagem do vídeo, salvo quando a linha diz outra; posições (x, y) em % da largura e da altura. A configuração completa de cada processamento está no relatório técnico anexado.])
#tabela-ou([Nenhum processamento além do corte e da junção dos trechos.], d.processamentos.map(p => (p.item, p.ordem, p.nome, p.parametros)), (auto, auto, auto, 1fr), ([Item], [Ordem], [Processamento], [Parâmetros]))

= Diferenças entre origem e saída
#tabela-ou([Sem diferenças registradas.], d.diferencas.map(x => (x.item, x.imagem, x.taxa, x.quadros, x.repetidos, x.descartados, x.obs)), (auto, 4fr, 4fr, auto, auto, auto, 5fr), ([Item], [Imagem], [Taxa (quadros/s)], [Quadros], [Repet.], [Desc.], [Observação]))
#par(text(fill: cinza)[Quadros: lidos da origem → gravados na saída. Repet.: vezes em que um quadro da origem foi exibido de novo. Desc.: quadros da origem que não entraram na saída.])

= Ocultações de imagem e de som
#if d.ocultacoes.len() == 0 [
  Nenhuma ocultação foi aplicada.
] else [
  // planilha de ocultações (SWGDE 18-M §11): uma linha por ocultação, tempos no arquivo original
  #ficha((
    ("Vídeo com as ocultações", ident(d.saida.arquivo)),
    ("Exportado em", d.gerado_em),
    ("Conferência no arquivo final", if d.conferencia_ocultacoes != none { d.conferencia_ocultacoes } else { [—] }),
  ))
  #tabela((auto, auto, 3fr, auto, 4fr, 4fr), ([Nº], [Item], [Gravação original], [De · até (original)], [Conteúdo e motivo], [Forma e filtro]),
    ..d.ocultacoes.map(x => (x.n, x.item, ident(x.gravacao + " · " + x.arquivo), x.intervalo, [#x.conteudo \ #text(fill: cinza, x.motivo)], [#x.forma \ #x.filtro])).flatten(),
  )
  #let regioes = d.ocultacoes.filter(x => x.regiao != none)
  #if regioes.len() > 0 [
    #par[*Região de cada ocultação de imagem.*]
    #tabela((auto, 1fr), ([Nº], [De onde veio a região · como cada quadro foi coberto]),
      ..regioes.map(x => (x.n, [#x.regiao #if x.quadros != none [\ #text(fill: cinza, x.quadros)]])).flatten(),
    )
  ]
  #par(text(fill: cinza)[As ocultações estão gravadas na imagem e no som do vídeo entregue. O que foi coberto por sólido opaco, ou trocado por tom ou por silêncio, não pode ser recuperado a partir dele; mosaico e desfoque dificultam o reconhecimento, sem garantia de impedi-lo. As gravações originais não têm ocultação.])
]

= Sincronização entre câmeras
#tabela-ou([O vídeo não mostra câmeras sincronizadas entre si.], d.sincronizacao.map(s => (s.par, s.tecnico.metodo, s.tecnico.deslocamento, s.tecnico.deriva, s.tecnico.incerteza)), (auto, 1fr, auto, auto, auto), ([Par], [Método], [Deslocamento], [Deriva], [Incerteza]))

= Codificação da saída
// código de 64 hexadecimais: na linha de baixo, inteiro
#let eh-hash(v) = type(v) == str and v.match(regex("^[0-9a-f]{64}$")) != none
#table(
  columns: (auto, 1fr),
  align: left + top,
  inset: (x: 6pt, y: 3.5pt),
  ..d.codificacao.map(c => if eh-hash(c.at(1)) { (table.cell(colspan: 2)[#text(fill: cinza, c.at(0)) \ #hash(c.at(1))],) } else { (text(fill: cinza, c.at(0)), tecnico(c.at(1))) }).flatten(),
)

= Programas e versões
#tabela((auto, auto, 1fr), ([Programa], [Versão], [Papel]),
  ..d.software.flatten(),
)
#if d.modelos_ia.len() > 0 [
  #par[*Modelos de inteligência artificial*, identificados pelo SHA-256 de cada arquivo.]
  #tabela((27%, 1fr), ([Modelo e uso], [Arquivos e SHA-256 · origem · execução]),
    ..d.modelos_ia.map(m => ([#m.modelo \ #text(fill: cinza, m.uso)], [#for a in m.arquivos [#ident(a.nome)#hash(a.sha256)] #ident(m.origem) #m.execucao #if m.desvio != none [\ #text(fill: cinza)[desvio conhecido: #m.desvio]]])).flatten(),
  )
]
#if d.rastreios.len() > 0 [
  #par[*Acompanhamento de alvos.* Cada corrida do programa, com o modelo, onde rodou, o sentido, o trecho percorrido, por que parou e os limiares usados. As amostras são o arquivo com a posição do alvo em cada quadro, identificado pelo SHA-256.]
  #for r in d.rastreios {
    // sem corrida, confirmação nem amostras: uma linha só, que não se separa do rastreio
    let vazio = r.corridas.len() == 0 and r.confirmacao == "—" and r.amostras == "—"
    block(breakable: not vazio, width: 100%, above: 0.75em, {
      set par(justify: false)
      [*#r.id* #h(0.4em) #r.origem #text(fill: cinza)[· #r.uso]]
      if vazio {
        block(above: 0.5em, inset: (left: 6pt), text(fill: cinza)[Corridas — · Confirmação — · Amostras —])
      } else {
        ficha((
          ("Corridas", if r.corridas.len() == 0 { [—] } else { list(tight: true, ..r.corridas) }),
          ("Confirmação", r.confirmacao),
          ("Amostras", if r.amostras.len() == 64 { hash(r.amostras) } else { r.amostras }),
        ))
      }
    })
  }
]

= Arquivos anexados a este PDF
#if d.anexos.len() == 0 [Nenhum arquivo anexado.]
#for a in d.anexos [
  - #mono(a.arquivo): #a.descricao \ #hash(a.sha256)
  #pdf.attach(a.arquivo, relationship: "supplement", mime-type: "application/json", description: a.descricao)
]

= Referências
#if d.referencias.len() > 0 [
  Relatório estruturado com base em:
  #for r in d.referencias [- #r]
]
