/**
 * Brand — marca SICRO (logo + nome "SICRO 2.0 / Suíte Pericial").
 *
 * Vive na barra de título (app bar), canto superior esquerdo. Logo de
 * `public/branding/sicro-logo.png` com fallback gracioso pro escudo se o
 * arquivo faltar — nada quebra.
 */

import { useState } from "react";
import { Shield } from "lucide-react";
import styles from "./Brand.module.css";

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
          SICRO <b>2.0</b>
        </span>
        <span className={styles.tag}>Suíte Pericial</span>
      </span>
    </span>
  );
}
