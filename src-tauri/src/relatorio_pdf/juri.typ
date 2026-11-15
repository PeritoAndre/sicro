// Relatório do pacote do júri (contrato sicro.relatorio_juri/2). Dados em dados.json, já em texto pronto para
// imprimir; nada aqui depende do relógio ou do sistema.
#let d = json("dados.json")
#assert(d.esquema == "sicro.relatorio_juri/2", message: "dados de outro contrato: " + d.esquema)

#let cinza = luma(95)
#let linha = luma(190)
#let fundo = rgb("#f3f5f8")
#let azul = rgb("#1f3a5f")

#let mono(t) = text(font: "DejaVu Sans Mono", size: 0.78em, t)
// Hash sempre em uma linha só e sem caractere invisível: copiar e colar tem de funcionar.
#let hash(h) = box(mono(h))
#let codigo(h) = block(above: 0.5em, inset: (left: 6pt), {
  text(fill: cinza)[Código de verificação (SHA-256)]
  linebreak()
  hash(h)
})
#let curto(h) = if h.len() == 64 { h.slice(0, 8) + "…" + h.slice(56) } else { h }
// Comando para copiar: uma linha, sem quebra no meio.
#let comando(c) = block(above: 0.4em, below: 0.6em, inset: (left: 6pt), mono(c))

#set document(
  title: "Relatório da edição demonstrativa de vídeo — " + d.caso.numero,
  author: d.perito.nome,
  keywords: ("SICRO", "vídeo", "edição demonstrativa"),
)
#set text(lang: "pt", region: "BR", font: "Libertinus Serif", size: 10.5pt)
#set par(justify: true, leading: 0.62em, spacing: 0.9em)
#set page(
  paper: "a4",
  margin: (top: 2.8cm, bottom: 2.4cm, x: 2.2cm),
  header: {
    set text(size: 8.5pt, fill: cinza)
    grid(
      columns: (1fr, auto),
      [Relatório da edição demonstrativa de vídeo · #d.caso.numero], [SICRO],
    )
    v(-0.5em)
    line(length: 100%, stroke: 0.4pt + linha)
  },
  footer: context {
    set text(size: 8.5pt, fill: cinza)
    line(length: 100%, stroke: 0.4pt + linha)
    v(-0.5em)
    grid(
      columns: (1fr, auto),
      [Vídeo: #d.saida.arquivo · código #mono(curto(d.saida.sha256))],
      [Página #counter(page).display() de #counter(page).final().first()],
    )
  },
)
#set heading(numbering: "1.1")
#show heading.where(level: 1): it => block(above: 1.5em, below: 0.8em, sticky: true, {
  set text(size: 13pt, fill: azul)
  if it.numbering != none { counter(heading).display(it.numbering); h(0.5em) }
  it.body
})
#show heading.where(level: 2): it => block(above: 1.2em, below: 0.6em, sticky: true, {
  set text(size: 11pt)
  if it.numbering != none { counter(heading).display(it.numbering); h(0.5em) }
  it.body
})
#set table(stroke: none, inset: (x: 6pt, y: 5pt))
#show table.cell: set par(justify: false)

#let tabela(colunas, cabecalho, ..celulas) = table(
  columns: colunas,
  align: left + top,
  table.hline(stroke: 0.8pt),
  table.header(..cabecalho.map(c => text(weight: "bold", size: 0.92em, c))),
  table.hline(stroke: 0.4pt),
  ..celulas,
  table.hline(stroke: 0.8pt),
)
// Lista vazia vira uma frase em vez de tabela só com cabeçalho.
#let tabela-ou(vazio, linhas, colunas, cabecalho) = if linhas.len() == 0 { par(vazio) } else { tabela(colunas, cabecalho, ..linhas.flatten()) }
#let ficha(pares) = table(
  columns: (auto, 1fr),
  align: left + top,
  inset: (x: 6pt, y: 3.5pt),
  ..pares.map(((k, v)) => (text(fill: cinza, k), v)).flatten(),
)
#let aviso(corpo) = block(
  width: 100%, fill: fundo, inset: (left: 12pt, rest: 10pt), radius: 2pt,
  stroke: (left: 2.5pt + azul), corpo,
)
// Nome, cargo e matrícula: só o que foi preenchido.
#let quem = (d.perito.nome, d.perito.cargo, if d.perito.matricula != "" { "matrícula " + d.perito.matricula } else { "" }).filter(x => x != "")

// ---------- Abertura ----------
#if d.at("exemplo", default: false) {
  block(below: 0.8em, box(stroke: 0.6pt + rgb("#a33"), inset: (x: 5pt, y: 3pt), radius: 2pt, text(size: 8pt, fill: rgb("#a33"))[DOCUMENTO DE EXEMPLO · DADOS FICTÍCIOS]))
}
#text(size: 19pt, weight: "bold", fill: azul)[Relatório da edição demonstrativa de vídeo]
#v(-0.3em)
#text(size: 11.5pt)[#(d.caso.numero, d.caso.referencia).filter(x => x != "").join(" · ")]
#v(0.2em)
#ficha((
  ("Órgão", d.caso.orgao),
  ("Requisitante", d.caso.requisitante),
  ("Perito", quem.join(", ")),
  ("Emitido em", d.gerado_em),
) + if d.revisado_por != none { (("Revisado por", d.revisado_por),) } else { () })

#let n-trechos = d.roteiro.len()
#let n-orig = d.originais.len()
#aviso[
  *Em poucas palavras*
  #set par(justify: false)
  - O vídeo #emph(d.saida.arquivo) é uma *montagem* feita pela perícia para facilitar a compreensão dos fatos. Ele tem #d.saida.duracao e reúne #n-trechos #if n-trechos == 1 [parte] else [partes], #if n-orig == 0 [sem uso de gravação] else [#if n-trechos == 1 [feita] else [feitas] a partir de #if n-orig == 1 [1 arquivo recebido] else [#n-orig arquivos recebidos]].
  - Os arquivos recebidos pela perícia *não foram alterados*. A montagem é um arquivo novo, feito a partir de cópias.
  - Tudo o que foi acrescentado ou modificado (textos, marcações, ampliação, câmera lenta, ocultações) está listado neste relatório, item por item.
  - Cada arquivo tem um *código de verificação*. Se um único ponto da imagem mudar, o código muda. É assim que qualquer pessoa pode conferir se o arquivo é o mesmo.
]

= O que é este documento
Este relatório acompanha o vídeo apresentado e explica, em linguagem simples, de onde vieram as imagens, em que ordem aparecem e o que a perícia fez com elas. A parte final, chamada *Anexo técnico*, traz os mesmos dados com o detalhe necessário para que outro perito refaça o trabalho e confira o resultado.

A montagem é um material de *demonstração*: serve para mostrar. A prova são as gravações originais, que continuam guardadas e podem ser vistas inteiras, sem cortes.

= O vídeo entregue
#ficha((
  ("Arquivo", d.saida.arquivo),
  ("Duração", d.saida.duracao),
  ("Tamanho da imagem", d.saida.imagem),
  ("Ritmo", d.saida.ritmo),
  ("Formato", [#d.saida.formato · #d.saida.tamanho]),
))
#codigo(d.saida.sha256)

= As gravações usadas
#if n-orig == 0 [Nenhuma gravação foi usada: o vídeo só tem telas de texto.]
#for o in d.originais [
  #block(breakable: false, width: 100%)[
    *#o.especie #o.id — #o.rotulo*
    #ficha((
      ("Arquivo", o.arquivo),
      ("Duração", o.duracao),
      ("Imagem", (o.imagem, o.ritmo).filter(x => x != none).join(" · ")),
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
    {
      set par(justify: false)
      [*#it.n. #it.tipo* #h(0.4em) #text(fill: cinza)[no vídeo: #it.na_saida]]
      if it.descricao != "" {
        parbreak()
        it.descricao
      }
      if it.origem != none {
        parbreak()
        text(size: 0.95em)[Origem: #it.origem.#if it.velocidade != none [ Velocidade: #it.velocidade.]]
      }
      if it.efeitos.len() > 0 {
        parbreak()
        text(size: 0.95em)[Acrescentado ou modificado: #it.efeitos.join("; ").]
      }
      if it.selos.len() > 0 {
        parbreak()
        text(size: 0.95em)[Aviso escrito na imagem: #it.selos.join("; ").]
      }
    },
  ))
}

= O que foi acrescentado e o que foi modificado
== Acrescentado pela perícia (não faz parte das gravações)
#tabela-ou([Nada foi acrescentado às imagens ou ao som.], d.acrescimos.map(a => (a.o_que, a.onde, a.explicacao)), (auto, auto, 1fr), ([O quê], [Onde], [Explicação]))
== Modificado na forma de mostrar
#tabela-ou([As gravações são mostradas como foram registradas, sem mudança de velocidade, tamanho ou aparência.], d.mudancas.map(a => (a.o_que, a.onde, a.explicacao)), (auto, auto, 1fr), ([O quê], [Onde], [Explicação]))
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

#v(1.6cm)
#align(center, block(width: 8cm, {
  line(length: 100%, stroke: 0.5pt)
  v(-0.4em)
  quem.join(linebreak())
}))

// ---------- Anexo técnico ----------
#pagebreak()
#counter(heading).update(0)
#set heading(numbering: "A.1")
#text(size: 16pt, weight: "bold", fill: azul)[Anexo técnico]
#par(text(fill: cinza)[Destinado a peritos e assistentes técnicos. Os tempos são tempos de apresentação (pts) do arquivo indicado.])
#set text(size: 9.5pt)

= Identificação dos arquivos
#tabela((auto, 1fr, auto), ([Id], [Arquivo e SHA-256], [Tamanho]),
  [Saída], [#d.saida.arquivo \ #hash(d.saida.sha256)], d.saida.tamanho,
  ..d.originais.map(o => (o.id, [#o.arquivo \ #hash(o.sha256)], if o.tamanho != none { o.tamanho } else { [—] })).flatten(),
  [Projeto], [#d.reproducao.projeto \ #hash(d.reproducao.projeto_sha256)], [—],
)
#if d.pacote.len() > 0 [
  #par[*Arquivos do pacote.* Este relatório é a raiz de confiança do pacote: traz o SHA-256 de todos os outros arquivos.]
  #tabela((1fr,), ([Arquivo e SHA-256],), ..d.pacote.map(a => ([#a.arquivo \ #hash(a.sha256)],)).flatten())
]

= Propriedades das gravações
#tabela-ou([Nenhuma gravação usada.], d.originais.map(o => (o.id, o.tecnico.conteiner, o.tecnico.codec, [#o.tecnico.pix_fmt \ #o.tecnico.cor], o.tecnico.par, o.tecnico.quadros, [#o.tecnico.fps_nominal · #o.tecnico.intervalo_quadros \ #o.tecnico.observacoes])), (auto, auto, auto, auto, auto, auto, 1fr), ([Id], [Contêiner], [Codec], [Pixel e cor], [PAR], [Quadros], [Taxa declarada · intervalo medido entre quadros · observações]))

= Processamentos, na ordem de aplicação
#tabela-ou([Nenhum processamento além do corte e da junção dos trechos.], d.processamentos.map(p => (p.item, p.ordem, p.nome, p.parametros)), (auto, auto, auto, 1fr), ([Item], [Ordem], [Processamento], [Parâmetros]))

= Diferenças entre origem e saída
#tabela-ou([Sem diferenças registradas.], d.diferencas.map(x => (x.item, x.imagem, x.taxa, x.quadros, x.repetidos, x.descartados, x.obs)), (auto, 4fr, 4fr, auto, auto, auto, 5fr), ([Item], [Imagem], [Taxa (quadros/s)], [Quadros], [Repet.], [Desc.], [Observação]))
#par(text(fill: cinza)[Quadros: lidos da origem → gravados na saída. Repet.: vezes em que um quadro da origem foi exibido de novo. Desc.: quadros da origem que não entraram na saída.])

= Ocultações de imagem e de som
#if d.ocultacoes.len() == 0 [
  Nenhuma ocultação foi aplicada.
] else [
  #tabela((auto, auto, auto, 3fr, 4fr, 4fr), ([Item], [Grav.], [De · até (original)], [Conteúdo], [Filtro], [Motivo]),
    ..d.ocultacoes.map(x => (x.item, x.gravacao, x.intervalo, x.conteudo, [#x.filtro #if x.quadros != none [\ #text(fill: cinza, x.quadros)]], x.motivo)).flatten(),
  )
  #par(text(fill: cinza)[As ocultações estão gravadas na imagem e no som do vídeo entregue e não podem ser desfeitas a partir dele. As gravações originais não têm ocultação.])
  #if d.conferencia_ocultacoes != none [#par[*Conferência no arquivo final.* #d.conferencia_ocultacoes]]
]

= Sincronização entre câmeras
#tabela-ou([O vídeo não mostra câmeras sincronizadas entre si.], d.sincronizacao.map(s => (s.par, s.tecnico.metodo, s.tecnico.deslocamento, s.tecnico.deriva, s.tecnico.incerteza)), (auto, 1fr, auto, auto, auto), ([Par], [Método], [Deslocamento], [Deriva], [Incerteza]))

= Codificação da saída
#ficha(d.codificacao.map(c => (c.at(0), c.at(1))))

= Programas e versões
#tabela((auto, auto, 1fr), ([Programa], [Versão], [Papel]),
  ..d.software.flatten(),
)
#if d.modelos_ia.len() > 0 [
  #par[*Modelos de inteligência artificial.*]
  #tabela((auto, 1fr, auto), ([Modelo], [Arquivos e SHA-256 · origem], [Uso]),
    ..d.modelos_ia.map(m => (m.modelo, [#for a in m.arquivos [#a.nome \ #hash(a.sha256) \ ] #m.origem], m.uso)).flatten(),
  )
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
