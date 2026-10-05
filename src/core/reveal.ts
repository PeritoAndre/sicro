/** Abre a pasta do arquivo exportado com ele selecionado, no gerenciador padrão do sistema. */

import { commands } from "./commands";

export function revealExported(workspacePath: string, path: string): void {
  const win = workspacePath.includes("\\");
  const sep = win ? "\\" : "/";
  const abs = /^([a-zA-Z]:)?[\\/]/.test(path)
    ? path
    : `${workspacePath.replace(/[\\/]+$/, "")}${sep}${path.replace(/^[\\/]+/, "")}`;
  void commands.revealPathInExplorer(win ? abs.replace(/\//g, "\\") : abs).catch(() => undefined);
}
