import { lazy, Suspense, useEffect } from "react";
import { HashRouter, Navigate, Route, Routes } from "react-router-dom";
import { AppShell } from "./AppShell";
import { WorkspaceProvider } from "./WorkspaceProvider";
import { HomeView } from "@modules/home/HomeView";
import { LaudoModule } from "@modules/laudo/LaudoModule";
import { DossieModule } from "@modules/dossie/DossieModule";
import { ConfiguracoesModule } from "@modules/configuracoes/ConfiguracoesModule";
import { useSettingsStore } from "@stores/settingsStore";
import { Toaster } from "@/components/toast/Toaster";
import { installAutoBackupWatcher } from "@core/autoBackup";

// F12.9 — Bundle splitting:
//   - Croqui carrega Konva (~280 KB) + Leaflet (~180 KB) → lazy.
//   - Imagem carrega Konva → lazy.
//   - Video / Lab spike → lazy.
// Home, Laudo e Dossiê (que agora inclui o modo Integridade, ex-Evidências)
// ficam no main bundle pois são frequentemente usados juntos no fluxo pericial.
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
const EstatisticasModule = lazy(() =>
  import("@modules/estatisticas/EstatisticasModule").then((m) => ({
    default: m.EstatisticasModule,
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
const DocumentoscopiaModule = lazy(() =>
  import("@modules/documentoscopia/DocumentoscopiaModule").then((m) => ({
    default: m.DocumentoscopiaModule,
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

export function App() {
  // Carrega as configurações globais (perfil, instituição, aparência) uma vez
  // no boot e aplica o tema + cor de destaque ao documento.
  useEffect(() => {
    void useSettingsStore.getState().load();
  }, []);

  // Auto-backup ao fechar/trocar a ocorrência (DR — Fase 2b). O observador
  // dispara o backup geral incremental para a pasta de backup configurada.
  useEffect(() => installAutoBackupWatcher(), []);

  return (
    <WorkspaceProvider>
      <Toaster />
      <HashRouter>
        <AppShell>
          <Suspense fallback={<ModuleLoading />}>
            <Routes>
              <Route path="/" element={<HomeView />} />
              <Route path="/dossie" element={<DossieModule />} />
              <Route path="/laudo" element={<LaudoModule />} />
              <Route path="/croqui" element={<CroquiModule />} />
              <Route path="/video" element={<VideoModule />} />
              <Route path="/audio" element={<AudioModule />} />
              <Route
                path="/audio/degravacao/:audioId"
                element={<DegravacaoView />}
              />
              {/* Evidências foi unificado ao Dossiê (modo Integridade). Mantemos
                  o redirect para não quebrar links/atalhos antigos. */}
              <Route
                path="/evidencias"
                element={<Navigate to="/dossie?modo=integridade" replace />}
              />
              <Route path="/imagem" element={<ImagemModule />} />
              <Route
                path="/imagens"
                element={<Navigate to="/imagem" replace />}
              />
              {/* "Mídias" foi removido: sua função (biblioteca de evidências
                  com hash e vínculos) já vive no Dossiê → modo Integridade. */}
              <Route
                path="/midias"
                element={<Navigate to="/dossie?modo=integridade" replace />}
              />
              <Route
                path="/documentoscopia"
                element={<DocumentoscopiaModule />}
              />
              <Route path="/estatisticas" element={<EstatisticasModule />} />
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
