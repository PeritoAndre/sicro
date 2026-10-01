/**
 * ConfiguracoesModule — Configurações GLOBAIS do app (o "cofrinho" fora do
 * `.sicro`): Perfil do perito, Instituição/marca, Aparência (tema + cor + zoom),
 * Caminhos padrão, Backup geral, IA de degravação, Atalhos e Diagnóstico.
 *
 * Persistência: `settingsStore` (→ `app-settings.json` no app_config_dir).
 * A aparência aplica e salva na hora; os campos de texto salvam no botão
 * "Salvar".
 */

import { useEffect, useMemo, useState } from "react";
import {
  Archive,
  Building2,
  Cpu,
  FolderCog,
  Info,
  Keyboard,
  Palette,
  Save,
  Settings,
  User,
} from "lucide-react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { useSettingsStore } from "@stores/settingsStore";
import { clampUiZoom } from "@core/uiZoom";
import { MUNICIPIOS_AP } from "@domain/pericia";
import { AiManagerCard } from "./AiManagerCard";
import { GlobalBackupCard } from "./GlobalBackupCard";
import type {
  AppSettings,
  AppearanceSettings,
  InstitutionSettings,
  PathsSettings,
  PeritoProfile,
  ThemeMode,
} from "@domain/app_settings";
import { ShortcutsEditor } from "./ShortcutsEditor";
import styles from "./ConfiguracoesModule.module.css";

const ACCENTS = [
  { hex: "#d7a84f", name: "Dourado (padrão)" },
  { hex: "#5aa9e6", name: "Azul" },
  { hex: "#35c47a", name: "Verde" },
  { hex: "#9b8cff", name: "Roxo" },
  { hex: "#e6804f", name: "Âmbar" },
];

const THEMES: { value: ThemeMode; label: string }[] = [
  { value: "dark", label: "Escuro" },
  { value: "light", label: "Claro" },
  { value: "auto", label: "Automático" },
];

/** Tamanhos oferecidos na tela; os atalhos Ctrl+Shift+= / - andam por mais degraus. */
const UI_ZOOMS = [0.9, 1, 1.1, 1.25, 1.5, 1.75];

function zoomLabel(z: number): string {
  return `${Math.round(z * 100)}%`;
}

function normalizeHex(s: string): string {
  return /^#[0-9a-fA-F]{6}$/.test(s.trim()) ? s.trim() : "#d7a84f";
}

interface FieldProps {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  wide?: boolean;
  type?: string;
  /** Se presente, vira um dropdown (select) com estas opções. */
  options?: readonly string[];
  /** Se true, mostra um botão "Escolher…" que abre o seletor de imagem nativo. */
  pickFile?: boolean;
}

async function pickImagePath(): Promise<string | null> {
  const picked = await openFileDialog({
    multiple: false,
    filters: [
      { name: "Imagens", extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif"] },
    ],
  });
  return typeof picked === "string" ? picked : null;
}

function Field({
  label,
  value,
  onChange,
  placeholder,
  wide,
  type = "text",
  options,
  pickFile,
}: FieldProps) {
  return (
    <div className={`${styles.field} ${wide ? styles.fieldWide : ""}`}>
      <label className={styles.label}>{label}</label>
      {options ? (
        <select
          className={styles.input}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">{placeholder ?? "Selecione…"}</option>
          {value && !options.includes(value) && (
            <option value={value}>{value}</option>
          )}
          {options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      ) : pickFile ? (
        <div className={styles.fileRow}>
          <input
            className={styles.input}
            type="text"
            value={value}
            placeholder={placeholder}
            onChange={(e) => onChange(e.target.value)}
          />
          <button
            type="button"
            className={styles.pickBtn}
            onClick={() => {
              void pickImagePath().then((p) => {
                if (p) onChange(p);
              });
            }}
          >
            Escolher…
          </button>
        </div>
      ) : (
        <input
          className={styles.input}
          type={type}
          value={value}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </div>
  );
}

/** Categorias do "Settings Center" — nav interna, sem scroll único. */
type CatId =
  | "perfil"
  | "instituicao"
  | "aparencia"
  | "caminhos"
  | "backup"
  | "iaocr"
  | "atalhos"
  | "diagnostico";

const CATS: { id: CatId; label: string; sub: string; Icon: typeof User }[] = [
  { id: "perfil", label: "Perfil", sub: "Dados pessoais e profissionais", Icon: User },
  { id: "instituicao", label: "Instituição & marca", sub: "Cabeçalho, unidade e brasões", Icon: Building2 },
  { id: "aparencia", label: "Aparência", sub: "Tema, cores e personalização", Icon: Palette },
  { id: "caminhos", label: "Caminhos padrão", sub: "Pastas e diretórios", Icon: FolderCog },
  { id: "backup", label: "Backup geral", sub: "Cópia de todos os casos", Icon: Archive },
  { id: "iaocr", label: "IA (degravação)", sub: "Transcrição local de áudio", Icon: Cpu },
  { id: "atalhos", label: "Atalhos de teclado", sub: "Customizáveis por ação", Icon: Keyboard },
  { id: "diagnostico", label: "Diagnóstico", sub: "Onde os dados ficam", Icon: Info },
];

export function ConfiguracoesModule() {
  const settings = useSettingsStore((s) => s.settings);
  const loaded = useSettingsStore((s) => s.loaded);
  const persist = useSettingsStore((s) => s.persist);

  const [draft, setDraft] = useState<AppSettings>(settings);
  const [activeCat, setActiveCat] = useState<CatId>("perfil");
  const [saving, setSaving] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [cfgPath, setCfgPath] = useState("");

  useEffect(() => {
    if (!loaded) void useSettingsStore.getState().load();
  }, [loaded]);

  useEffect(() => {
    setDraft(settings);
  }, [settings]);

  useEffect(() => {
    void commands.getSettingsFilePath().then(setCfgPath).catch(() => {});
  }, []);

  const dirty = useMemo(
    () => JSON.stringify(draft) !== JSON.stringify(settings),
    [draft, settings],
  );

  const setProfile = (p: Partial<PeritoProfile>) =>
    setDraft((d) => ({ ...d, profile: { ...d.profile, ...p } }));
  const setInstitution = (p: Partial<InstitutionSettings>) =>
    setDraft((d) => ({ ...d, institution: { ...d.institution, ...p } }));
  const setPaths = (p: Partial<PathsSettings>) =>
    setDraft((d) => ({ ...d, paths: { ...d.paths, ...p } }));

  const doPersist = async (next: AppSettings) => {
    setSaving(true);
    setError(null);
    try {
      await persist(next);
      setFeedback("Configurações salvas.");
      setTimeout(() => setFeedback(null), 2500);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setSaving(false);
    }
  };

  // Aparência é preferência instantânea: aplica e salva na hora.
  const setAppearance = (p: Partial<AppearanceSettings>) => {
    const next: AppSettings = {
      ...draft,
      appearance: { ...draft.appearance, ...p },
    };
    setDraft(next);
    void doPersist(next);
  };

  return (
    <div className={styles.wrap}>
      <header className={styles.topBar}>
        <div className={styles.title}>
          <h1>
            <Settings size={16} aria-hidden /> Configurações
          </h1>
          <p className={styles.subtitle}>
            Preferências do app e do perito — valem em todas as ocorrências.
          </p>
        </div>
        <div className={styles.headActions}>
          {feedback && <span className={styles.feedback}>{feedback}</span>}
          {error && <span className={styles.errorMsg}>{error}</span>}
          {dirty && <span className={styles.dirtyTag}>alterações não salvas</span>}
          <Button
            variant="primary"
            leftIcon={<Save size={14} />}
            onClick={() => void doPersist(draft)}
            disabled={!dirty || saving}
          >
            {saving ? "Salvando…" : "Salvar"}
          </Button>
        </div>
      </header>

      <div className={styles.body}>
        <nav className={styles.catNav} aria-label="Categorias de configuração">
          {CATS.map((c) => {
            const Icon = c.Icon;
            return (
              <button
                key={c.id}
                type="button"
                className={styles.catItem}
                data-active={activeCat === c.id}
                onClick={() => setActiveCat(c.id)}
              >
                <Icon size={16} aria-hidden />
                <span className={styles.catItemText}>
                  <span className={styles.catItemLabel}>{c.label}</span>
                  <span className={styles.catItemSub}>{c.sub}</span>
                </span>
              </button>
            );
          })}
        </nav>

        <div className={styles.content}>
          {/* Backup geral (todos os casos) */}
          {activeCat === "backup" && <GlobalBackupCard />}

          {/* Perfil do perito */}
          {activeCat === "perfil" && (
        <section className={styles.card}>
          <div className={styles.cardHead}>
            <User size={15} aria-hidden />
            <h2 className={styles.cardTitle}>Perfil</h2>
          </div>
          <p className={styles.cardDesc}>
            Identifica a autoria e servirá para pré-preencher laudos e medições.
            <span className={styles.soon}>pré-preenche laudos em breve</span>
          </p>
          <div className={styles.grid}>
            <Field
              label="Nome completo"
              value={draft.profile.full_name}
              onChange={(v) => setProfile({ full_name: v })}
              placeholder="André Ricardo Barroso"
            />
            <Field
              label="Matrícula"
              value={draft.profile.registration}
              onChange={(v) => setProfile({ registration: v })}
            />
            <Field
              label="Cargo"
              value={draft.profile.role}
              onChange={(v) => setProfile({ role: v })}
              placeholder="Perito Criminal"
            />
            <Field
              label="Formação"
              value={draft.profile.formation}
              onChange={(v) => setProfile({ formation: v })}
            />
            <Field
              label="Município de atuação"
              value={draft.profile.municipio_atuacao}
              onChange={(v) => setProfile({ municipio_atuacao: v })}
              options={MUNICIPIOS_AP}
              placeholder="Selecione…"
            />
            <Field
              wide
              pickFile
              label="Imagem de assinatura"
              value={draft.profile.signature_image_path}
              onChange={(v) => setProfile({ signature_image_path: v })}
              placeholder="Nenhum arquivo selecionado"
            />
            <Field
              wide
              pickFile
              label="Foto do perito"
              value={draft.profile.photo_path}
              onChange={(v) => setProfile({ photo_path: v })}
              placeholder="Nenhum arquivo selecionado"
            />
          </div>
        </section>
          )}

          {/* Instituição & marca */}
          {activeCat === "instituicao" && (
        <section className={styles.card}>
          <div className={styles.cardHead}>
            <Building2 size={15} aria-hidden />
            <h2 className={styles.cardTitle}>Instituição & marca</h2>
          </div>
          <p className={styles.cardDesc}>
            Padrão institucional que alimentará o cabeçalho dos laudos.
            <span className={styles.soon}>alimenta o cabeçalho em breve</span>
          </p>
          <div className={styles.grid}>
            <Field
              label="Órgão"
              value={draft.institution.organization}
              onChange={(v) => setInstitution({ organization: v })}
              placeholder="Polícia Científica do Amapá"
            />
            <Field
              label="Unidade / setor"
              value={draft.institution.unit}
              onChange={(v) => setInstitution({ unit: v })}
            />
            <Field
              wide
              label="Endereço"
              value={draft.institution.address}
              onChange={(v) => setInstitution({ address: v })}
            />
            <Field
              wide
              label="Texto de rodapé"
              value={draft.institution.footer_text}
              onChange={(v) => setInstitution({ footer_text: v })}
            />
            <Field
              pickFile
              label="Brasão esquerdo"
              value={draft.institution.brasao_left_path}
              onChange={(v) => setInstitution({ brasao_left_path: v })}
              placeholder="Nenhum arquivo selecionado"
            />
            <Field
              pickFile
              label="Brasão direito"
              value={draft.institution.brasao_right_path}
              onChange={(v) => setInstitution({ brasao_right_path: v })}
              placeholder="Nenhum arquivo selecionado"
            />
          </div>
        </section>
          )}

          {/* Aparência */}
          {activeCat === "aparencia" && (
        <section className={styles.card}>
          <div className={styles.cardHead}>
            <Palette size={15} aria-hidden />
            <h2 className={styles.cardTitle}>Aparência</h2>
          </div>
          <p className={styles.cardDesc}>
            Tema, cor de destaque e tamanho da interface. As mudanças valem na
            hora e são salvas automaticamente.
          </p>
          <div className={styles.grid}>
            <div className={styles.field}>
              <label className={styles.label}>Tema</label>
              <div className={styles.segmented} role="tablist">
                {THEMES.map((t) => (
                  <button
                    key={t.value}
                    type="button"
                    role="tab"
                    aria-selected={draft.appearance.theme === t.value}
                    className={`${styles.segBtn} ${
                      draft.appearance.theme === t.value ? styles.segBtnActive : ""
                    }`}
                    onClick={() => setAppearance({ theme: t.value })}
                  >
                    {t.label}
                  </button>
                ))}
              </div>
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Cor de destaque</label>
              <div className={styles.accentRow}>
                {ACCENTS.map((a) => (
                  <button
                    key={a.hex}
                    type="button"
                    title={a.name}
                    aria-label={a.name}
                    className={`${styles.swatch} ${
                      draft.appearance.accent.toLowerCase() === a.hex.toLowerCase()
                        ? styles.swatchActive
                        : ""
                    }`}
                    style={{ background: a.hex }}
                    onClick={() => setAppearance({ accent: a.hex })}
                  />
                ))}
                <input
                  type="color"
                  className={styles.customColor}
                  value={normalizeHex(draft.appearance.accent)}
                  onChange={(e) => setAppearance({ accent: e.target.value })}
                  title="Cor personalizada"
                  aria-label="Cor personalizada"
                />
              </div>
            </div>
            <div className={styles.field}>
              <label className={styles.label}>
                Tamanho da interface — {zoomLabel(clampUiZoom(draft.appearance.ui_zoom))}
              </label>
              <div className={styles.segmented} role="tablist">
                {UI_ZOOMS.map((z) => {
                  const active =
                    Math.abs(clampUiZoom(draft.appearance.ui_zoom) - z) < 0.001;
                  return (
                    <button
                      key={z}
                      type="button"
                      role="tab"
                      aria-selected={active}
                      className={`${styles.segBtn} ${active ? styles.segBtnActive : ""}`}
                      onClick={() => setAppearance({ ui_zoom: z })}
                    >
                      {zoomLabel(z)}
                    </button>
                  );
                })}
              </div>
              <span className={styles.hint}>
                Em qualquer tela: Ctrl + Shift + = aumenta, Ctrl + Shift + −
                diminui, Ctrl + Shift + 0 volta a 100%.
              </span>
            </div>
          </div>
        </section>
          )}

          {/* Caminhos padrão */}
          {activeCat === "caminhos" && (
        <section className={styles.card}>
          <div className={styles.cardHead}>
            <FolderCog size={15} aria-hidden />
            <h2 className={styles.cardTitle}>Caminhos padrão</h2>
          </div>
          <p className={styles.cardDesc}>
            Pastas sugeridas ao criar ocorrências e ao exportar.
            <span className={styles.soon}>entra em vigor em breve</span>
          </p>
          <div className={styles.grid}>
            <Field
              wide
              label="Pasta padrão de workspaces (.sicro)"
              value={draft.paths.default_workspace_dir}
              onChange={(v) => setPaths({ default_workspace_dir: v })}
              placeholder="C:\\SICRO\\Ocorrências"
            />
            <Field
              wide
              label="Pasta padrão de exportação"
              value={draft.paths.default_export_dir}
              onChange={(v) => setPaths({ default_export_dir: v })}
              placeholder="C:\\SICRO\\Exportações"
            />
          </div>
        </section>
          )}

          {/* IA de degravação (whisper.cpp local, usado pelos Áudios) */}
          {activeCat === "iaocr" && <AiManagerCard />}

          {/* Atalhos de teclado */}
          {activeCat === "atalhos" && (
        <section className={styles.card}>
          <div className={styles.cardHead}>
            <Keyboard size={15} aria-hidden />
            <h2 className={styles.cardTitle}>Atalhos de teclado</h2>
          </div>
          <p className={styles.cardDesc}>
            <strong>Todos os atalhos são customizáveis</strong>: clique numa
            tecla e pressione a combinação que preferir (Esc cancela). Estão
            organizados na <strong>ordem dos módulos</strong> — Dossiê, Laudo,
            Croqui, Vídeo, Áudio, Imagem e Documentoscopia. Cada atalho vale na
            tela do seu módulo, então a mesma tecla pode se repetir entre módulos
            sem conflito (eles nunca estão ativos ao mesmo tempo).
          </p>

          <ShortcutsEditor />
        </section>
          )}

          {/* Diagnóstico */}
          {activeCat === "diagnostico" && (
        <section className={styles.card}>
          <div className={styles.cardHead}>
            <Info size={15} aria-hidden />
            <h2 className={styles.cardTitle}>Diagnóstico</h2>
          </div>
          <p className={styles.cardDesc}>
            Onde suas configurações ficam guardadas neste computador (o
            "cofrinho" global, fora de qualquer ocorrência).
          </p>
          {cfgPath ? (
            <div className={styles.mono}>{cfgPath}</div>
          ) : (
            <p className={styles.note}>—</p>
          )}
          <p className={styles.note}>
            Em breve nesta seção: backup automático do trabalho e verificação de
            dependências (FFmpeg).
          </p>
        </section>
          )}
        </div>
      </div>
    </div>
  );
}
