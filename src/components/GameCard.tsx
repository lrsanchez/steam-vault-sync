import { useState } from "react";
import clsx from "clsx";
import { confirm } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useCopy } from "@/hooks/useCopy";
import { useLibraryStore } from "@/store/library";
import type { Game, LocalLibrary } from "@/types";

interface GameCardProps {
  game: Game;
}

function ssdBadgeClass(driveLetter: string): string {
  // Stable color per letter — S → purple, others rotate
  if (driveLetter === "S") return "ssd-badge-1";
  if (driveLetter === "T") return "ssd-badge-2";
  return "ssd-badge-default";
}

export function GameCard({ game }: GameCardProps) {
  const { startCopy, removeLocal } = useCopy();
  const localLibraries = useLibraryStore((s) => s.localLibraries);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [imgFailed, setImgFailed] = useState(false);

  const disabled = !game.isAvailable;

  const onCopyClick = async () => {
    if (localLibraries.length === 0) return;
    if (localLibraries.length === 1) {
      await startCopy(game, localLibraries[0]);
    } else {
      setPickerOpen(true);
    }
  };

  const onRemoveClick = async () => {
    const ok = await confirm(
      `Remove ${game.title} from this PC? Vault copy is safe.`,
      { title: "Remove local copy", kind: "warning" },
    );
    if (ok) await removeLocal(game);
  };

  const onLaunchClick = async () => {
    if (!game.appId) return;
    try {
      await invoke("launch_game", { appId: game.appId });
    } catch (e) {
      console.error("Launch failed:", e);
    }
  };

  return (
    <div
      className={clsx(
        "group relative rounded-md overflow-hidden bg-neutral-900 border border-neutral-800 flex flex-col",
        disabled && "opacity-50",
      )}
    >
      <div className="aspect-[2/3] bg-neutral-800 relative">
        {game.coverUrl && !imgFailed ? (
          <img
            src={game.coverUrl}
            alt={game.title}
            className="w-full h-full object-cover"
            onError={() => setImgFailed(true)}
            loading="lazy"
          />
        ) : (
          <div className="absolute inset-0 flex items-center justify-center text-2xl text-neutral-600">
            ▣
          </div>
        )}

        <div className="absolute top-1.5 left-1.5 flex gap-1">
          <span
            className={clsx(
              "px-1.5 py-0.5 rounded text-[10px] font-medium",
              ssdBadgeClass(game.ssdDriveLetter),
            )}
          >
            {game.ssdDriveLetter}:
          </span>
          {game.isInstalled && (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-accent-green/20 text-accent-green">
              Local
            </span>
          )}
        </div>
      </div>

      <div className="p-2 flex-1 flex flex-col gap-1">
        <div className="text-xs font-medium truncate" title={game.title}>
          {game.title}
        </div>
        <div className="text-[11px] text-neutral-500">
          {game.sizeGb.toFixed(1)} GB
        </div>

        <div className="mt-auto pt-2 flex items-center gap-1">
          {game.isInstalled ? (
            <>
              <button
                onClick={onLaunchClick}
                disabled={!game.appId}
                title={
                  game.appId
                    ? "Launch in Steam"
                    : "Steam AppID unknown — try Rescan"
                }
                className={clsx(
                  "flex-1 text-[11px] py-1 rounded font-medium transition-colors",
                  game.appId
                    ? "bg-accent-green text-white hover:bg-accent-green/90"
                    : "bg-neutral-800 text-neutral-600 cursor-not-allowed",
                )}
              >
                Launch Game
              </button>
              <button
                onClick={onRemoveClick}
                title="Remove local copy"
                className="text-[11px] py-1 px-2 rounded text-red-400 hover:bg-red-500/10"
              >
                ✕
              </button>
            </>
          ) : (
            <button
              disabled={disabled}
              onClick={onCopyClick}
              className={clsx(
                "flex-1 text-[11px] py-1 rounded font-medium transition-colors",
                disabled
                  ? "bg-neutral-800 text-neutral-600 cursor-not-allowed"
                  : "bg-accent-blue text-white hover:bg-accent-blue/90",
              )}
            >
              Copy to PC
            </button>
          )}
        </div>
      </div>

      {pickerOpen && (
        <div
          className="absolute inset-0 bg-black/70 flex items-center justify-center p-2 z-10"
          onClick={() => setPickerOpen(false)}
        >
          <div
            className="bg-neutral-900 border border-neutral-700 rounded-md p-2 w-full space-y-1"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="text-[11px] text-neutral-400 px-1 pb-1">
              Choose library
            </div>
            {localLibraries.map((lib: LocalLibrary) => (
              <button
                key={lib.path}
                className="w-full text-left text-[11px] px-2 py-1.5 rounded hover:bg-neutral-800"
                onClick={async () => {
                  setPickerOpen(false);
                  await startCopy(game, lib);
                }}
              >
                <div className="font-medium">{lib.driveLetter}:</div>
                <div className="text-neutral-500 truncate">{lib.path}</div>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
