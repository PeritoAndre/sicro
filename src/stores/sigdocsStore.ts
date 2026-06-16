/**
 * sigdocsStore — estado global do cover do SIGDOC.
 *
 * J — "Cover mode": quando aberto, um webview borderless do SIGDOC
 * cobre EXATAMENTE a área de conteúdo do editor (entre topbar e
 * statusbar, à direita da rail). O React continua renderizando
 * normalmente "por baixo" — mas o webview do SIGDOC fica em cima,
 * dando a impressão de que o portal abriu "no lugar" do laudo.
 *
 * O `SigdocsCoverHost` mede sua própria bounding rect via
 * ResizeObserver e chama `updateSigdocsCoverBounds` no backend.
 */

import { create } from "zustand";

interface SigdocsState {
  /** Cover do SIGDOC está aberto (webview borderless cobrindo o editor). */
  coverOpen: boolean;
  setCoverOpen: (open: boolean) => void;
}

export const useSigdocsStore = create<SigdocsState>((set) => ({
  coverOpen: false,
  setCoverOpen: (open) => set({ coverOpen: open }),
}));
