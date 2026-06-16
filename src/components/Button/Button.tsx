import type { ButtonHTMLAttributes, ReactNode } from "react";
import styles from "./Button.module.css";

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md" | "lg";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  leftIcon?: ReactNode;
  rightIcon?: ReactNode;
}

const variantClass: Record<Variant, string> = {
  primary: styles.primary!,
  secondary: styles.secondary!,
  ghost: styles.ghost!,
  danger: styles.danger!,
};

const sizeClass: Partial<Record<Size, string>> = {
  sm: styles.sm,
  lg: styles.lg,
};

export function Button({
  variant = "secondary",
  size = "md",
  leftIcon,
  rightIcon,
  children,
  className,
  type = "button",
  ...rest
}: ButtonProps) {
  const classes = [styles.button, variantClass[variant], sizeClass[size], className]
    .filter(Boolean)
    .join(" ");
  return (
    <button {...rest} type={type} className={classes}>
      {leftIcon}
      {children}
      {rightIcon}
    </button>
  );
}
