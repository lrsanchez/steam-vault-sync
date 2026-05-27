import clsx from "clsx";
import { invoke } from "@tauri-apps/api/core";
import { useLibraryStore } from "@/store/library";

interface SidebarProps {
  onRescan: () => void | Promise<void>;
  onOpenSettings: () => void;
  onOpenLibrary: () => void;
  currentView: "library" | "settings";
}

export function Sidebar({
  onRescan,
  onOpenSettings,
  onOpenLibrary,
  currentView,
}: SidebarProps) {
  const ssds = useLibraryStore((s) => s.ssds);
  const games = useLibraryStore((s) => s.games);
  const localOnlyGames = useLibraryStore((s) => s.localOnlyGames);
  const localLibraries = useLibraryStore((s) => s.localLibraries);
  const filter = useLibraryStore((s) => s.filter);
  const setFilter = useLibraryStore((s) => s.setFilter);
  const selectedVaultId = useLibraryStore((s) => s.selectedVaultId);
  const setSelectedVault = useLibraryStore((s) => s.setSelectedVault);

  const totalCount = games.length;
  const installedCount = games.filter((g) => g.isInstalled).length;
  const outdatedCount = games.filter((g) => g.hasUpdate).length;

  // Live count per vault derived from the games array, so the sidebar
  // updates as games are added/removed (inter-vault copy, delete from
  // vault, rescan) without needing to refresh ssd.totalGames.
  const gameCountBySsd = new Map<string, number>();
  for (const g of games) {
    gameCountBySsd.set(g.ssdId, (gameCountBySsd.get(g.ssdId) ?? 0) + 1);
  }

  const installedByDrive = new Map<string, number>();
  for (const g of games) {
    if (!g.isInstalled || !g.installedPath) continue;
    const letter = g.installedPath.charAt(0).toUpperCase();
    installedByDrive.set(letter, (installedByDrive.get(letter) ?? 0) + 1);
  }

  const item = (active: boolean) =>
    clsx(
      "w-full flex items-center justify-between px-3 py-1.5 rounded text-sm cursor-pointer transition-colors",
      active
        ? "bg-accent-blue/15 text-accent-blue"
        : "text-neutral-400 hover:text-neutral-100 hover:bg-neutral-800/60",
    );

  return (
    <aside className="w-60 shrink-0 border-r border-neutral-800 bg-neutral-100 dark:bg-neutral-900 flex flex-col">
      <div className="p-3 space-y-1 flex-1 overflow-auto">
        <button
          className={item(currentView === "library" && filter === "all")}
          onClick={() => {
            onOpenLibrary();
            setFilter("all");
          }}
        >
          <span>All games</span>
          <span className="text-xs text-neutral-500">{totalCount}</span>
        </button>
        <button
          className={item(currentView === "library" && filter === "available")}
          onClick={() => {
            onOpenLibrary();
            setFilter("available");
          }}
        >
          <span>Available</span>
          <span className="text-xs text-neutral-500">
            {games.filter((g) => g.isAvailable).length}
          </span>
        </button>
        <button
          className={item(currentView === "library" && filter === "installed")}
          onClick={() => {
            onOpenLibrary();
            setFilter("installed");
          }}
        >
          <span>Installed on this PC</span>
          <span className="text-xs text-neutral-500">{installedCount}</span>
        </button>
        <button
          className={item(currentView === "library" && filter === "outdated")}
          onClick={() => {
            onOpenLibrary();
            setFilter("outdated");
          }}
        >
          <span className="flex items-center gap-2">
            <span className="w-1.5 h-1.5 rounded-full bg-amber-400" />
            Updates available
          </span>
          <span className="text-xs text-amber-400">{outdatedCount}</span>
        </button>
        <button
          className={item(currentView === "library" && filter === "local-only")}
          onClick={() => {
            onOpenLibrary();
            setFilter("local-only");
          }}
          title="Steam games installed on this PC that aren't in any vault"
        >
          <span>Local only (not in vault)</span>
          <span className="text-xs text-neutral-500">{localOnlyGames.length}</span>
        </button>

        {localLibraries.length > 0 && (
          <div className="pl-3 pr-1 pb-1 space-y-0.5">
            {localLibraries.map((lib) => {
              const letter = lib.driveLetter.toUpperCase();
              const count = installedByDrive.get(letter) ?? 0;
              return (
                <div
                  key={lib.path}
                  className="flex items-center justify-between text-[11px] text-neutral-500"
                  title={lib.path}
                >
                  <span className="flex items-center gap-1.5">
                    <span className="w-1 h-1 rounded-full bg-neutral-600" />
                    {letter}: <span className="text-neutral-600 truncate max-w-[120px]">{lib.path}</span>
                  </span>
                  <span>{count}</span>
                </div>
              );
            })}
          </div>
        )}
        {localLibraries.length === 0 && (
          <div className="pl-6 pr-3 pb-1 text-[11px] text-neutral-600">
            No local Steam libraries detected
          </div>
        )}

        <div className="pt-4 pb-1 px-3 text-[11px] uppercase tracking-wider text-neutral-500">
          SSDs
        </div>
        {ssds.length === 0 && (
          <div className="px-3 py-2 text-xs text-neutral-500">
            No SSD connected
          </div>
        )}
        {ssds.map((ssd) => {
          const isSelected = selectedVaultId === ssd.id;
          return (
            <button
              key={ssd.id}
              onClick={() => {
                onOpenLibrary();
                setSelectedVault(isSelected ? null : ssd.id);
              }}
              title={
                isSelected
                  ? `Showing ${ssd.name} only — click to clear`
                  : `Filter to ${ssd.name}`
              }
              className={clsx(
                "w-full flex items-center justify-between px-3 py-1.5 text-sm rounded transition-colors cursor-pointer",
                isSelected
                  ? "bg-accent-blue/15 text-accent-blue"
                  : "text-neutral-300 hover:bg-neutral-800/60",
              )}
            >
              <span className="flex items-center gap-2">
                <span
                  className={clsx(
                    "w-2 h-2 rounded-full",
                    ssd.connected ? "bg-accent-green" : "bg-neutral-600",
                  )}
                />
                {ssd.name}
              </span>
              <span
                className={clsx(
                  "text-xs",
                  isSelected ? "text-accent-blue" : "text-neutral-500",
                )}
              >
                {gameCountBySsd.get(ssd.id) ?? 0}
              </span>
            </button>
          );
        })}
      </div>

      <div className="p-3 border-t border-neutral-800 space-y-1">
        <button className="btn-ghost w-full text-left" onClick={() => onRescan()}>
          Rescan drives
        </button>
        <button
          className={clsx(
            "btn-ghost w-full text-left",
            currentView === "settings" && "bg-neutral-800",
          )}
          onClick={onOpenSettings}
        >
          Settings
        </button>
        <button
          className="btn-ghost w-full text-left text-red-400 hover:bg-red-500/10"
          onClick={() => invoke("quit_app").catch(() => {})}
        >
          Exit Steam Vault Sync
        </button>
      </div>
    </aside>
  );
}
