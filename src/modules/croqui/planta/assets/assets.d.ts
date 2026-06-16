// Permite importar SVGs como URL (Vite) com `?url`, p/ texturas Pixi.
declare module "*.svg?url" {
  const url: string;
  export default url;
}
