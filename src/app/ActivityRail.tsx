/** Barra lateral de navegação: módulos, card do perito e rodapé com a versão. */

import { NavLink, useLocation, useNavigate, type NavigateFunction } from "react-router-dom";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  ArrowRight,
  Clapperboard,
  HelpCircle,
  Home as HomeIcon,
  ImagePlus,
  Map as MapIcon,
  Settings,
  User,
  type LucideIcon,
} from "lucide-react";
import { useSettingsStore } from "@stores/settingsStore";
import styles from "./ActivityRail.module.css";
import { useNavGuard } from "./navGuard";
import { isMidiaPath, lastMidiaAba } from "@modules/midia/midiaNav";
import { version as APP_VERSION } from "../../package.json";

interface RailItem {
  to: string;
  label: string;
  icon: LucideIcon;
  /** Ativo também nestes caminhos (Vídeo e Áudio tem duas telas). */
  activeWhen?: (pathname: string) => boolean;
}

const primary: RailItem = { to: "/", label: "Início", icon: HomeIcon };

const modules: RailItem[] = [
  { to: "/croqui", label: "Croquis", icon: MapIcon },
  { to: "/midia", label: "Vídeo e Áudio", icon: Clapperboard, activeWhen: isMidiaPath },
  { to: "/imagem", label: "Imagens", icon: ImagePlus },
];

const settingsItem: RailItem = {
  to: "/configuracoes",
  label: "Configurações",
  icon: Settings,
};

const helpItem: RailItem = { to: "/ajuda", label: "Ajuda", icon: HelpCircle };

/** Navegação que respeita o guard de alterações não salvas. */
function guardedGo(navigate: NavigateFunction, to: string): void {
  const guard = useNavGuard.getState().guard;
  if (!guard) {
    navigate(to);
    return;
  }
  void useNavGuard.getState().attemptNavigation(() => navigate(to));
}

export function ActivityRail() {
  const navigate = useNavigate();
  const profile = useSettingsStore((s) => s.settings.profile);
  const name = profile.full_name.trim();
  const role = profile.role.trim();
  const initials = name
    ? name
        .split(/\s+/)
        .filter(Boolean)
        .slice(0, 2)
        .map((w) => w.charAt(0).toUpperCase())
        .join("")
    : "";
  // Avatar: foto → iniciais → ícone genérico.
  const photoPath = profile.photo_path?.trim() ?? "";
  const photoSrc = photoPath ? convertFileSrc(photoPath) : null;

  return (
    <nav className={styles.rail} aria-label="Navegação principal">
      <div className={styles.nav}>
        <RailLink {...primary} />
        <div className={styles.navLabel}>Módulos</div>
        {modules.map((item) => (
          <RailLink key={item.to} {...item} />
        ))}
        <div className={styles.navSep} aria-hidden />
        <RailLink {...settingsItem} />
        <RailLink {...helpItem} />
      </div>

      <div className={styles.spacer} aria-hidden />

      <button
        type="button"
        className={styles.profile}
        onClick={() => guardedGo(navigate, "/configuracoes")}
        title="Perfil e configurações"
      >
        <span className={styles.avatar}>
          {photoSrc ? (
            <img src={photoSrc} alt="" className={styles.avatarImg} />
          ) : (
            initials || <User size={15} aria-hidden />
          )}
        </span>
        <span className={styles.profileText}>
          <span className={styles.profileName}>
            {name || "Configurar perfil"}
          </span>
          <span className={styles.profileRole}>
            {role || "toque para preencher"}
          </span>
        </span>
      </button>

      <div className={styles.footer}>
        <span className={styles.modeChip} title="Aplicação 100% local, sem nuvem">
          <span className={styles.modeDot} aria-hidden /> Local · Offline
        </span>
        <span className={styles.version}>v{APP_VERSION}</span>
      </div>
    </nav>
  );
}

function RailLink({ to, label, icon: Icon, activeWhen }: RailItem) {
  const navigate = useNavigate();
  const { pathname } = useLocation();
  // /midia aponta direto para a aba usada por último.
  const target = to === "/midia" ? lastMidiaAba() : to;
  return (
    <NavLink
      to={target}
      end={to === "/"}
      aria-label={label}
      className={({ isActive }) =>
        [styles.item, isActive || activeWhen?.(pathname) ? styles.itemActive : null]
          .filter(Boolean)
          .join(" ")
      }
      onClick={(e) => {
        const guard = useNavGuard.getState().guard;
        if (!guard) return; // deixa o NavLink navegar normalmente
        e.preventDefault();
        void useNavGuard.getState().attemptNavigation(() => navigate(target));
      }}
    >
      <Icon size={18} aria-hidden className={styles.itemIcon} />
      <span className={styles.itemLabel}>{label}</span>
      <ArrowRight size={15} aria-hidden className={styles.itemArrow} />
    </NavLink>
  );
}

