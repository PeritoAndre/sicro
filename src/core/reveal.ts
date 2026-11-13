/** Abre a pasta do arquivo exportado com ele selecionado, no gerenciador padrão do sistema. */

import { caminhoDoCaso } from "./caminhoDoCaso";
import { commands } from "./commands";

export function revealExported(workspacePath: string, path: string): void {
  const win = workspacePath.includes("\\");
  const abs = /^([a-zA-Z]:)?[\\/]/.test(path)
    ? path
    : caminhoDoCaso(workspacePath, path);
  void commands.revealPathInExplorer(win ? abs.replace(/\//g, "\\") : abs).catch(() => undefined);
}
