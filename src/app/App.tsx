import { lazy, Suspense, useEffect } from "react";
import { HashRouter, Navigate, Route, Routes } from "react-router-dom";
import { AppShell } from "./AppShell";
import { WorkspaceProvider } from "./WorkspaceProvider";
import { HomeView } from "@modules/home/HomeView";
import { IntegridadeModule } from "@modules/integridade/IntegridadeModule";
import { lastMidiaAba } from "@modules/midia/midiaNav";
import { ConfiguracoesModule } from "@modules/configuracoes/ConfiguracoesModule";
import { useSettingsStore } from "@stores/settingsStore";
import { Toaster } from "@/components/toast/Toaster";
import { installAutoBackupWatcher } from "@core/autoBackup";
import { installContextMenuGuard } from "@components/ContextMenu/ContextMenu";

// F12.9 — Bundle splitting:
//   - Croqui carrega Konva (~280 KB) + Leaflet (~180 KB) → lazy.
//   - Imagem carrega Konva → lazy.
//   - Video / Lab spike → lazy.
// Home e Integridade ficam no main bundle.
const CroquiModule = lazy(() =>
  import("@modules/croqui/CroquiModule").then((m) => ({
    default: m.CroquiModule,
  })),
);
const VideoModule = lazy(() =>
  import("@modules/video/VideoModule").then((m) => ({
    default: m.VideoModule,
  })),
);
const ImagemModule = lazy(() =>
  import("@modules/imagem/ImagemModule").then((m) => ({
    default: m.ImagemModule,
  })),
);
const AudioModule = lazy(() =>
  import("@modules/audio/AudioModule").then((m) => ({
    default: m.AudioModule,
  })),
);
const DegravacaoView = lazy(() =>
  import("@modules/audio/DegravacaoView").then((m) => ({
    default: m.DegravacaoView,
  })),
);
// Ajuda carrega o manual (texto) + marked → lazy pra ficar fora do bundle main.
const AjudaModule = lazy(() =>
  import("@modules/ajuda/AjudaModule").then((m) => ({
    default: m.AjudaModule,
  })),
);
/** Spinner mínimo enquanto chunks carregam. */
function ModuleLoading() {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: "100%",
        color: "var(--sicro-fg-dim, #94a3b8)",
        fontSize: 13,
      }}
    >
      Carregando módulo…
    </div>
  );
}

function MidiaRedirect() {
  return <Navigate to={lastMidiaAba()} replace />;
}

export function App() {
  // Carrega as configurações globais (perfil, instituição, aparência) uma vez
  // no boot e aplica o tema + cor de destaque ao documento.
  useEffect(() => {
    void useSettingsStore.getState().load();
  }, []);

  // Auto-backup ao fechar/trocar a ocorrência (DR — Fase 2b). O observador
  // dispara o backup geral incremental para a pasta de backup configurada.
  useEffect(() => installAutoBackupWatcher(), []);

  // Sem o menu de navegador do WebKit no botão direito (Voltar, Recarregar…);
  // os menus próprios do SICRO continuam.
  useEffect(() => installContextMenuGuard(), []);

  return (
    <WorkspaceProvider>
      <Toaster />
      <HashRouter>
        <AppShell>
          <Suspense fallback={<ModuleLoading />}>
            <Routes>
              <Route path="/" element={<HomeView />} />
              <Route path="/integridade" element={<IntegridadeModule />} />
              <Route path="/croqui" element={<CroquiModule />} />
              {/* Vídeo e Áudio: um módulo só, com as abas Vídeos / Áudios na
                  barra do topo. /midia volta à aba usada por último. */}
              <Route path="/midia" element={<MidiaRedirect />} />
              <Route path="/video" element={<VideoModule />} />
              <Route path="/audio" element={<AudioModule />} />
              <Route
                path="/audio/degravacao/:audioId"
                element={<DegravacaoView />}
              />
              <Route path="/imagem" element={<ImagemModule />} />
              {/* Endereços de módulos que saíram no 4.0 (ou foram renomeados). */}
              <Route path="/imagens" element={<Navigate to="/imagem" replace />} />
              <Route path="/evidencias" element={<Navigate to="/integridade" replace />} />
              <Route path="/midias" element={<Navigate to="/integridade" replace />} />
              <Route path="/configuracoes" element={<ConfiguracoesModule />} />
              <Route path="/ajuda" element={<AjudaModule />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
          </Suspense>
        </AppShell>
      </HashRouter>
    </WorkspaceProvider>
  );
}
