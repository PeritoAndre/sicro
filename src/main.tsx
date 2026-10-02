import { createRoot } from "react-dom/client";
import { App } from "@app/App";
// CSS do Leaflet uma vez na raiz, para qualquer módulo que monte um mapa.
import "leaflet/dist/leaflet.css";
import "./index.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("Missing #root element in index.html");
}

// Sem StrictMode de propósito: o mount duplo em DEV deixava DOM órfão de editores visível ("texto fantasma").
createRoot(container).render(<App />);
