/**
 * Branding asset loader.
 *
 * The editor can reference `/branding/*.png` directly (Vite serves the
 * `public/` folder at the root, and Tauri's WebView2 picks the assets up
 * automatically). The HTML/PDF export pipeline can't: the Edge headless
 * process reads the HTML from a temp file in `<workspace>/cache/` and has
 * no way to resolve a path relative to the SICRO app bundle.
 *
 * To make both pipelines work with the same `<img>` markup, this module
 * pre-loads each branding asset, converts it to a base-64 data URI and
 * caches the result. Render-time consumers ask for the data URIs via
 * `getBrandingAssets()`; if the cache is cold they kick off a fetch and
 * fall back to the on-disk path while the load is in flight.
 */

export interface BrandingAssets {
  /** Data URI for the State of Amapá coat of arms. */
  estado: string;
  /** Data URI for the Polícia Científica coat of arms. */
  pca: string;
}

const PATHS = {
  estado: "/branding/brasao-amapa.png",
  pca: "/branding/brasao-pca.png",
} as const;

let cache: BrandingAssets | null = null;
let loading: Promise<BrandingAssets> | null = null;

/**
 * Resolve both branding assets as data URIs. Cached after the first call.
 * Returns empty strings if the fetch fails — callers must guard with
 * `if (assets.estado) { … }` to avoid emitting broken <img>.
 */
export function loadBrandingAssets(): Promise<BrandingAssets> {
  if (cache) return Promise.resolve(cache);
  if (loading) return loading;

  loading = (async () => {
    const [estado, pca] = await Promise.all([
      fetchAsDataUri(PATHS.estado),
      fetchAsDataUri(PATHS.pca),
    ]);
    cache = { estado, pca };
    loading = null;
    return cache;
  })();

  return loading;
}

/** Returns the cached assets synchronously, or `null` if not yet loaded. */
export function getCachedBrandingAssets(): BrandingAssets | null {
  return cache;
}

/** Forces a re-fetch — useful after the user swaps the files manually. */
export function invalidateBrandingCache(): void {
  cache = null;
  loading = null;
}

/** Returns the direct `/branding/...` paths (used by the editor's <img>). */
export function brandingPaths(): typeof PATHS {
  return PATHS;
}

async function fetchAsDataUri(path: string): Promise<string> {
  try {
    const res = await fetch(path);
    if (!res.ok) return "";
    const blob = await res.blob();
    return await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result ?? ""));
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(blob);
    });
  } catch {
    return "";
  }
}
