import type { HTMLAttributes, KeyboardEvent } from "react";
import styles from "./Card.module.css";

interface CardProps extends HTMLAttributes<HTMLDivElement> {
  interactive?: boolean;
  density?: "compact" | "default" | "padded";
}

export function Card({
  interactive = false,
  density = "default",
  className,
  onClick,
  onKeyDown,
  children,
  ...rest
}: CardProps) {
  const classes = [
    styles.card,
    interactive ? styles.interactive : null,
    density === "compact" ? styles.compact : null,
    density === "padded" ? styles.padded : null,
    className,
  ]
    .filter(Boolean)
    .join(" ");

  const handleKey = (e: KeyboardEvent<HTMLDivElement>) => {
    onKeyDown?.(e);
    if (interactive && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      onClick?.(e as unknown as React.MouseEvent<HTMLDivElement>);
    }
  };

  return (
    <div
      {...rest}
      role={interactive ? "button" : rest.role}
      tabIndex={interactive ? 0 : rest.tabIndex}
      onClick={onClick}
      onKeyDown={handleKey}
      className={classes}
    >
      {children}
    </div>
  );
}
