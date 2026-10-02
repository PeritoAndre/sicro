/** Marca SICRO na barra de título; a série vem do package.json. */

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
