import { useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
  }, []);

  return { startCopy, removeLocal, pauseCopy, resumeCopy, cancelCopy };
}

function folderFromPath(p: string): string {
  const parts = p.split(/[/\\]/);
  return parts[parts.length - 1] ?? "";
}
