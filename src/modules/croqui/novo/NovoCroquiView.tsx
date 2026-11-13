/**
 * "Novo croqui viário": a cena em metros (nunca em pixel), ou um lugar no mapa, ou uma foto de drone.
 * A folha e a escala nascem prontas; só o drone deixa a escala para o editor (ferramenta Escala em destaque).
 */

import { Component, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Group, Layer, Line, Rect, Stage, Text } from "react-konva";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { ArrowRight, FileImage, Globe, MapPin, Plus, Search, Square, X } from "lucide-react";
import { CampoNumero } from "@components/CampoNumero/CampoNumero";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { selectActiveOccurrence, selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import { useCroquiStore } from "../store/croquiStore";
import type { SicroCroquiDoc, SicroObject } from "../engine";
import { parseCoordinates, type LatLon } from "../engine/coordinates";
import { fetchOverpassBBox } from "../engine/osm";
import { convertOsmDatasetToParityObjects } from "../engine/road-parity/osmAdapter";
import { CENA_MAX_M, CENA_MIN_M, CENA_PADRAO, CENA_PRESETS, cenasSalvas, esquecerCena, folhaEmPx, limitarCena, nomePadrao, pxPorMetroParaCena, salvarCena } from "../engine/novoCroqui";
import { GRUPOS_DE_MAPA_BASE, MAPAS_BASE, mapaBasePorId } from "../engine/mapasBase";
import { montarFundoDeMapa } from "../engine/fundoDeMapa";
import { caixaDoRaio } from "../engine/mercator";
import { RoadParityRenderer } from "../engine/road-parity/renderer";
import type { SicroParityObject } from "../engine/road-parity/types";
import cenaMacapa from "../assets/cena-macapa.json";
import { DroneAjuste, EXTENSOES_DE_FOTO, carregarImagemDrone, type DroneAjusteEstado } from "../editor/DroneImportModal";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { OsmMapPanel } from "../editor/OsmMapPanel";
import styles from "./NovoCroquiView.module.css";

type Modo = "branco" | "mapa" | "drone";
const FONTE_VIAS = "osm-vias";
const LADO_MIN = 10;
const LADO_MAX = 1000;
const LADOS_PRONTOS = [20, 50, 100, 200, 300];
/** Fração da área de prévia que a folha ocupa. */
const PREVIA_OCUPACAO = 0.86;
/** O adaptador do OSM encaixa 25 m de raio no canvas que recebe: com 50 m de canvas, px/m sai igual ao pedido. */
const RAIO_DE_REFERENCIA_DO_ADAPTADOR_M = 25;

/** Leaflet quebrando não derruba a tela: fica só a coordenada. */
class MapaBoundary extends Component<{ children: ReactNode }, { falhou: boolean }> {
  state = { falhou: false };
  static getDerivedStateFromError() {
    return { falhou: true };
  }
  render() {
    return this.state.falhou ? <div className={styles.mapaFalhou}>Mapa indisponível</div> : this.props.children;
  }
}

export function NovoCroquiView() {
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const list = useCroquiStore((s) => s.list);
  const createCroqui = useCroquiStore((s) => s.createCroqui);
  const saveCurrent = useCroquiStore((s) => s.saveCurrent);
  const fecharNovoViario = useCroquiStore((s) => s.fecharNovoViario);
  const clearCurrent = useCroquiStore((s) => s.clearCurrent);

  const [nome, setNome] = useState(() => nomePadrao(list.map((c) => c.title)));
  const [modo, setModo] = useState<Modo>("branco");
  const [busy, setBusy] = useState(false);
  const [progresso, setProgresso] = useState<string | null>(null);
  const [erro, setErro] = useState<string | null>(null);

  // Folha em branco.
  const [wM, setWM] = useState(CENA_PADRAO.w);
  const [hM, setHM] = useState(CENA_PADRAO.h);
  const [salvas, setSalvas] = useState(cenasSalvas);
  const chipsCena = [...CENA_PRESETS, ...salvas];

  // Mapa ou satélite.
  const [lugar, setLugar] = useState("");
  const [buscando, setBuscando] = useState(false);
  const [achados, setAchados] = useState<{ nome: string; lat: number; lon: number }[]>([]);
  const [centro, setCentro] = useState<LatLon | null>(
    occurrence?.latitude != null && occurrence?.longitude != null ? { lat: occurrence.latitude, lon: occurrence.longitude } : null,
  );
  /** Lado da folha (m); o que vai ao OSM e aos tiles é o quadrado inteiro. */
  const [lado, setLado] = useState(50);
  const raio = lado / 2;
  const [fonte, setFonte] = useState<string>(FONTE_VIAS);
  const [trazerVias, setTrazerVias] = useState(true);

  // Drone.
  const [drone, setDrone] = useState<DroneAjusteEstado>({ loaded: null, crop: null, intensity: 0.1 });

  const folha = useMemo(() => folhaEmPx(wM, hM), [wM, hM]);
  const presetAtivo = chipsCena.find((p) => (p.w === wM && p.h === hM) || (p.w === hM && p.h === wM)) ?? null;
  const retrato = hM >= wM;

  const buscarLugar = useCallback(async () => {
    const q = lugar.trim();
    if (!q) return;
    // Coordenada colada direto ("0.03 S, 51.05 W", "-0.03, -51.05"…) nem passa pela rede.
    const coord = parseCoordinates(q);
    if (coord.ok && coord.value) {
      setCentro(coord.value);
      setAchados([]);
      return;
    }
    setBuscando(true);
    setErro(null);
    try {
      const r = await commands.geocodePlace(q);
      setAchados(r);
      if (r.length === 1) setCentro({ lat: r[0]!.lat, lon: r[0]!.lon });
      if (r.length === 0) setErro("Nenhum lugar com esse nome.");
    } catch (e) {
      setErro(toSicroError(e).message);
    } finally {
      setBuscando(false);
    }
  }, [lugar]);

  /** GPS de uma foto do caso (EXIF via exiftool); a foto precisa estar na pasta do caso. */
  const gpsDaFoto = useCallback(async () => {
    if (!workspacePath) return;
    const picked = await openFileDialog({
      multiple: false,
      title: "Foto com GPS",
      defaultPath: workspacePath,
      filters: [{ name: "Imagens", extensions: ["jpg", "jpeg", "png", "tif", "tiff", "webp"] }],
    });
    if (typeof picked !== "string") return;
    const raiz = workspacePath.replace(/\\/g, "/").replace(/\/$/, "");
    const abs = picked.replace(/\\/g, "/");
    if (!abs.startsWith(raiz + "/")) {
      setErro("A foto precisa estar dentro da pasta do caso.");
      return;
    }
    try {
      const meta = await commands.readAllImageMetadata(workspacePath, abs.slice(raiz.length + 1));
      const lat = lerGrau(meta.entries.find((e) => /^GPS ?Latitude$/i.test(e.tag))?.value);
      const lon = lerGrau(meta.entries.find((e) => /^GPS ?Longitude$/i.test(e.tag))?.value);
      if (lat == null || lon == null) {
        setErro("Essa foto não tem GPS.");
        return;
      }
      setCentro({ lat, lon });
      setErro(null);
    } catch (e) {
      setErro(toSicroError(e).message);
    }
  }, [workspacePath]);

  const podeCriar =
    !busy && (modo === "branco" || (modo === "mapa" && centro != null) || (modo === "drone" && drone.loaded != null && drone.crop != null));

  const criar = useCallback(async () => {
    if (!workspacePath || !podeCriar) return;
    setBusy(true);
    setErro(null);
    setProgresso(null);
    try {
      const titulo = nome.trim() || nomePadrao(list.map((c) => c.title));
      const created = await createCroqui(workspacePath, titulo, "viario");
      const doc = useCroquiStore.getState().activeDoc;
      if (!doc) throw new Error("croqui criado sem documento");
      const next = await prepararDoc(doc);
      await saveCurrent(workspacePath, next);
      fecharNovoViario();
      void created;
    } catch (e) {
      setErro(toSicroError(e).message);
    } finally {
      setBusy(false);
      setProgresso(null);
    }

    async function prepararDoc(doc: SicroCroquiDoc): Promise<SicroCroquiDoc> {
      const grade = (ppm: number) => ({ enabled: true, size_px: Math.round(ppm), size_m: 1 });
      if (modo === "branco") {
        return {
          ...doc,
          canvas: { ...doc.canvas, width_px: folha.width_px, height_px: folha.height_px, origin_x: 0, origin_y: 0, grid: grade(folha.px_per_m) },
          scale: { px_per_m: folha.px_per_m },
        };
      }
      if (modo === "drone") {
        const { loaded, crop, intensity } = drone;
        if (!loaded || !crop) throw new Error("escolha a foto");
        setProgresso("Corrigindo e recortando a foto…");
        const r = await commands.importDroneImage(workspacePath!, {
          source_absolute_path: loaded.absolutePath,
          intensity,
          crop,
          croqui_id: useCroquiStore.getState().activeCroquiId ?? undefined,
          occurrence_id: occurrence?.id,
        });
        return {
          ...doc,
          canvas: { ...doc.canvas, width_px: r.output_width, height_px: r.output_height, origin_x: 0, origin_y: 0 },
          scale: null,
          background_image: {
            source_path: r.output_relative_path,
            x: 0,
            y: 0,
            width: r.output_width,
            height: r.output_height,
            opacity: 1,
            locked: true,
            rotation: 0,
            sidecar_path: r.sidecar_relative_path,
          },
        };
      }
      // Mapa ou satélite.
      const c = centro!;
      // A folha fica na resolução padrão (A4 a 300 dpi) seja qual for o zoom dos tiles; a imagem é fundo bloqueado esticado à folha.
      const ppm = pxPorMetroParaCena(lado, lado);
      const W = Math.round(lado * ppm);
      const H = W;
      let background: SicroCroquiDoc["background_image"] = null;
      const mapa = fonte === FONTE_VIAS ? null : mapaBasePorId(fonte);
      if (mapa) {
        const fundo = await montarFundoDeMapa(mapa, c.lat, c.lon, raio, ppm, (f, t) => setProgresso(`Baixando ${mapa.rotulo}: ${f} de ${t}`));
        setProgresso("Gravando o fundo…");
        const rel = await commands.saveCroquiBackground(workspacePath!, fundo.pngBase64, mapa.id);
        background = { source_path: rel, x: 0, y: 0, width: W, height: H, opacity: 1, locked: true, rotation: 0 };
      }
      let objects: SicroObject[] = doc.objects;
      let osmImports = doc.osm_imports ?? [];
      if (!mapa || trazerVias) {
        setProgresso("Buscando as vias no OpenStreetMap…");
        const caixa = caixaDoRaio(c.lat, c.lon, raio);
        const dataset = await fetchOverpassBBox(caixa);
        const ladoRef = 2 * RAIO_DE_REFERENCIA_DO_ADAPTADOR_M * ppm;
        const res = convertOsmDatasetToParityObjects({
          ways: dataset.ways,
          nodes: dataset.nodes,
          center: c,
          radius_m: raio * Math.SQRT2,
          canvas: { width: ladoRef, height: ladoRef },
          options: { margin: 0, clip_to_radius: true, simplify_tolerance_m: 0.6, min_way_length_m: 4, preserve_roundabouts: true, ignore_non_vehicle: true },
        });
        // Objetos parity ficam em metros: o adaptador centra em 25 m, a folha centra no meio dela.
        const dx = W / 2 / ppm - RAIO_DE_REFERENCIA_DO_ADAPTADOR_M;
        const dy = H / 2 / ppm - RAIO_DE_REFERENCIA_DO_ADAPTADOR_M;
        const vias: SicroObject[] = [
          ...res.roads.map((o) => ({ ...o, ax: o.ax + dx, ay: o.ay + dy, bx: o.bx + dx, by: o.by + dy, cx1: o.cx1 + dx, cy1: o.cy1 + dy, cx2: o.cx2 + dx, cy2: o.cy2 + dy })),
          ...res.roundabouts.map((o) => ({ ...o, cx: o.cx + dx, cy: o.cy + dy })),
        ];
        objects = [...objects, ...vias];
        osmImports = [
          ...osmImports,
          {
            imported_at: new Date().toISOString(),
            source: "osm:overpass",
            center_lat: c.lat,
            center_lon: c.lon,
            radius_m: raio,
            query_bbox: caixa,
            selected_way_ids: dataset.ways.map((w) => w.id).sort((a, b) => a - b),
            suggested_px_per_m: ppm,
          },
        ];
      }
      return {
        ...doc,
        canvas: { ...doc.canvas, width_px: W, height_px: H, origin_x: 0, origin_y: 0, grid: grade(ppm) },
        scale: { px_per_m: ppm },
        background_image: background,
        objects,
        osm_imports: osmImports,
      };
    }
  }, [workspacePath, podeCriar, nome, list, createCroqui, saveCurrent, fecharNovoViario, modo, folha, drone, occurrence, centro, raio, lado, fonte, trazerVias]);

  const cancelar = () => {
    if (busy) return;
    fecharNovoViario();
    clearCurrent();
  };

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") cancelar();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [busy]);

  const linhaFolha =
    modo === "branco"
      ? `${folha.px_per_m} px/m · ${folha.width_px} × ${folha.height_px} px · A4 a 300 dpi`
      : modo === "mapa"
        ? `folha ${lado} × ${lado} m · ${pxPorMetroParaCena(lado, lado)} px/m`
        : drone.crop
          ? `folha ${drone.crop.width} × ${drone.crop.height} px · escala no editor`
          : "";

  return (
    <div className={styles.wrap}>
      <header className={styles.header}>
        <button type="button" className={styles.crumb} onClick={cancelar}>
          Croquis
        </button>
        <span className={styles.crumbSep}>›</span>
        <span className={styles.crumbAtual}>Novo croqui viário</span>
        {occurrence?.titulo && <span className={styles.caso}>{occurrence.titulo}</span>}
      </header>

      <div className={styles.body}>
        <section className={styles.col} aria-label="Configuração do croqui">
          <label className={styles.campo}>
            <span className={styles.rotulo}>NOME</span>
            <input className={styles.input} value={nome} onChange={(e) => setNome(e.target.value)} disabled={busy} />
          </label>

          <fieldset className={styles.modos}>
            <legend className={styles.rotulo}>COMEÇAR COM</legend>
            <ModoCard ativo={modo === "branco"} onClick={() => setModo("branco")} icone={<Square size={20} />} titulo="Folha em branco" sub="Você escolhe o tamanho da cena" />
            <ModoCard ativo={modo === "mapa"} onClick={() => setModo("mapa")} icone={<Globe size={20} />} titulo="Mapa ou satélite" sub="OSM, Google ou Esri de um lugar e tamanho" />
            <ModoCard ativo={modo === "drone"} onClick={() => setModo("drone")} icone={<FileImage size={20} />} titulo="Foto de drone" sub="A folha nasce do tamanho da foto" />
          </fieldset>

          {modo === "branco" && (
            <div className={styles.bloco}>
              <span className={styles.rotulo}>TAMANHO DA CENA</span>
              <div className={styles.chips}>
                {chipsCena.map((p) => {
                  const salva = salvas.includes(p);
                  return (
                    <span key={`${p.w}x${p.h}${salva ? "s" : ""}`} className={`${styles.chip} ${presetAtivo === p ? styles.chipAtivo : ""} ${salva ? styles.chipSalva : ""}`}>
                      <button
                        type="button"
                        className={styles.chipBtn}
                        onClick={() => {
                          setWM(retrato ? Math.min(p.w, p.h) : Math.max(p.w, p.h));
                          setHM(retrato ? Math.max(p.w, p.h) : Math.min(p.w, p.h));
                        }}
                      >
                        {p.w} × {p.h}
                      </button>
                      {salva && (
                        <button type="button" className={styles.chipX} aria-label={`Esquecer ${p.w} × ${p.h}`} onClick={() => setSalvas(esquecerCena(p.w, p.h))}>
                          <X size={12} />
                        </button>
                      )}
                    </span>
                  );
                })}
                {!presetAtivo && (
                  <button type="button" className={`${styles.chip} ${styles.chipSalvar}`} onClick={() => setSalvas(salvarCena(wM, hM))} aria-label={`Salvar ${wM} × ${hM} como tamanho meu`}>
                    <Plus size={12} /> salvar {wM} × {hM}
                  </button>
                )}
              </div>
              <div className={styles.duas}>
                <CampoNumero label="Largura" value={wM} step={5} min={CENA_MIN_M} max={CENA_MAX_M} unidade="m" onChange={(v) => setWM(limitarCena(v))} />
                <CampoNumero label="Altura" value={hM} step={5} min={CENA_MIN_M} max={CENA_MAX_M} unidade="m" onChange={(v) => setHM(limitarCena(v))} />
              </div>
              <div className={styles.linhaEntre}>
                <div className={styles.orient}>
                  <button type="button" aria-label="Retrato" className={`${styles.orientBtn} ${retrato ? styles.orientAtivo : ""}`} onClick={() => wM > hM && (setWM(hM), setHM(wM))}>
                    <svg aria-hidden="true" width="14" height="18" viewBox="0 0 14 18" fill="none" stroke="currentColor" strokeWidth="2"><rect x="1" y="1" width="12" height="16" rx="1" /></svg>
                  </button>
                  <button type="button" aria-label="Paisagem" className={`${styles.orientBtn} ${!retrato ? styles.orientAtivo : ""}`} onClick={() => hM > wM && (setWM(hM), setHM(wM))}>
                    <svg aria-hidden="true" width="18" height="14" viewBox="0 0 18 14" fill="none" stroke="currentColor" strokeWidth="2"><rect x="1" y="1" width="16" height="12" rx="1" /></svg>
                  </button>
                </div>
                <span className={styles.mono}>{linhaFolha}</span>
              </div>
            </div>
          )}

          {modo === "mapa" && (
            <div className={styles.bloco}>
              <span className={styles.rotulo}>LUGAR</span>
              <div className={styles.busca}>
                <Search size={16} aria-hidden />
                <input
                  className={styles.buscaInput}
                  placeholder="Endereço ou coordenada"
                  value={lugar}
                  onChange={(e) => setLugar(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      void buscarLugar();
                    }
                  }}
                  disabled={busy}
                />
                <button type="button" className={styles.miniBtn} onClick={() => void buscarLugar()} disabled={busy || buscando}>
                  {buscando ? "…" : "Buscar"}
                </button>
              </div>
              {achados.length > 1 && (
                <ul className={styles.achados}>
                  {achados.map((a) => (
                    <li key={`${a.lat},${a.lon}`}>
                      <button
                        type="button"
                        onClick={() => {
                          setCentro({ lat: a.lat, lon: a.lon });
                          setAchados([]);
                          setLugar(a.nome.split(",").slice(0, 3).join(","));
                        }}
                      >
                        <MapPin size={12} aria-hidden /> {a.nome}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <div className={styles.linhaEntre}>
                <span className={styles.mono}>{centro ? formatarLatLon(centro) : "sem lugar"}</span>
                <span className={styles.links}>
                  {occurrence?.latitude != null && occurrence?.longitude != null && (
                    <button type="button" className={styles.link} onClick={() => setCentro({ lat: occurrence.latitude!, lon: occurrence.longitude! })}>
                      Coordenada do caso
                    </button>
                  )}
                  <button type="button" className={styles.link} onClick={() => void gpsDaFoto()}>
                    GPS de uma foto
                  </button>
                </span>
              </div>

              <span className={styles.subRotulo}>Lado da folha</span>
              <div className={styles.chips}>
                {LADOS_PRONTOS.map((l) => (
                  <span key={l} className={`${styles.chip} ${lado === l ? styles.chipAtivo : ""}`}>
                    <button type="button" className={styles.chipBtn} onClick={() => setLado(l)}>
                      {l} m
                    </button>
                  </span>
                ))}
              </div>
              <div className={styles.raio}>
                <input type="range" min={LADO_MIN} max={LADO_MAX} step={10} value={lado} onChange={(e) => setLado(Number(e.target.value))} aria-label="Lado da folha em metros" />
                <CampoNumero value={lado} step={10} min={LADO_MIN} max={LADO_MAX} unidade="m" onChange={setLado} className={styles.ladoCampo} />
              </div>

              <label className={styles.campo}>
                <span className={styles.rotulo}>FONTE</span>
                <select className={styles.input} value={fonte} onChange={(e) => setFonte(e.target.value)} disabled={busy}>
                  <optgroup label="Vias (objetos editáveis)">
                    <option value={FONTE_VIAS}>OpenStreetMap · vias</option>
                  </optgroup>
                  {GRUPOS_DE_MAPA_BASE.map((g) => (
                    <optgroup key={g.id} label={g.rotulo}>
                      {MAPAS_BASE.filter((m) => m.grupo === g.id).map((m) => (
                        <option key={m.id} value={m.id}>
                          {m.rotulo}
                        </option>
                      ))}
                    </optgroup>
                  ))}
                </select>
              </label>
              {fonte !== FONTE_VIAS && (
                <label className={styles.check}>
                  <input type="checkbox" checked={trazerVias} onChange={(e) => setTrazerVias(e.target.checked)} />
                  Trazer as vias do OpenStreetMap por cima
                </label>
              )}
              <div className={styles.linhaEntre}>
                <span />
                <span className={styles.mono}>{linhaFolha}</span>
              </div>
            </div>
          )}

          {modo === "drone" && (
            <div className={styles.bloco}>
              <span className={styles.rotulo}>FOTO</span>
              <DroneAjuste estado={drone} onChange={setDrone} busy={busy} partes="controles" />
              <span className={styles.mono}>{linhaFolha}</span>
            </div>
          )}

          <div className={styles.espaco} />
          {erro && <p className={styles.erro}>{erro}</p>}
          {progresso && <p className={styles.progresso}>{progresso}</p>}
          <div className={styles.rodape}>
            <button type="button" className={styles.btn} onClick={cancelar} disabled={busy}>
              Cancelar
            </button>
            <button type="button" className={styles.btnPrimario} onClick={() => void criar()} disabled={!podeCriar}>
              {busy ? "Criando…" : "Criar croqui"} <ArrowRight size={16} aria-hidden />
            </button>
          </div>
        </section>

        <section className={styles.previa} aria-label="Prévia">
          <div className={styles.previaTopo}>
            <span className={styles.rotulo}>PRÉVIA</span>
            <span className={styles.dica}>
              {modo === "branco" && "centro de Macapá (OpenStreetMap) · grade de 1 m"}
              {modo === "mapa" && "clique no mapa para mover o centro · o quadrado é a folha"}
              {modo === "drone" && "a folha é o recorte da foto"}
            </span>
          </div>
          <div className={styles.previaCaixa}>
            {modo === "branco" && <PreviaCena wM={wM} hM={hM} />}
            {modo === "mapa" && (
              <MapaBoundary>
                <div className={styles.mapa}>
                  <OsmMapPanel centre={centro} radius={raio} dataset={null} selectedIds={new Set()} onMapPick={setCentro} onToggleWay={() => undefined} onUnload={() => undefined} enxuto quadrado semCirculo mapaBase={fonte === FONTE_VIAS ? null : mapaBasePorId(fonte) ?? null} />
                </div>
              </MapaBoundary>
            )}
            {modo === "drone" && <PreviaDrone estado={drone} onChange={setDrone} busy={busy} />}
          </div>
        </section>
      </div>
    </div>
  );
}

function ModoCard({ ativo, onClick, icone, titulo, sub }: { ativo: boolean; onClick: () => void; icone: ReactNode; titulo: string; sub: string }) {
  return (
    <label className={`${styles.modo} ${ativo ? styles.modoAtivo : ""}`}>
      <input type="radio" name="modo" checked={ativo} onChange={onClick} />
      <span className={styles.modoIcone}>{icone}</span>
      <span className={styles.modoTexto}>
        <span className={styles.modoTitulo}>{titulo}</span>
        <span className={styles.modoSub}>{sub}</span>
      </span>
    </label>
  );
}

/** Mede o elemento; a prévia enche o que tiver. */
function useTamanho<T extends HTMLElement>(): [React.RefObject<T>, { w: number; h: number }] {
  const ref = useRef<T>(null) as React.RefObject<T>;
  const [box, setBox] = useState({ w: 600, h: 600 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => {
      if (e) setBox({ w: Math.max(1, e.contentRect.width), h: Math.max(1, e.contentRect.height) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, box];
}

const CENA = cenaMacapa as { lado_m: number; centro: { x: number; y: number }; roads: SicroParityObject[]; roundabouts: SicroParityObject[] };
const CENA_OBJETOS: SicroParityObject[] = [...CENA.roads, ...CENA.roundabouts];

/** Folha na proporção da cena: uma janela de W × H m sobre as vias reais do centro de Macapá, desenhadas pelo motor de vias. */
function PreviaCena({ wM, hM }: { wM: number; hM: number }) {
  const [ref, box] = useTamanho<HTMLDivElement>();
  const m = 40;
  const s = Math.max(0.5, Math.min(((box.w - 2 * m) * PREVIA_OCUPACAO) / wM, ((box.h - 2 * m) * PREVIA_OCUPACAO) / hM));
  const W = wM * s;
  const H = hM * s;
  const ox = Math.round((box.w - W) / 2);
  const oy = Math.round((box.h - H) / 2);
  // A janela fica sobre o cruzamento mais central da cena; o que cai fora da folha é recortado.
  const offX = ox - (CENA.centro.x - wM / 2) * s;
  const offY = oy - (CENA.centro.y - hM / 2) * s;
  const passo = s >= 4 ? 1 : s >= 1.5 ? 5 : 10;
  const grade: number[][] = [];
  for (let x = 0; x <= wM + 1e-6; x += passo) grade.push([ox + x * s, oy, ox + x * s, oy + H]);
  for (let y = 0; y <= hM + 1e-6; y += passo) grade.push([ox, oy + y * s, ox + W, oy + y * s]);
  return (
    <div ref={ref} className={styles.previaStage}>
      <Stage width={box.w} height={box.h} listening={false}>
        <Layer listening={false}>
          <Rect x={ox} y={oy} width={W} height={H} fill="#f5f6f8" />
          {grade.map((pts, i) => (
            <Line key={i} points={pts} stroke={(i * passo) % 10 === 0 ? "#b8c0cc" : "#dde3ea"} strokeWidth={1} />
          ))}
          <Group clipX={ox} clipY={oy} clipWidth={W} clipHeight={H}>
            <RoadParityRenderer objects={CENA_OBJETOS} pxPerM={s} offsetX={offX} offsetY={offY} />
          </Group>
          <Rect x={ox} y={oy} width={W} height={H} stroke="#d7a84f" strokeWidth={2} />
          <Line points={[ox, oy - 14, ox, oy - 22, ox, oy - 18, ox + W, oy - 18, ox + W, oy - 22, ox + W, oy - 14]} stroke="#a9b6c7" strokeWidth={1} />
          <Text x={ox} y={oy - 36} width={W} align="center" text={`${wM} m`} fontSize={12} fontFamily="monospace" fill="#a9b6c7" />
          <Line points={[ox - 14, oy, ox - 22, oy, ox - 18, oy, ox - 18, oy + H, ox - 22, oy + H, ox - 14, oy + H]} stroke="#a9b6c7" strokeWidth={1} />
          <Text x={ox - 34} y={oy + H / 2} text={`${hM} m`} fontSize={12} fontFamily="monospace" fill="#a9b6c7" rotation={-90} offsetX={18} />
        </Layer>
      </Stage>
    </div>
  );
}

/** Prévia da foto do drone na área grande, com o recorte pelas bordas; também recebe a foto arrastada para cá. */
function PreviaDrone({ estado, onChange, busy }: { estado: DroneAjusteEstado; onChange: (e: DroneAjusteEstado) => void; busy: boolean }) {
  const [ref, box] = useTamanho<HTMLDivElement>();
  const [arrastando, setArrastando] = useState(false);
  const [erro, setErro] = useState<string | null>(null);
  const estadoRef = useRef(estado);
  estadoRef.current = estado;
  useEffect(() => {
    let solta: (() => void) | null = null;
    let vivo = true;
    void getCurrentWebview()
      .onDragDropEvent((ev) => {
        if (ev.payload.type === "over" || ev.payload.type === "enter") setArrastando(true);
        else if (ev.payload.type === "leave") setArrastando(false);
        else if (ev.payload.type === "drop") {
          setArrastando(false);
          const foto = ev.payload.paths.find((p) => EXTENSOES_DE_FOTO.test(p));
          if (!foto) {
            setErro("Solte uma imagem (jpg, png, webp, tif).");
            return;
          }
          setErro(null);
          void carregarImagemDrone(foto)
            .then((r) => onChange({ ...estadoRef.current, ...r }))
            .catch((e: Error) => setErro(`Falha ao abrir imagem: ${e.message}`));
        }
      })
      .then((fn) => {
        if (vivo) solta = fn;
        else fn();
      })
      .catch(() => undefined);
    return () => {
      vivo = false;
      solta?.();
    };
  }, [onChange]);
  const l = estado.loaded;
  const fit = l ? Math.min((box.w - 16) / l.width, (box.h - 16) / l.height, 1) : 1;
  const previewMax = l ? Math.max(l.width, l.height) * fit : 600;
  return (
    <div ref={ref} className={`${styles.previaStage} ${arrastando ? styles.previaArrastando : ""}`}>
      {l ? (
        <div className={styles.previaDroneCentro}>
          <DroneAjuste estado={estado} onChange={onChange} busy={busy} partes="previa" previewMax={previewMax} />
        </div>
      ) : (
        <div className={styles.previaDrone}>
          <FileImage size={28} aria-hidden />
          <span>{arrastando ? "solte a foto" : "escolha a foto ao lado ou arraste para cá"}</span>
          {erro && <span className={styles.erro}>{erro}</span>}
        </div>
      )}
    </div>
  );
}

function formatarLatLon(p: LatLon): string {
  const f = (v: number, pos: string, neg: string) => {
    const a = Math.abs(v);
    const g = Math.floor(a);
    const mi = Math.floor((a - g) * 60);
    const se = ((a - g) * 60 - mi) * 60;
    return `${g}°${String(mi).padStart(2, "0")}'${se.toFixed(1).padStart(4, "0")}"${v < 0 ? neg : pos}`;
  };
  return `${f(p.lat, "N", "S")} ${f(p.lon, "E", "W")}`;
}

/** "0 deg 2' 21.00\" S" (exiftool) ou "-0.0391" → graus decimais. */
function lerGrau(v: string | undefined): number | null {
  if (!v) return null;
  const dec = Number(v);
  if (Number.isFinite(dec)) return dec;
  const m = v.match(/(\d+(?:\.\d+)?)\s*deg\s*(\d+(?:\.\d+)?)'\s*(\d+(?:\.\d+)?)"?\s*([NSEW])/i);
  if (!m) return null;
  const g = Number(m[1]) + Number(m[2]) / 60 + Number(m[3]) / 3600;
  return /[SW]/i.test(m[4]!) ? -g : g;
}
