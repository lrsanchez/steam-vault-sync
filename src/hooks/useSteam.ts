import { useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLibraryStore } from "@/store/library";
import type { GameMetadata } from "@/types";

export function useSteam() {
  const games = useLibraryStore((s) => s.games);
  const updateGame = useLibraryStore((s) => s.updateGame);
  const resolvedRef = useRef<Set<number>>(new Set());

  const resolveAppId = useCallback(async (folderName: string) => {
    try {
      return await invoke<string | null>("resolve_app_id", { folderName });
    } catch {
      return null;
    }
  }, []);

  const fetchMetadata = useCallback(async (appId: string) => {
    try {
      return await invoke<GameMetadata>("fetch_steam_metadata", {
        appId,
        apiKey: "",
      });
    } catch {
      return null;
    }
  }, []);

  // Background metadata fetcher — runs once per game on mount
  useEffect(() => {
    let cancelled = false;
    (async () => {
      for (const game of games) {
        if (cancelled) return;
        if (resolvedRef.current.has(game.id)) continue;
        if (game.appId && game.coverUrl) {
          resolvedRef.current.add(game.id);
          continue;
        }

        let appId = game.appId;
        if (!appId) {
          appId = await resolveAppId(game.folderName);
          if (!appId) {
            resolvedRef.current.add(game.id);
            continue;
          }
        }

        const meta = await fetchMetadata(appId);
        if (meta && !cancelled) {
          updateGame(game.id, {
            appId: meta.appId,
            coverUrl: meta.libraryCover ?? meta.headerImage ?? game.coverUrl,
            title: meta.name || game.title,
          });

          invoke("upsert_game", {
            driveLetter: game.ssdDriveLetter,
            game: {
              appId: meta.appId,
              title: meta.name || game.title,
              folderName: game.folderName,
              sizeGb: game.sizeGb,
              coverUrl: meta.libraryCover ?? meta.headerImage ?? null,
            },
          }).catch(() => {});
        }
        resolvedRef.current.add(game.id);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [games, resolveAppId, fetchMetadata, updateGame]);

  return { resolveAppId, fetchMetadata };
}
