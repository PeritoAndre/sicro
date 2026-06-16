# Icons — placeholders gerados no Spike A

Estes ícones são **placeholders técnicos** gerados a partir de `source.png` (1024×1024, fundo navy + letra "S" dourada, refletindo as cores do design system). Servem apenas para destravar o build do Tauri no Windows (`icon.ico` é exigido por `tauri-build` ao montar o Windows Resource).

Não use estes arquivos como branding final.

## Como foram gerados

1. `source.png` foi gerado por um script PowerShell com `System.Drawing` (cores `#07111f`, `#111f35`, `#d7a84f`).
2. Em seguida, `pnpm tauri icon src-tauri/icons/source.png` foi executado para gerar todos os tamanhos abaixo.

## Arquivos atuais

- `icon.ico` — referenciado por `tauri.conf.json` e pelo `tauri-build` no Windows.
- `icon.icns` — macOS.
- `icon.png` — base 1024×1024.
- `32x32.png`, `64x64.png`, `128x128.png`, `128x128@2x.png` — Linux/X11.
- `Square*Logo.png` + `StoreLogo.png` — Microsoft Store / MSIX.
- `source.png` — PNG fonte (1024×1024). Conserve esse arquivo: novas gerações partem dele.

## Como regenerar

```powershell
cd "C:\SICRO 2.0\sicro-desktop"
pnpm tauri icon src-tauri/icons/source.png
```

## Quando trocar o branding final

Quando o time de design entregar o ícone definitivo (PDF mestre §22, Design System §30.4):

1. Substitua `source.png` (mesma resolução de 1024×1024 ou maior, fundo transparente OK).
2. Rode o comando acima.
3. Faça commit dos arquivos resultantes.
