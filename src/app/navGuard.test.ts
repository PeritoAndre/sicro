/**
 * Global navigation guard — unit tests (MVP 9 Round 3).
 */

import { describe, expect, it, beforeEach } from "vitest";
import { useNavGuard } from "./navGuard";

describe("navGuard", () => {
  beforeEach(() => {
    useNavGuard.getState().unregister();
  });

  it("runs the target immediately when no guard is registered", async () => {
    let ran = false;
    await useNavGuard.getState().attemptNavigation(() => {
      ran = true;
    });
    expect(ran).toBe(true);
  });

  it("invokes the guard before running the target", async () => {
    let guardCalls = 0;
    useNavGuard.getState().register(async () => {
      guardCalls += 1;
      return true;
    });
    let ran = false;
    await useNavGuard.getState().attemptNavigation(() => {
      ran = true;
    });
    expect(guardCalls).toBe(1);
    expect(ran).toBe(true);
  });

  it("skips the target when the guard returns false", async () => {
    useNavGuard.getState().register(async () => false);
    let ran = false;
    await useNavGuard.getState().attemptNavigation(() => {
      ran = true;
    });
    expect(ran).toBe(false);
  });

  it("register replaces any previously registered guard", async () => {
    const calls: string[] = [];
    useNavGuard.getState().register(async () => {
      calls.push("first");
      return true;
    });
    useNavGuard.getState().register(async () => {
      calls.push("second");
      return true;
    });
    await useNavGuard.getState().attemptNavigation(() => {});
    expect(calls).toEqual(["second"]);
  });

  it("unregister removes the active guard", async () => {
    useNavGuard.getState().register(async () => false);
    useNavGuard.getState().unregister();
    let ran = false;
    await useNavGuard.getState().attemptNavigation(() => {
      ran = true;
    });
    expect(ran).toBe(true);
  });
});
