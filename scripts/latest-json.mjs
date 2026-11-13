// latest.json do atualizador (release.yml). Uso: node scripts/latest-json.mjs <versão> <notas.md> <setup.exe.sig> [<AppImage.sig>]
import { readFileSync } from "node:fs";

const [versao, notasMd, sigWindows, sigLinux] = process.argv.slice(2);
if (!versao || !notasMd || !sigWindows) {
  console.error("uso: latest-json.mjs <versão> <notas.md> <setup.exe.sig>");
  process.exit(1);
}
const md = readFileSync(notasMd, "utf8");
// Só a seção "O que mudou" das notas, sem a tabela de downloads.
const m = md.match(/###\s*O que mudou\s*\n([\s\S]*?)(?=\n###\s|$)/);
const notas = (m ? m[1] : md).trim();
const base = `https://github.com/PeritoAndre/sicro/releases/download/v${versao}`;
const plataformas = {
  "windows-x86_64": {
    signature: readFileSync(sigWindows, "utf8").trim(),
    url: `${base}/SICRO-${versao}-windows-x64-setup.exe`,
  },
};
if (sigLinux) {
  plataformas["linux-x86_64"] = {
    signature: readFileSync(sigLinux, "utf8").trim(),
    url: `${base}/SICRO-${versao}-linux-x86_64.AppImage`,
  };
}
const out = {
  version: versao,
  notes: notas,
  pub_date: new Date().toISOString(),
  platforms: plataformas,
};
process.stdout.write(JSON.stringify(out, null, 2) + "\n");
