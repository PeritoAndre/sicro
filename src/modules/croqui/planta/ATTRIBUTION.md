# Atribuição — Motor de Planta Baixa (Croqui de Planta)

O motor 2D de planta baixa do SICRO (paredes com auto-junção, portas/janelas que
grudam na parede, mobília, ferramenta de medida, transform handles e serializer)
é um **fork adaptado** do projeto:

- **arcada** — https://github.com/mehanix/arcada
- Autora: Nicoleta Mehanix (mehanix)
- Licença: **Apache License 2.0**

## O que foi reaproveitado

Os arquivos sob `engine/editor/`, `engine/stores/` e `engine/helpers/` derivam do
diretório `src/` do arcada (Pixi.js). Mantêm a estrutura original e estão marcados
com `// @ts-nocheck` (tratados como código vendido, interfaçado via os tipos do
SICRO).

## O que foi modificado para o SICRO

- Removidas as dependências `@mantine/*`, `tabler-icons-react`, `file-saver` e
  `react-device-detect` (substituídas por `engine/vendorShim.ts`).
- Removida a vista 3D (Three.js) e a UI Mantine — o SICRO usa seu próprio design
  system e renderiza só em 2D.
- Substituído o `api-client` (que buscava catálogo de mobília e definições de
  porta/janela de um servidor `localhost:4133`/MongoDB) por um **catálogo LOCAL
  e offline** (`engine/api/api-client.ts`) com assets SVG **originais** do SICRO
  (`assets/2d/`). Nenhuma dependência de rede.
- Persistência adaptada para o formato `.sicroplanta` (Tauri/SQLite), no lugar do
  `localStorage`/download do arcada.

## Contribuições da comunidade arcada reaproveitadas

Além do código da autora, foram portadas melhorias enviadas por contribuidores ao
arcada (também sob Apache-2.0), com crédito:

- **Editar a medida da parede digitando na cota** (clicar no rótulo e digitar o
  comprimento exato) — baseado no PR
  [mehanix/arcada#14](https://github.com/mehanix/arcada/pull/14) de
  **SSakibHossain10**. Adaptado para o SICRO (offset do canvas, sem Mantine,
  `<input>` no PlantaEditor). Arquivos: `engine/.../Walls/Wall.ts`
  (`updateWallLength`), `engine/.../TransformControls/Label.ts`, `engine/.../Main.ts`.

A licença Apache-2.0 original é preservada; este arquivo cumpre o requisito de
atribuição/NOTICE.
