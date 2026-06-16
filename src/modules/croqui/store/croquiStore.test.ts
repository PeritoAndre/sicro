/**
 * croquiStore — unit tests for the dirty/export tracking added in
 * MVP 9 Round 3 (Croqui Avançado corrections).
 *
 * The store talks to the Rust backend via `@core/commands`, which is
 * not available under vitest/jsdom. So instead of exercising the
 * async loaders, these tests poke the store's setState directly and
 * verify the pure derivations (`isExportStale`).
 */

import { describe, expect, it, beforeEach } from "vitest";
import { useCroquiStore } from "./croquiStore";
import type { Croqui } from "@/types/croqui";

function makeCroqui(id: string, updated_at: string): Croqui {
  return {
    id,
    occurrence_id: "occ_1",
    title: `Croqui ${id}`,
    relative_path: `croquis/${id}.sicrocroqui`,
    status: "draft",
    schema_version: "0.3",
    last_export_relative_path: null,
    kind: "viario",
    created_at: updated_at,
    updated_at,
  };
}

describe("croquiStore.isExportStale", () => {
  beforeEach(() => {
    // Reset store to a known state before each test.
    useCroquiStore.setState({
      list: [],
      lastExportedAt: {},
      activeCroqui: null,
      activeDoc: null,
      activeCroquiId: null,
    });
  });

  it("returns true when no export has happened yet", () => {
    const croqui = makeCroqui("c1", "2026-05-01T10:00:00Z");
    useCroquiStore.setState({ list: [croqui] });
    expect(useCroquiStore.getState().isExportStale("c1")).toBe(true);
  });

  it("returns true when the croqui isn't in the list", () => {
    expect(useCroquiStore.getState().isExportStale("missing")).toBe(true);
  });

  it("returns false when the export timestamp is newer than the save", () => {
    const croqui = makeCroqui("c1", "2026-05-01T10:00:00Z");
    useCroquiStore.setState({
      list: [croqui],
      lastExportedAt: { c1: "2026-05-01T11:00:00Z" },
    });
    expect(useCroquiStore.getState().isExportStale("c1")).toBe(false);
  });

  it("returns true when the save is newer than the export", () => {
    const croqui = makeCroqui("c1", "2026-05-02T09:00:00Z");
    useCroquiStore.setState({
      list: [croqui],
      lastExportedAt: { c1: "2026-05-01T11:00:00Z" },
    });
    expect(useCroquiStore.getState().isExportStale("c1")).toBe(true);
  });

  it("treats equal timestamps as fresh (export covers the save)", () => {
    const t = "2026-05-01T10:00:00Z";
    const croqui = makeCroqui("c1", t);
    useCroquiStore.setState({
      list: [croqui],
      lastExportedAt: { c1: t },
    });
    expect(useCroquiStore.getState().isExportStale("c1")).toBe(false);
  });
});
