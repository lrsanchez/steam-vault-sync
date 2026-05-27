import { useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { confirm } from "@tauri-apps/plugin-dialog";
import { useLibraryStore } from "@/store/library";
import type { Game, LocalLibrary } from "@/types";

interface ProgressPayload {
  copiedBytes: number;
  totalBytes: number;
  speedBps: number;
  etaSeconds: number;
}

interface DonePayload {
  title: string;
  destPath: string;
}

interface ErrorPayload {
  title: string;
  error: string;
}

export function useCopy() {
  const setActiveCopy = useLibraryStore((s) => s.setActiveCopy);
  const updateActiveCopy = useLibraryStore((s) => s.updateActiveCopy);
  const completeActiveCopy = useLibraryStore((s) => s.completeActiveCopy);
  const failActiveCopy = useLibraryStore((s) => s.failActiveCopy);
  const markInstalled = useLibraryStore((s) => s.markInstalled);
  const enqueueCopy = useLibraryStore((s) => s.enqueueCopy);
  const replaceGamesForSsd = useLibraryStore((s) => s.replaceGamesForSsd);
  const removeGame = useLibraryStore((s) => s.removeGame);

  useEffect(() => {
    const unlistenFns: Array<() => void> = [];

    listen<ProgressPayload>("copy://progress", (e) => {
      updateActiveCopy({
        copiedBytes: e.payload.copiedBytes,
        totalBytes: e.payload.totalBytes,
        speedBps: e.payload.speedBps,
        etaSeconds: e.payload.etaSeconds,
        status: "copying",
      });
    }).then((u) => unlistenFns.push(u));

    listen<string>("copy://paused", () => {
      updateActiveCopy({ status: "paused" });
    }).then((u) => unlistenFns.push(u));

    listen<string>("copy://resumed", () => {
      updateActiveCopy({ status: "copying" });
    }).then((u) => unlistenFns.push(u));

    listen<DonePayload>("copy://done", (e) => {
      const active = useLibraryStore.getState().activeCopy;
      if (active) {
        markInstalled(folderFromPath(active.sourcePath), active.destLibraryPath);
        const game = useLibraryStore
          .getState()
          .games.find((g) => g.id === active.gameId);
        if (game?.appId) {
          invoke("register_game_in_steam", { appId: game.appId }).catch(() => {});
        }
      }
      completeActiveCopy();
      void e;
    }).then((u) => unlistenFns.push(u));

    // Vault push completion: don't mark-installed (game was already
    // installed locally), don't fire steam://install (that would
    // re-trigger Steam's slow patcher we just bypassed). Just close
    // the progress panel.
    listen<DonePayload>("vault-push://done", () => {
      completeActiveCopy();
    }).then((u) => unlistenFns.push(u));

    listen<ErrorPayload>("copy://error", (e) => {
      failActiveCopy(e.payload.error);
    }).then((u) => unlistenFns.push(u));

    listen<ErrorPayload>("copy://cancelled", () => {
      completeActiveCopy();
    }).then((u) => unlistenFns.push(u));

    return () => {
      unlistenFns.forEach((fn) => fn());
    };
  }, [updateActiveCopy, completeActiveCopy, failActiveCopy, markInstalled]);

  const startCopy = useCallback(
    async (game: Game, destLibrary: LocalLibrary) => {
      const sourcePath = `${game.ssdDriveLetter}:\\SteamLibrary\\steamapps\\common\\${game.folderName}`;
      const job = {
        gameId: game.id,
        title: game.title,
        sourcePath,
        destLibraryPath: destLibrary.path,
        totalBytes: Math.round(game.sizeGb * 1_073_741_824),
        copiedBytes: 0,
        speedBps: 0,
        etaSeconds: 0,
        status: "queued" as const,
      };
      enqueueCopy(job);
      setActiveCopy(job);
      try {
        await invoke("copy_game", {
          sourcePath,
          destLibraryPath: destLibrary.path,
          gameTitle: game.title,
          appId: game.appId,
        });
      } catch (e) {
        failActiveCopy(String(e));
      }
    },
    [enqueueCopy, setActiveCopy, failActiveCopy],
  );

  const copyToOtherVault = useCallback(
    async (game: Game, destSsd: { id: string; driveLetter: string }) => {
      if (!game.isAvailable) return;
      const sourceLibPath = `${game.ssdDriveLetter}:\\SteamLibrary\\steamapps\\common`;
      const destLibPath = `${destSsd.driveLetter}:\\SteamLibrary\\steamapps\\common`;
      const job = {
        gameId: game.id,
        title: game.title,
        sourcePath: `${sourceLibPath}\\${game.folderName}`,
        destLibraryPath: destLibPath,
        totalBytes: Math.round(game.sizeGb * 1_073_741_824),
        copiedBytes: 0,
        speedBps: 0,
        etaSeconds: 0,
        status: "queued" as const,
      };
      enqueueCopy(job);
      setActiveCopy(job);
      try {
        await invoke("copy_to_vault", {
          sourceLibPath,
          destLibPath,
          folderName: game.folderName,
          appId: game.appId,
          gameTitle: game.title,
        });
        // Refresh dest vault catalog so the new copy appears in the
        // grid without requiring a manual Rescan.
        try {
          const games = await invoke<Game[]>("rescan_ssd", {
            driveLetter: destSsd.driveLetter,
          });
          replaceGamesForSsd(destSsd.id, games);
        } catch (e) {
          console.warn(
            "Post-copy rescan failed (use Rescan drives to refresh):",
            e,
          );
        }
      } catch (e) {
        failActiveCopy(String(e));
      }
    },
    [enqueueCopy, setActiveCopy, failActiveCopy, replaceGamesForSsd],
  );

  const deleteFromVault = useCallback(
    async (game: Game) => {
      try {
        await invoke("delete_from_vault", {
          driveLetter: game.ssdDriveLetter,
          folderName: game.folderName,
          appId: game.appId,
        });
        removeGame(game.id);
      } catch (e) {
        console.error("delete_from_vault failed:", e);
        throw e;
      }
    },
    [removeGame],
  );

  const pushToVault = useCallback(
    async (game: Game) => {
      if (!game.installedPath) return;
      const vaultLibPath = `${game.ssdDriveLetter}:\\SteamLibrary\\steamapps\\common`;
      const job = {
        gameId: game.id,
        title: game.title,
        sourcePath: `${game.installedPath}\\${game.folderName}`,
        destLibraryPath: vaultLibPath,
        totalBytes: Math.round(game.sizeGb * 1_073_741_824),
        copiedBytes: 0,
        speedBps: 0,
        etaSeconds: 0,
        status: "queued" as const,
      };
      enqueueCopy(job);
      setActiveCopy(job);
      try {
        await invoke("push_to_vault", {
          localLibPath: game.installedPath,
          vaultLibPath,
          folderName: game.folderName,
          appId: game.appId,
          gameTitle: game.title,
        });
      } catch (e) {
        failActiveCopy(String(e));
      }
    },
    [enqueueCopy, setActiveCopy, failActiveCopy],
  );

  const removeLocal = useCallback(
    async (game: Game) => {
      if (!game.installedPath) return;
      try {
        await invoke("remove_local_game", {
          libraryPath: game.installedPath,
          folderName: game.folderName,
          appId: game.appId,
        });
        markInstalled(game.folderName, null);
      } catch (e) {
        console.error("Remove failed:", e);
      }
    },
    [markInstalled],
  );

  /// Full automated vault-update workflow:
  ///   1. Copy vault → local (if not already installed)
  ///   2. Trigger Steam to update the local copy
  ///   3. Poll the local appmanifest until Steam reports fully-installed
  ///      with a buildid different from the vault's
  ///   4. Push the updated local copy back to vault
  ///   5. (Caller can ask to remove local afterwards)
  const autoUpdateVault = useCallback(
    async (game: Game, pickedLibrary?: LocalLibrary) => {
      if (!game.appId) {
        console.error("Cannot auto-update without an AppID");
        return;
      }

      const libs = useLibraryStore.getState().localLibraries;

      // Step 1: ensure the game is installed locally
      let installedAt = game.installedPath;
      if (!installedAt) {
        const target = pickedLibrary ?? libs[0];
        if (!target) {
          console.error("No local Steam library available to stage to");
          return;
        }
        await startCopy(game, target);
        installedAt = target.path;
        // startCopy returns once the spawn_blocking task is dispatched
        // and progress events start flowing; the copy://done listener
        // will fire markInstalled. We need to wait until isInstalled
        // becomes true OR the copy fails.
        await waitFor(() => {
          const g = useLibraryStore.getState().games.find((x) => x.id === game.id);
          const active = useLibraryStore.getState().activeCopy;
          return (g?.isInstalled ?? false) || active === null;
        }, 24 * 60 * 60 * 1000); // hard cap 24h
        const refreshed = useLibraryStore
          .getState()
          .games.find((x) => x.id === game.id);
        if (!refreshed?.isInstalled) {
          console.error("Stage-to-local failed; aborting auto-update");
          return;
        }
        installedAt = refreshed.installedPath ?? installedAt;
      }

      // Step 2a: isolate the vault from Steam's view BEFORE triggering
      // an update. This hides the vault appmanifest, edits
      // libraryfolders.vdf to remove the AppID from vault's apps list,
      // and gracefully closes Steam so the VDF edit sticks. Without
      // all three steps Steam re-discovers the vault install on
      // restart and patches that instead of local.
      const vaultLibPath = `${game.ssdDriveLetter}:\\SteamLibrary\\steamapps\\common`;
      let isolated = false;
      try {
        await invoke("isolate_vault_for_steam_update", {
          vaultLibPath,
          vaultDriveLetter: game.ssdDriveLetter,
          appId: game.appId,
        });
        isolated = true;
      } catch (e) {
        console.error("Failed to isolate vault:", e);
        failActiveCopy(`Could not isolate vault: ${e}`);
        return;
      }

      // Step 2b: trigger Steam to install/update the local copy.
      // Steam launches fresh (we just closed it), reads the modified
      // libraryfolders.vdf, sees no vault install for this AppID, and
      // patches the local install only.
      try {
        await invoke("register_game_in_steam", { appId: game.appId });
      } catch (e) {
        console.error("Failed to open Steam:", e);
        if (isolated) {
          await invoke("restore_vault_from_isolation", {
            vaultLibPath,
            appId: game.appId,
          }).catch(() => {});
        }
        return;
      }

      // Step 3: poll the local appmanifest until Steam reports
      // fully-installed AND a buildid different from the vault's.
      // While we poll, set a synthetic "Waiting for Steam" copy job
      // so the user sees what's happening.
      const fakeJob = {
        gameId: game.id,
        title: `${game.title} — waiting for Steam to update…`,
        sourcePath: "",
        destLibraryPath: installedAt!,
        totalBytes: 0,
        copiedBytes: 0,
        speedBps: 0,
        etaSeconds: 0,
        status: "copying" as const,
      };
      setActiveCopy(fakeJob);

      const ok = await waitFor(
        async () => {
          // Exit immediately if the user clicked Cancel on the panel.
          const active = useLibraryStore.getState().activeCopy;
          if (!active || active.status === "cancelled") return true;

          try {
            const state = await invoke<{
              fullyInstalled: boolean;
              buildId: string | null;
            } | null>("read_local_appmanifest_state", {
              libraryPath: installedAt,
              appId: game.appId,
            });
            if (!state) return false;
            const buildChanged =
              !!state.buildId && state.buildId !== game.buildId;
            return state.fullyInstalled && buildChanged;
          } catch {
            return false;
          }
        },
        2 * 60 * 60 * 1000, // hard cap 2h for Steam to finish
        5000, // poll every 5s
      );

      // Distinguish: did we exit because of cancel, timeout, or success?
      const finalActive = useLibraryStore.getState().activeCopy;
      if (!finalActive || finalActive.status === "cancelled") {
        if (isolated) {
          await invoke("restore_vault_from_isolation", {
            vaultLibPath,
            appId: game.appId,
          }).catch(() => {});
        }
        completeActiveCopy();
        return;
      }

      if (!ok) {
        failActiveCopy("Timed out waiting for Steam to finish updating");
        if (isolated) {
          await invoke("restore_vault_from_isolation", {
            vaultLibPath,
            appId: game.appId,
          }).catch(() => {});
        }
        return;
      }

      // Brief breather so any in-flight file handles release.
      await sleep(500);

      // Step 4: push the updated local copy back to vault. This
      // overwrites the vault folder and writes a fresh appmanifest
      // (from the updated local copy) to where the hidden one used
      // to be.
      const refreshed = useLibraryStore
        .getState()
        .games.find((x) => x.id === game.id);
      if (!refreshed) return;

      try {
        await pushToVault(refreshed);
      } catch (e) {
        console.error("push_to_vault failed:", e);
        if (isolated) {
          await invoke("restore_vault_from_isolation", {
            vaultLibPath,
            appId: game.appId,
          }).catch(() => {});
        }
        return;
      }

      // Push succeeded — restore libraryfolders.vdf from backup and
      // discard the hidden manifest sentinel (push_to_vault already
      // copied a fresh manifest into place).
      if (isolated) {
        await invoke("restore_vault_from_isolation", {
          vaultLibPath,
          appId: game.appId,
        }).catch(() => {});
      }

      // Step 5: optionally remove the local copy. Default = keep, so
      // launching the game from Steam is instant (no need to fetch
      // from the vault next time).
      const keepLocal = await confirm(
        `Vault is updated.\n\nKeep the local copy of ${game.title} on this PC for fast launches, ` +
          `or remove it to free up internal SSD space?`,
        {
          title: "Keep local copy?",
          kind: "info",
          okLabel: "Keep local copy",
          cancelLabel: "Remove local copy",
        },
      );
      if (!keepLocal) {
        await removeLocal(refreshed);
      }
    },
    [startCopy, pushToVault, setActiveCopy, failActiveCopy, removeLocal, completeActiveCopy],
  );

  const pauseCopy = useCallback(async () => {
    try {
      await invoke("pause_copy");
    } catch (e) {
      console.error("Pause failed:", e);
    }
  }, []);

  const resumeCopy = useCallback(async () => {
    try {
      await invoke("resume_copy");
    } catch (e) {
      console.error("Resume failed:", e);
    }
  }, []);

  const cancelCopy = useCallback(async () => {
    try {
      await invoke("cancel_copy");
    } catch (e) {
      console.error("Cancel failed:", e);
    }
    // Flip the active job to "cancelled" so JS-side polling loops
    // (the Steam-patch wait in autoUpdateVault) can also exit. The
    // Rust cancel_copy command is a no-op when no chunked copy is in
    // flight (i.e., during the "waiting for Steam" phase) so we need
    // this frontend signal too.
    updateActiveCopy({ status: "cancelled" });
  }, [updateActiveCopy]);

  return {
    startCopy,
    pushToVault,
    copyToOtherVault,
    deleteFromVault,
    autoUpdateVault,
    removeLocal,
    pauseCopy,
    resumeCopy,
    cancelCopy,
  };
}

function folderFromPath(p: string): string {
  const parts = p.split(/[/\\]/);
  return parts[parts.length - 1] ?? "";
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/// Poll `predicate` every `intervalMs` until it returns truthy or
/// `timeoutMs` elapses. Returns true on success, false on timeout.
/// Predicate may be sync or async.
async function waitFor(
  predicate: () => boolean | Promise<boolean>,
  timeoutMs: number,
  intervalMs: number = 250,
): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (await predicate()) return true;
    await sleep(intervalMs);
  }
  return false;
}
