import { createRoot } from "react-dom/client";
import { App } from "@app/App";
// MVP 10 — Leaflet's default stylesheet ships the icon sprite + the
// internal positioning for the zoom controls / popups. Imported once
// at the app root so any module (Croqui OSM modal) that mounts a map
// inherits the correct visuals.
import "leaflet/dist/leaflet.css";
import "./index.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("Missing #root element in index.html");
}

// F7.10 — StrictMode removido. Motivo: TipTap React (@tiptap/react)
// implementa `componentWillUnmount` movendo nós DOM do contenteditable
// para um `<div>` órfão, e re-anexando no remount. StrictMode dispara
// mount→unmount→remount em DEV, e em alguns frames o DOM órfão
// (carregando o estado ANTIGO sem spacers de paginação) permanece
// visível sobreposto ao DOM novo (com spacers). O resultado é o
// "texto fantasma" — duas versões do editor renderizadas ao mesmo
// tempo. Removendo StrictMode, o componentDidMount/Unmount roda só uma
// vez e o DOM órfão é descartado pelo garbage collector. Em produção
// StrictMode já não causa esse comportamento.
createRoot(container).render(<App />);
