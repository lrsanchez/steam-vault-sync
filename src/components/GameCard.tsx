import { useEffect, useRef, useState } from "react";
import clsx from "clsx";
import { confirm } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useCopy } from "@/hooks/useCopy";
import { useLibraryStore } from "@/store/library";
import { LOCAL_ONLY_SSD_ID, type Game, type LocalLibrary, type SSD } from "@/types";

interface GameCardProps {
  game: Game;
}

function ssdBadgeClass(driveLetter: string): string {
  // Stable color per letter — S → purple, others rotate
  if (driveLetter === "S") return "ssd-badge-1";
  if (driveLetter === "T") return "ssd-badge-2";
  return "ssd-badge-default";
}

type PickerMode = "copy" | "autoupdate" | "vault";

export function GameCard({ game }: GameCardProps) {
  const {
    startCopy,
    pushToVault,
    autoUpdateVault,
    removeLocal,
    copyToOtherVault,
    deleteFromVault,
  } = useCopy();
  const localLibraries = useLibraryStore((s) => s.localLibraries);
  const ssds = useLibraryStore((s) => s.ssds);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerMode, setPickerMode] = useState<PickerMode>("copy");
  const [imgFailed, setImgFailed] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<{ top: number; right: number } | null>(
    null,
  );
  const menuRef = useRef<HTMLDivElement | null>(null);
  const menuButtonRef = useRef<HTMLButtonElement | null>(null);

  // Close overflow menu on outside click. Skip the menu itself and the
  // toggle button — clicks on the button are handled by its own onClick.
  useEffect(() => {
    if (!menuOpen) return;
    const handler = (e: MouseEvent) => {
      const target = e.target as Node;
      if (menuRef.current && menuRef.current.contains(target)) return;
      if (menuButtonRef.current && menuButtonRef.current.contains(target)) return;
      setMenuOpen(false);
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [menuOpen]);

  const toggleMenu = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    if (menuOpen) {
      setMenuOpen(false);
      return;
    }
    // Capture button position in viewport coordinates so the menu can
    // render with position:fixed and escape any parent overflow:hidden
    // / overflow:auto containers (the card, the grid scroll container).
    const rect = e.currentTarget.getBoundingClientRect();
    setMenuPos({
      top: rect.top,
      right: window.innerWidth - rect.right,
    });
    setMenuOpen(true);
  };

  const isLocalOnly = game.ssdId === LOCAL_ONLY_SSD_ID;

  // For local-only games, "other vaults" means any connected vault
  // (the source isn't a vault). For vault games, it excludes the
  // source vault as before.
  const otherVaults: SSD[] = ssds.filter(
    (s) => s.connected && (isLocalOnly || s.id !== game.ssdId),
  );

  const disabled = !game.isAvailable;

  const onBackUpToVaultClick = async () => {
    if (otherVaults.length === 0) {
      await confirm("No vault is connected to back up to.", {
        title: "Back up to vault",
        kind: "info",
      });
      return;
    }
    if (otherVaults.length === 1) {
      const dest = otherVaults[0];
      const ok = await confirm(
        `Copy ${game.title} from your local Steam library to vault ${dest.driveLetter}: (${dest.name})?`,
        { title: "Back up to vault", kind: "info" },
      );
      if (ok) await copyToOtherVault(game, dest);
    } else {
      setPickerMode("vault");
      setPickerOpen(true);
    }
  };

  const onCopyClick = async () => {
    if (localLibraries.length === 0) return;
    if (localLibraries.length === 1) {
      await startCopy(game, localLibraries[0]);
    } else {
      setPickerMode("copy");
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

  const onPushClick = async () => {
    const ok = await confirm(
      `Push your local copy of ${game.title} back to the vault?\n\n` +
        `This OVERWRITES the vault folder. Use this AFTER Steam has finished ` +
        `updating the local copy on your fast internal drive — much faster ` +
        `than letting Steam patch the vault directly over USB.`,
      { title: "Update vault from local copy", kind: "info" },
    );
    if (ok) await pushToVault(game);
  };

  const onCopyToOtherVaultClick = async () => {
    setMenuOpen(false);
    if (otherVaults.length === 0) return;
    if (otherVaults.length === 1) {
      const dest = otherVaults[0];
      const ok = await confirm(
        `Copy ${game.title} from ${game.ssdDriveLetter}: to ${dest.driveLetter}: (${dest.name})?\n\n` +
          `USB-to-USB copy may be slower than vault → local → vault staging.`,
        { title: "Copy to another vault", kind: "info" },
      );
      if (ok) await copyToOtherVault(game, dest);
    } else {
      setPickerMode("vault");
      setPickerOpen(true);
    }
  };

  const onDeleteFromVaultClick = async () => {
    setMenuOpen(false);
    const ok = await confirm(
      `Permanently delete ${game.title} from the vault on ${game.ssdDriveLetter}:?\n\n` +
        `This removes the game folder, its appmanifest, the vaultsync catalog entry, ` +
        `and the Steam library reference. Local install on this PC is NOT affected.\n\n` +
        `This cannot be undone — you'd have to redownload via Steam.`,
      { title: "Delete from vault", kind: "warning" },
    );
    if (!ok) return;
    try {
      await deleteFromVault(game);
    } catch (e) {
      await confirm(`Delete failed: ${e}`, {
        title: "Delete from vault",
        kind: "error",
      });
    }
  };

  const onAutoUpdateClick = async () => {
    if (!game.appId) return;
    const ok = await confirm(
      `Auto-update ${game.title} in the vault?\n\n` +
        `What this does:\n` +
        (game.isInstalled
          ? `• Steam patches your local copy on the fast internal drive\n`
          : `• Copies vault → local (uses NVMe, not USB)\n` +
            `• Steam patches the local copy on the fast internal drive\n`) +
        `• Pushes the updated copy back to the vault\n\n` +
        `Important: Steam will close briefly so we can isolate the vault from ` +
        `Steam's view (otherwise Steam updates the vault directly — slow). ` +
        `Steam reopens automatically. When Steam asks where to install, pick the ` +
        `LOCAL drive.`,
      { title: "Auto-update vault copy", kind: "warning" },
    );
    if (!ok) return;

    let picked: LocalLibrary | undefined;
    if (!game.isInstalled && localLibraries.length > 1) {
      setPickerMode("autoupdate");
      setPickerOpen(true);
      return;
    } else if (!game.isInstalled) {
      picked = localLibraries[0];
    }
    await autoUpdateVault(game, picked);
  };

  return (
    <div
      className={clsx(
        "group relative rounded-md bg-neutral-900 border border-neutral-800 flex flex-col",
        disabled && "opacity-50",
      )}
    >
      <div className="aspect-[2/3] bg-neutral-800 relative overflow-hidden rounded-t-md">
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
          {isLocalOnly ? (
            <span
              className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-neutral-700/70 text-neutral-200"
              title="This game is on your PC but not in any vault"
            >
              Local only
            </span>
          ) : (
            <span
              className={clsx(
                "px-1.5 py-0.5 rounded text-[10px] font-medium",
                ssdBadgeClass(game.ssdDriveLetter),
              )}
            >
              {game.ssdDriveLetter}:
            </span>
          )}
          {!isLocalOnly && game.isInstalled && (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-accent-green/20 text-accent-green">
              Local
            </span>
          )}
          {game.hasUpdate && (
            <span
              className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-amber-500/20 text-amber-400"
              title="Steam has a newer build for this game"
            >
              Update
            </span>
          )}
        </div>
      </div>

      <div className="p-2 flex-1 flex flex-col gap-1">
        <div className="text-xs font-medium truncate" title={game.title}>
          {game.title}
        </div>
        <div
          className="text-[11px] text-neutral-500"
          title={
            game.buildId || game.localBuildId
              ? `Build IDs from Steam appmanifest_*.acf` +
                (game.buildId ? `\nVault: ${game.buildId}` : "") +
                (game.localBuildId ? `\nLocal: ${game.localBuildId}` : "")
              : undefined
          }
        >
          <span>{game.sizeGb.toFixed(1)} GB</span>
          {isLocalOnly
            ? game.localBuildId && (
                <>
                  <span className="mx-1">·</span>
                  <span>
                    PC <span className="text-neutral-400">{game.localBuildId}</span>
                  </span>
                </>
              )
            : (game.buildId || game.localBuildId) && (
                <>
                  <span className="mx-1">·</span>
                  {game.buildId && (
                    <span>
                      vault{" "}
                      <span className="text-neutral-400">{game.buildId}</span>
                    </span>
                  )}
                  {game.isInstalled && game.localBuildId && (
                    <>
                      <span className="mx-1">·</span>
                      <span>
                        PC{" "}
                        <span
                          className={clsx(
                            game.buildId && game.localBuildId !== game.buildId
                              ? "text-amber-400"
                              : "text-neutral-400",
                          )}
                        >
                          {game.localBuildId}
                        </span>
                      </span>
                    </>
                  )}
                </>
              )}
        </div>

        <div className="mt-auto pt-2 flex items-center gap-1">
          {isLocalOnly ? (
            // Local-only game: primary action is to back up into a vault.
            <>
              <button
                onClick={onBackUpToVaultClick}
                disabled={otherVaults.length === 0}
                title={
                  otherVaults.length === 0
                    ? "Connect a vault SSD to back up this game"
                    : "Copy this local install into a vault"
                }
                className={clsx(
                  "flex-1 text-[11px] py-1 rounded font-medium transition-colors",
                  otherVaults.length > 0
                    ? "bg-accent-blue text-white hover:bg-accent-blue/90"
                    : "bg-neutral-800 text-neutral-600 cursor-not-allowed",
                )}
              >
                → Back up to vault
              </button>
              <button
                onClick={onLaunchClick}
                disabled={!game.appId}
                title="Launch in Steam"
                className={clsx(
                  "text-[11px] py-1 px-2 rounded",
                  game.appId
                    ? "text-accent-green hover:bg-accent-green/10"
                    : "text-neutral-600 cursor-not-allowed",
                )}
              >
                ▶
              </button>
              <button
                onClick={onRemoveClick}
                title="Remove local copy"
                className="text-[11px] py-1 px-2 rounded text-red-400 hover:bg-red-500/10"
              >
                ✕
              </button>
            </>
          ) : game.hasUpdate && game.isAvailable && game.appId ? (
            // Outdated vault copy: surface the one-button automated
            // workflow. Other actions become small icon buttons.
            <>
              <button
                onClick={onAutoUpdateClick}
                title="Stage to local → Steam patches → push back to vault"
                className="flex-1 text-[11px] py-1 rounded font-medium bg-amber-500 text-white hover:bg-amber-600 transition-colors"
              >
                ↻ Update Vault
              </button>
              {game.isInstalled && (
                <button
                  onClick={onLaunchClick}
                  title="Launch in Steam"
                  className="text-[11px] py-1 px-2 rounded text-accent-green hover:bg-accent-green/10"
                >
                  ▶
                </button>
              )}
              {game.isInstalled && (
                <button
                  onClick={onRemoveClick}
                  title="Remove local copy"
                  className="text-[11px] py-1 px-2 rounded text-red-400 hover:bg-red-500/10"
                >
                  ✕
                </button>
              )}
            </>
          ) : game.isInstalled ? (
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
                onClick={onPushClick}
                disabled={!game.isAvailable}
                title={
                  game.isAvailable
                    ? "Push local copy back to vault (after Steam has updated it)"
                    : "Vault SSD not connected"
                }
                className={clsx(
                  "text-[11px] py-1 px-2 rounded transition-colors",
                  game.isAvailable
                    ? "text-accent-blue hover:bg-accent-blue/10"
                    : "text-neutral-600 cursor-not-allowed",
                )}
              >
                ↑ Vault
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

          {/* Overflow menu trigger. The dropdown itself renders below
              using position:fixed so it can escape overflow boundaries
              (card border, grid scroll container, sidebar). Hidden for
              local-only games since both menu items are vault-specific. */}
          {!isLocalOnly && (
            <button
              ref={menuButtonRef}
              onClick={toggleMenu}
              title="More actions"
              className="text-[11px] py-1 px-1.5 rounded text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
            >
              ⋯
            </button>
          )}
        </div>
      </div>

      {menuOpen && menuPos && (
        <div
          ref={menuRef}
          style={{
            position: "fixed",
            top: menuPos.top,
            right: menuPos.right,
            transform: "translateY(-100%) translateY(-4px)",
          }}
          className="bg-neutral-900 border border-neutral-700 rounded-md shadow-lg z-50 min-w-[180px] py-1"
        >
          <button
            disabled={!game.isAvailable || otherVaults.length === 0}
            onClick={onCopyToOtherVaultClick}
            title={
              !game.isAvailable
                ? "Source vault is not connected"
                : otherVaults.length === 0
                  ? "No other vault connected"
                  : "Copy this game to another vault"
            }
            className={clsx(
              "w-full text-left text-[11px] px-3 py-1.5",
              game.isAvailable && otherVaults.length > 0
                ? "text-neutral-200 hover:bg-neutral-800"
                : "text-neutral-600 cursor-not-allowed",
            )}
          >
            → Copy to other vault…
          </button>
          <div className="my-0.5 border-t border-neutral-800" />
          <button
            disabled={!game.isAvailable}
            onClick={onDeleteFromVaultClick}
            title={
              !game.isAvailable
                ? "Vault SSD not connected"
                : "Permanently delete from vault"
            }
            className={clsx(
              "w-full text-left text-[11px] px-3 py-1.5",
              game.isAvailable
                ? "text-red-400 hover:bg-red-500/10"
                : "text-neutral-600 cursor-not-allowed",
            )}
          >
            ✕ Delete from vault
          </button>
        </div>
      )}

      {pickerOpen && (
        <div
          className="absolute inset-0 bg-black/70 flex items-center justify-center p-2 z-10 rounded-md"
          onClick={() => setPickerOpen(false)}
        >
          <div
            className="bg-neutral-900 border border-neutral-700 rounded-md p-2 w-full space-y-1"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="text-[11px] text-neutral-400 px-1 pb-1">
              {pickerMode === "vault" ? "Choose destination vault" : "Choose library"}
            </div>
            {pickerMode === "vault"
              ? otherVaults.map((dest) => (
                  <button
                    key={dest.id}
                    className="w-full text-left text-[11px] px-2 py-1.5 rounded hover:bg-neutral-800"
                    onClick={async () => {
                      setPickerOpen(false);
                      await copyToOtherVault(game, dest);
                    }}
                  >
                    <div className="font-medium">{dest.driveLetter}:</div>
                    <div className="text-neutral-500 truncate">{dest.name}</div>
                  </button>
                ))
              : localLibraries.map((lib: LocalLibrary) => (
                  <button
                    key={lib.path}
                    className="w-full text-left text-[11px] px-2 py-1.5 rounded hover:bg-neutral-800"
                    onClick={async () => {
                      setPickerOpen(false);
                      if (pickerMode === "autoupdate") {
                        await autoUpdateVault(game, lib);
                      } else {
                        await startCopy(game, lib);
                      }
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
