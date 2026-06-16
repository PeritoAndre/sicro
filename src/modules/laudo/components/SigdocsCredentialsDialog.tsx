/**
 * SigdocsCredentialsDialog — gerenciar email + senha do SIGDOC.
 *
 * K — Modal que permite ao perito cadastrar / atualizar / esquecer
 * suas credenciais do SIGDOC. As credenciais são armazenadas:
 *   - email: arquivo em `app_config_dir/sigdoc-email.txt`
 *   - senha: Windows Credential Manager (criptografado per-user via
 *            Win32 API, mesmo nível de proteção do Edge/Chrome).
 *
 * O dialog tem aviso de segurança claro — esta é a abordagem padrão
 * para gerenciadores de senha desktop (1Password, Bitwarden, etc.),
 * mas o perito precisa estar ciente.
 *
 * Após salvar, o autofill é executado automaticamente sempre que o
 * cover do SIGDOC abrir.
 */

import { useEffect, useState } from "react";
import {
  ShieldCheck,
  X,
  Loader2,
  Eye,
  EyeOff,
  Trash2,
  CheckCircle2,
  AlertTriangle,
  KeyRound,
} from "lucide-react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import styles from "./SigdocsCredentialsDialog.module.css";

interface Props {
  open: boolean;
  onClose: () => void;
  onSaved?: () => void;
}

export function SigdocsCredentialsDialog({ open, onClose, onSaved }: Props) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [showPwd, setShowPwd] = useState(false);
  const [hasExistingPassword, setHasExistingPassword] = useState(false);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  // K — Quando o dialog abre, carrega o email já cadastrado (se houver).
  useEffect(() => {
    if (!open) return;
    setError(null);
    setSuccess(null);
    setPassword("");
    setShowPwd(false);
    setLoading(true);
    void commands
      .getSigdocCredentialsStatus()
      .then((status) => {
        setEmail(status.email ?? "");
        setHasExistingPassword(status.has_password);
      })
      .catch((err) => setError(toSicroError(err).message))
      .finally(() => setLoading(false));
  }, [open]);

  useEffect(() => {
    if (!open) return undefined;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  const handleSave = async () => {
    if (!email.trim()) {
      setError("Informe o email do SIGDOC.");
      return;
    }
    if (!password.trim()) {
      setError("Informe a senha do SIGDOC.");
      return;
    }
    setSaving(true);
    setError(null);
    setSuccess(null);
    try {
      await commands.saveSigdocCredentials(email.trim(), password);
      setSuccess("Credenciais salvas com segurança.");
      setHasExistingPassword(true);
      setPassword("");
      onSaved?.();
      window.setTimeout(() => onClose(), 900);
    } catch (err) {
      setError(toSicroError(err).message);
    } finally {
      setSaving(false);
    }
  };

  const handleForget = async () => {
    if (
      !window.confirm(
        "Remover email e senha do SIGDOC deste computador? Você precisará informá-los novamente ao usar o autofill.",
      )
    ) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await commands.deleteSigdocCredentials();
      setEmail("");
      setPassword("");
      setHasExistingPassword(false);
      setSuccess("Credenciais removidas.");
      onSaved?.();
    } catch (err) {
      setError(toSicroError(err).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className={styles.overlay}
      role="dialog"
      aria-modal="true"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className={styles.dialog}>
        <header className={styles.head}>
          <strong>
            <KeyRound size={15} /> Credenciais do SIGDOC
          </strong>
          <button
            type="button"
            className={styles.closeBtn}
            onClick={onClose}
            aria-label="Fechar"
          >
            <X size={14} />
          </button>
        </header>

        <div className={styles.warning}>
          <ShieldCheck size={14} /> A senha fica guardada no{" "}
          <strong>Gerenciador de Credenciais do Windows</strong>{" "}
          (criptografado por usuário) — mesmo padrão do Edge e Chrome.
          O autofill apenas <em>preenche</em> o login do SIGDOC; você
          sempre clica "Entrar" manualmente.
        </div>

        {loading ? (
          <div className={styles.body}>
            <p className={styles.muted}>
              <Loader2 size={13} className={styles.spin} /> Carregando…
            </p>
          </div>
        ) : (
          <div className={styles.body}>
            <div className={styles.field}>
              <label htmlFor="sig-email">Email institucional</label>
              <input
                id="sig-email"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="seu.nome@policiacientifica.ap.gov.br"
                autoComplete="off"
                autoFocus
              />
            </div>

            <div className={styles.field}>
              <label htmlFor="sig-password">
                Senha
                {hasExistingPassword && (
                  <span className={styles.savedBadge}>
                    <CheckCircle2 size={10} /> uma senha já está salva
                  </span>
                )}
              </label>
              <div className={styles.pwdWrap}>
                <input
                  id="sig-password"
                  type={showPwd ? "text" : "password"}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder={
                    hasExistingPassword
                      ? "Digite uma nova senha (deixe vazio pra manter)…"
                      : "Sua senha do SIGDOC"
                  }
                  autoComplete="new-password"
                />
                <button
                  type="button"
                  className={styles.pwdToggle}
                  onClick={() => setShowPwd((v) => !v)}
                  aria-label={showPwd ? "Esconder senha" : "Mostrar senha"}
                  title={showPwd ? "Esconder senha" : "Mostrar senha"}
                >
                  {showPwd ? <EyeOff size={13} /> : <Eye size={13} />}
                </button>
              </div>
              <small className={styles.hint}>
                Caracteres comuns são suportados (incluindo acentos).
              </small>
            </div>

            {error && (
              <div className={styles.errorBox}>
                <AlertTriangle size={12} /> {error}
              </div>
            )}
            {success && (
              <div className={styles.successBox}>
                <CheckCircle2 size={12} /> {success}
              </div>
            )}
          </div>
        )}

        <footer className={styles.footer}>
          {hasExistingPassword && (
            <button
              type="button"
              className={styles.dangerBtn}
              onClick={() => void handleForget()}
              disabled={saving}
              title="Apagar email + senha deste computador"
            >
              <Trash2 size={12} /> Esquecer
            </button>
          )}
          <span style={{ flex: 1 }} />
          <button
            type="button"
            className={styles.secondaryBtn}
            onClick={onClose}
            disabled={saving}
          >
            Cancelar
          </button>
          <button
            type="button"
            className={styles.primaryBtn}
            onClick={() => void handleSave()}
            disabled={saving || !email.trim() || !password.trim()}
          >
            {saving ? (
              <>
                <Loader2 size={12} className={styles.spin} /> Salvando…
              </>
            ) : (
              <>
                <ShieldCheck size={12} /> Salvar
              </>
            )}
          </button>
        </footer>
      </div>
    </div>
  );
}
