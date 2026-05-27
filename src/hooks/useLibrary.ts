import { useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLibraryStore } from "@/store/library";
import type { Game, LocalLibrary, SSD } from "@/types";

const HOTPLUG_INTERVAL_MS = 3000;
const DEFAULT_DRIVE_LETTER = "S";

export function useLibrary() {
  const setSsds = useLibraryStore((s) => s.setSsds);
  const upsertSsd = useLibraryStore((s) => s.upsertSsd);
  const setSsdConnected = useLibraryStore((s) => s.setSsdConnected);
  const setGames = useLibraryStore((s) => s.setGames);
  const mergeGames = useLibraryStore((s) => s.mergeGames);
  const setLocalLibraries = useLibraryStore((s) => s.setLocalLibraries);
  const ssds = useLibraryStore((s) => s.ssds);

  const loadInitial = useCallback(async () => {
    try {
      const ssd = await invoke<SSD>("scan_vault_ssd", {
        driveLetter: DEFAULT_DRIVE_LETTER,
      });
      upsertSsd(ssd);
      const games = await invoke<Game[]>("get_ssd_catalog", {
        driveLetter: DEFAULT_DRIVE_LETTER,
      });
      mergeGames(games);
    } catch {
      // SSD not connected — leave catalog empty
      setSsds([]);
      setGames([]);
    }

    try {
      const libs = await invoke<LocalLibrary[]>("scan_local_steam_libraries", {
        vaultDriveLetter: DEFAULT_DRIVE_LETTER,
      });
      setLocalLibraries(libs);
      await refreshInstalled(libs);
    } catch {
      setLocalLibraries([]);
    }
  }, [upsertSsd, mergeGames, setSsds, setGames, setLocalLibraries]);

  const refreshInstalled = useCallback(async (libs: LocalLibrary[]) => {
    const allGames = useLibraryStore.getState().games;
    if (allGames.length === 0) return;
    const folderNames = allGames.map((g) => g.folderName);
    const libraryPaths = libs.map((l) => l.path);
    try {
      const map = await invoke<Record<string, string>>("check_installed_games", {
        vaultGames: folderNames,
        localLibraries: libraryPaths,
      });
      const updated = allGames.map((g) => ({
        ...g,
        isInstalled: !!map[g.folderName],
        installedPath: map[g.folderName] ?? null,
      }));
      useLibraryStore.getState().setGames(updated);
    } catch {
      // ignore
    }
  }, []);

  const rescan = useCallback(async () => {
    try {
      const games = await invoke<Game[]>("rescan_ssd", {
        driveLetter: DEFAULT_DRIVE_LETTER,
      });
      setGames(games);
      const libs = useLibraryStore.getState().localLibraries;
      await refreshInstalled(libs);
    } catch (e) {
      console.error("Rescan failed:", e);
    }
  }, [setGames, refreshInstalled]);

  // Hotplug polling
  const lastConnectedRef = useRef<boolean | null>(null);
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const ssd = await invoke<SSD>("scan_vault_ssd", {
          driveLetter: DEFAULT_DRIVE_LETTER,
        });
        if (lastConnectedRef.current === false) {
          upsertSsd(ssd);
          const games = await invoke<Game[]>("get_ssd_catalog", {
            driveLetter: DEFAULT_DRIVE_LETTER,
          });
          mergeGames(games);
          const libs = useLibraryStore.getState().localLibraries;
          await refreshInstalled(libs);
        } else {
          setSsdConnected(ssd.id, true);
        }
        lastConnectedRef.current = true;
      } catch {
        if (lastConnectedRef.current !== false) {
          ssds.forEach((s) => setSsdConnected(s.id, false));
        }
        lastConnectedRef.current = false;
      }
    }, HOTPLUG_INTERVAL_MS);
    return () => clearInterval(interval);
  }, [ssds, upsertSsd, mergeGames, setSsdConnected, refreshInstalled]);

  useEffect(() => {
    loadInitial();
  }, [loadInitial]);

  return { rescan, loadInitial };
}
