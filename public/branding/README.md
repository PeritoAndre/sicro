# Branding assets

Brasões institucionais usados no cabeçalho do laudo (módulo Laudo, MVP 2).

| Arquivo | Uso |
|---|---|
| `brasao-amapa.png` | Brasão do Estado do Amapá — centralizado no topo do cabeçalho, sobre a linha "GOVERNO DO ESTADO DO AMAPÁ" |
| `brasao-pca.png` | Brasão da Polícia Científica do Amapá — à esquerda das três linhas brand, na faixa "POLÍCIA CIENTÍFICA DO ESTADO DO AMAPÁ" |

## Estado atual

Os arquivos atualmente presentes são **placeholders** gerados por um script
PowerShell durante o ajuste do MVP 2 (selos circulares simples com as siglas
"AP" e "PCA"). Eles existem apenas para destravar o layout — **não use em
documentação oficial**.

## Como substituir pelos brasões oficiais

1. Copie os arquivos PNG oficiais sobre os placeholders, mantendo os nomes:
   - `public/branding/brasao-amapa.png`
   - `public/branding/brasao-pca.png`
2. Recomendação de tamanho: **240×240 px** (ou maior, proporção quadrada).
   O CSS do cabeçalho redimensiona via `width`/`height`.
3. Recomendação de formato: PNG com transparência (fundo `transparent`),
   para que o brasão fique sobre o branco da folha.
4. Reinicie `pnpm tauri:dev` se ele estiver rodando — Vite recarrega
   assets de `public/` automaticamente em dev.

## Pipeline

- **Editor (TipTap)**: as imagens são referenciadas via `/branding/*.png` no
  `<img>` do cabeçalho decorativo de `EditorPage`. Vite serve o `public/`
  na raiz, então o WebView2 do Tauri consegue carregá-las direto.
- **Exportação HTML/PDF**: o renderer pré-carrega ambos os arquivos via
  `fetch()` no front-end, converte para **data URI base64** e injeta no
  HTML emitido. Isso é necessário porque o Edge headless lê o HTML temp
  em `<workspace>/cache/` e não enxerga o caminho `/branding/`. Por isso
  o pré-carregamento acontece antes de cada export e o cache é
  invalidado se o arquivo mudar.
- **Exportação DOCX**: brasões **não são embutidos** nesta versão.
  Ver `MVP2_LAUDO_INSTITUCIONAL_RELATORIO.md` para a justificativa
  (`docx-rs::Image` + `Pic` + relacionamentos requer trabalho não-trivial
  e fidelidade entre Word/LibreOffice/Office Mobile é instável).
