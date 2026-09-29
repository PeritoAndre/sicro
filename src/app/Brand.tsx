/**
 * Brand — marca SICRO (logo + nome "SICRO 3.1 / Suíte Pericial").
 *
 * O número vem do package.json (maior.menor) — subir a versão já atualiza.
 *
 * Vive na barra de título (app bar), canto superior esquerdo. Logo de
 * `public/branding/sicro-logo.png` com fallback gracioso pro escudo se o
 * arquivo faltar — nada quebra.
 */

import { useState } from "react";
import { Shield } from "lucide-react";
import { version } from "../../package.json";
import styles from "./Brand.module.css";

/** "3.1.0" → "3.1" */
const SERIES = version.split(".").slice(0, 2).join(".");

export function Brand() {
  const [imageOk, setImageOk] = useState(true);
  return (
    <span className={styles.brand}>
      {imageOk ? (
        <img
          src="/branding/sicro-logo.png"
          alt="SICRO"
          className={styles.logo}
          draggable={false}
          width={30}
          height={30}
          onError={() => setImageOk(false)}
        />
      ) : (
        <span className={styles.logoFallback} aria-hidden>
          <Shield size={17} />
        </span>
      )}
      <span className={styles.text}>
        <span className={styles.name}>
          SICRO <b>{SERIES}</b>
        </span>
        <span className={styles.tag}>Suíte Pericial</span>
      </span>
    </span>
  );
}
