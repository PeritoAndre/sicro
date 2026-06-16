/**
 * useDossieList — small hook every Dossiê tab uses to fetch its list.
 *
 * Just async-loads `fetcher(workspacePath)` and exposes
 * `{ items, loading, error, reload }`. Avoids re-implementing the same
 * try/catch in every tab.
 */

import { useCallback, useEffect, useState } from "react";
import { toSicroError } from "@core/errors";

export interface DossieListState<T> {
  items: T[];
  loading: boolean;
  error: string | null;
  reload: () => Promise<void>;
}

export function useDossieList<T>(
  workspacePath: string,
  fetcher: (workspacePath: string) => Promise<T[]>,
): DossieListState<T> {
  const [items, setItems] = useState<T[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await fetcher(workspacePath);
      setItems(result);
    } catch (err) {
      setError(toSicroError(err).message);
    } finally {
      setLoading(false);
    }
  }, [workspacePath, fetcher]);

  useEffect(() => {
    void load();
  }, [load]);

  return { items, loading, error, reload: load };
}
