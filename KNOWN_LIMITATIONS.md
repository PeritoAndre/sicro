# Known Limitations — SICRO Desktop 2.0 Alpha

Versão: MVP 8 / Alpha-prep · Data: 2026-05-25.

Este arquivo lista limitações **conhecidas** do SICRO Alpha. Não é
uma lista de bugs ou de funcionalidades por fazer; é o contrato
honesto com o operador.

---

## 1. Exportação DOCX com imagens reais

**Status:** placeholder em vez da imagem real.
**Origem:** MVP 4 (ressalva técnica), confirmada na validação manual.
**Diagnóstico:** `docx-rs 0.4.x` aceita o `Pic` mas o resultado não
aparece renderizado no Word (provavelmente faltam relacionamentos
em `_rels/document.xml.rels` ou a função `add_image` está
silenciosamente perdendo o stream).
**Workaround:** use **PDF** como saída oficial. DOCX continua
disponível como saída editável, mas com placeholder em vez da imagem.
**Próximo passo recomendado:** Spike DOCX-imagens (ver
`MVP4_EVIDENCIAS_NO_LAUDO_RELATORIO.md` §7).

## 2. Paginação do editor de Laudo é "soft"

**Status:** o editor renderiza uma única tira branca contínua com
marcadores tracejados a cada 29,7 cm sinalizando onde cai a quebra
de página no PDF final.
**Origem:** decisão arquitetural do MVP 2 (paginação visual via
paper stack).
**Impacto:** o número de páginas exibido no editor é aproximado;
o PDF final pode ter ±1 página em relação ao mostrado.
**Workaround:** use o botão "Prévia HTML" do Laudo para conferir
a paginação real antes de exportar.

## 3. Vídeo — dependências externas

**Status:** `ffmpeg` e `ffprobe` precisam estar no PATH.
**Origem:** Spike F.
**Impacto:** se uma das duas estiver ausente, o módulo Vídeo
não consegue registrar mídia nem coletar frames.
**Sinalização:** o AlphaDashboard mostra um pill "ausente" ao lado
de `ffmpeg` / `ffprobe` quando não as encontra.
**Próximo passo:** empacotar ambos no installer Alpha (futuro spike
de empacotamento).

## 4. Editor de Imagem — operações geométricas sem UI

**Status:** o backend Rust (`image_editor/processor.rs`) suporta
`rotate_90_cw/ccw`, `rotate_180`, `flip_h/v`, `crop`, `resize` —
todos com testes. A UI do MVP 7 não expõe botões para eles.
**Impacto:** o perito não consegue girar/cortar/redimensionar pela
interface ainda. Só ajustes não destrutivos (brilho/contraste/gamma/
saturação/grayscale/invert) e anotações.
**Workaround:** nenhum por enquanto na UI; usar export e edição
externa se necessário.
**Próximo passo:** MVP 9 ou Spike "UI de operações geométricas".

## 5. Editor de Imagem — sem FFT/Wavelets/CLAHE/autenticação profunda

**Status:** o Editor MVP 7 entrega a **fundação** (canvas + camadas
+ anotações + ajustes + escala + medida + tarja + export derivado +
sidecar JSON). Filtros forenses avançados (Sobel/Canny/Laplaciano/
CLAHE/blur gaussiano/mediana/Kuwahara/FFT/Wavelets) e detecção de
manipulação ficaram **fora de escopo deliberado**.
**Próximo passo:** MVP 9 — Filtros Forenses (Sobel/Canny/CLAHE/etc),
extensível via o enum `BackendOperation` já em pé.

## 6. EXIF não é lido

**Status:** `get_image_metadata` retorna `exif_json: null`.
**Origem:** decisão de escopo do MVP 7.
**Workaround:** dimensões / mime / hash já são lidos. EXIF detalhado
fica para spike próprio.

## 7. Editor de Imagem — sem undo/redo persistente

**Status:** ações destrutivas (delete de anotação) são imediatas.
Não há histórico de undo dentro de uma sessão.
**Workaround:** `image_operation_logs` registra auditoria
operacional, mas não permite voltar ao estado anterior.

## 8. Croqui Pericial — sem OSM / Google Maps / ortorretificação

**Status:** o módulo Croqui MVP 6 usa imagem de fundo do disco ou
do Dossiê. Não há integração com mapas externos nem correção de
perspectiva de imagens aéreas (drone).
**Workaround:** importe a imagem do drone como fundo e ajuste
manualmente a escala.

## 9. Croqui — sem edição de vértices individuais

**Status:** linhas e polylines são arrastadas inteiras pelo
Transformer. Não há edição por vértice (ainda).
**Workaround:** apague a linha e desenhe nova.

## 10. Performance — Konva > 500 objetos / imagens > 12 MP

**Status:** o Croqui e o Editor de Imagem funcionam bem até ~500
objetos / ~12 MP. Acima disso a interação fica mais pesada
(zoom/pan mantém ok, drag de seleção fica notavelmente mais lento).
**Workaround:** subdividir o trabalho em mais croquis / análises.

## 11. Verificação profunda pode ser lenta em vídeos grandes

**Status:** "Verificação profunda" da Central de Evidências
(MVP 5) recompute SHA-256 de cada item. Em vídeos > 1 GB demora.
Não há cancelamento.
**Workaround:** rode em background ou só quando necessário.

## 12. Instalador final ainda não validado

**Status:** `pnpm tauri build` ainda não foi validado em múltiplas
máquinas Windows. O empacotamento Alpha (instalador `.msi`/`.exe`)
é trabalho de outro spike.
**Workaround:** rode via `pnpm tauri dev` ou empacote manualmente
com `pnpm tauri build` em sua máquina (deve funcionar mas sem
garantias de bundle reproduzível).

## 13. Não há IA / OCR / análise automática

**Status:** decisão arquitetural permanente. O SICRO Desktop é
ferramenta de suporte ao perito; **não interpreta** evidência, **não
conclui** culpa, **não calcula** velocidade automaticamente, **não
faz** OCR, **não detecta** manipulação. Tudo é controlado pelo
operador.
**Não é limitação técnica — é princípio do produto.**

## 14. Alpha não substitui validação humana

**Status:** mensagem que precisa ficar na cabeça do operador.
A versão Alpha é para testes controlados em workspace descartável
ou em ocorrências de baixo risco. **Todo laudo gerado precisa ser
revisto pelo perito antes de ser usado oficialmente.**

---

## Como reportar

Crie um issue no GitHub do projeto (`SICRO-desktop`) com:
- versão (visível no AlphaDashboard / Relatório de Saúde);
- workspace de teste (NÃO compartilhe workspace com dados reais);
- passos para reproduzir;
- screenshots se aplicável.
