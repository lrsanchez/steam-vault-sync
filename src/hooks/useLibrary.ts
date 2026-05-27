import { useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLibraryStore } from "@/store/library";
import type { Game, LocalLibrary, SSD } from "@/types";

const HOTPLUG_INTERVAL_MS = 3000;

export function useLibrary() {
  const upsertSsd = useLibraryStore((s) => s.upsertSsd);
  const setSsdConnected = useLibraryStore((s) => s.setSsdConnected);
  const replaceGamesForSsd = useLibraryStore((s) => s.replaceGamesForSsd);
  const setLocalLibraries = useLibraryStore((s) => s.setLocalLibraries);
  const setLocalOnlyGames = useLibraryStore((s) => s.setLocalOnlyGames);
  const markUpdates = useLibraryStore((s) => s.markUpdates);

  const refreshLocalOnly = useCallback(async (libs: LocalLibrary[]) => {
    const vaultFolderNames = useLibraryStore
      .getState()
      .games.map((g) => g.folderName);
    try {
      const localOnly = await invoke<Game[]>("scan_local_only_games", {
        vaultFolderNames,
        localLibraryPaths: libs.map((l) => l.path),
      });
      setLocalOnlyGames(localOnly);
    } catch (e) {
      console.warn("scan_local_only_games failed:", e);
      setLocalOnlyGames([]);
    }
  }, [setLocalOnlyGames]);

  const checkUpdates = useCallback(async () => {
    const state = useLibraryStore.getState();
    const appIds = Array.from(
      new Set(state.games.map((g) => g.appId).filter((x): x is string => !!x)),
    );
    if (appIds.length === 0) return;

    // Include every Steam library Steam knows about (local + every
    // connected vault). Steam writes pending-update state to whichever
    // library's appmanifest is "active" for an AppID, so we need to
    // check all of them.
    const libraryPaths = new Set<string>();
    for (const lib of state.localLibraries) libraryPaths.add(lib.path);
    for (const ssd of state.ssds) {
      if (ssd.connected) {
        libraryPaths.add(
          `${ssd.driveLetter}:\\SteamLibrary\\steamapps\\common`,
        );
      }
    }

    try {
      const outdated = await invoke<string[]>("check_vault_updates", {
        libraryPaths: Array.from(libraryPaths),
        appIds,
      });
      markUpdates(new Set(outdated));
    } catch (e) {
      console.error("check_vault_updates failed:", e);
    }
  }, [markUpdates]);

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
      const updated = await Promise.all(
        allGames.map(async (g) => {
          const installedPath = map[g.folderName] ?? null;
          let localBuildId: string | null = null;
          if (installedPath && g.appId) {
            try {
              const state = await invoke<{ buildId: string | null } | null>(
                "read_local_appmanifest_state",
                { libraryPath: installedPath, appId: g.appId },
              );
              localBuildId = state?.buildId ?? null;
            } catch {
              // ignore — missing manifest is OK
            }
          }
          return {
            ...g,
            isInstalled: !!installedPath,
            installedPath,
            localBuildId,
          };
        }),
      );
      useLibraryStore.getState().setGames(updated);
    } catch {
      // ignore
    }
  }, []);

  /// Load (or reload) a single vault by drive letter — scans the SSD,
  /// merges its games into the store. Used both at initial load and
  /// when a new vault is hot-plugged.
  const loadVault = useCallback(
    async (letter: string) => {
      try {
        const ssd = await invoke<SSD>("scan_vault_ssd", {
          driveLetter: letter,
        });
        upsertSsd(ssd);
        const games = await invoke<Game[]>("get_ssd_catalog", {
          driveLetter: letter,
        });
        replaceGamesForSsd(ssd.id, games);
        return ssd;
      } catch (e) {
        console.warn(`Failed to load vault ${letter}:`, e);
        return null;
      }
    },
    [upsertSsd, replaceGamesForSsd],
  );

  const loadInitial = useCallback(async () => {
    const letters = await invoke<string[]>("discover_vault_letters").catch(
      () => [] as string[],
    );

    for (const letter of letters) {
      await loadVault(letter);
    }

    try {
      const libs = await invoke<LocalLibrary[]>("scan_local_steam_libraries", {
        vaultDriveLetters: letters,
      });
      setLocalLibraries(libs);
      await refreshInstalled(libs);
      await refreshLocalOnly(libs);
    } catch {
      setLocalLibraries([]);
    }

    checkUpdates();
  }, [loadVault, refreshInstalled, refreshLocalOnly, setLocalLibraries, checkUpdates]);

  /// User-triggered rescan: walks every CONNECTED vault's SteamLibrary
  /// folder, refreshes catalog + buildids, then re-checks updates.
  const rescan = useCallback(async () => {
    const ssds = useLibraryStore.getState().ssds.filter((s) => s.connected);
    for (const ssd of ssds) {
      try {
        const games = await invoke<Game[]>("rescan_ssd", {
          driveLetter: ssd.driveLetter,
        });
        replaceGamesForSsd(ssd.id, games);
      } catch (e) {
        console.error(`Rescan failed for ${ssd.driveLetter}:`, e);
      }
    }
    const libs = useLibraryStore.getState().localLibraries;
    await refreshInstalled(libs);
    await refreshLocalOnly(libs);
    checkUpdates();
  }, [replaceGamesForSsd, refreshInstalled, refreshLocalOnly, checkUpdates]);

  // Hot-plug: poll discovery every 3s, react to new/removed vaults.
  useEffect(() => {
    const interval = setInterval(async () => {
      let detected: string[];
      try {
        detected = await invoke<string[]>("discover_vault_letters");
      } catch {
        return;
      }
      const detectedSet = new Set(detected.map((s) => s.toUpperCase()));
      const known = useLibraryStore.getState().ssds;
      const knownLetters = new Set(known.map((s) => s.driveLetter.toUpperCase()));

      // Newly-connected vaults
      let topologyChanged = false;
      for (const letter of detected) {
        if (!knownLetters.has(letter.toUpperCase())) {
          await loadVault(letter);
          topologyChanged = true;
        }
      }

      // Disappeared vaults — mark disconnected but keep their catalog
      // entries (so user still sees them, just greyed out).
      for (const ssd of known) {
        const stillThere = detectedSet.has(ssd.driveLetter.toUpperCase());
        if (!stillThere && ssd.connected) {
          setSsdConnected(ssd.id, false);
          topologyChanged = true;
        } else if (stillThere && !ssd.connected) {
          setSsdConnected(ssd.id, true);
          topologyChanged = true;
        }
      }

      // If a vault appeared or vanished, refresh local-library exclusion
      // and the update badges.
      if (topologyChanged) {
        try {
          const libs = await invoke<LocalLibrary[]>(
            "scan_local_steam_libraries",
            { vaultDriveLetters: detected },
          );
          setLocalLibraries(libs);
          await refreshInstalled(libs);
          checkUpdates();
        } catch {
          // ignore
        }
      }
    }, HOTPLUG_INTERVAL_MS);
    return () => clearInterval(interval);
  }, [loadVault, setSsdConnected, setLocalLibraries, refreshInstalled, checkUpdates]);

  useEffect(() => {
    loadInitial();
  }, [loadInitial]);

  return { rescan, loadInitial, checkUpdates, loadVault };
}
