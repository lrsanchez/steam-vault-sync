import { useEffect, useMemo, useState } from "react";
import { Store } from "@tauri-apps/plugin-store";
import { Titlebar } from "@/components/Titlebar";
import { Sidebar } from "@/components/Sidebar";
import { Toolbar } from "@/components/Toolbar";
import { GameGrid } from "@/components/GameGrid";
import { ProgressPanel } from "@/components/ProgressPanel";
import { Settings } from "@/components/Settings";
import { useLibrary } from "@/hooks/useLibrary";
import { useCopy } from "@/hooks/useCopy";
import { useSteam } from "@/hooks/useSteam";
import { useLibraryStore } from "@/store/library";
import { DEFAULT_SETTINGS, type AppSettings } from "@/types";

type View = "library" | "settings";

export default function App() {
  const [view, setView] = useState<View>("library");
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [settingsReady, setSettingsReady] = useState(false);

  const games = useLibraryStore((s) => s.games);
  const filter = useLibraryStore((s) => s.filter);
  const searchQuery = useLibraryStore((s) => s.searchQuery);
  const activeCopy = useLibraryStore((s) => s.activeCopy);

  useEffect(() => {
    (async () => {
      try {
        const store = await Store.load("settings.json");
        const loaded: Partial<AppSettings> = {
          vaultDriveLetter:
            (await store.get<string>("vaultDriveLetter")) ?? "S",
          defaultLocalLibraryPath:
            (await store.get<string>("defaultLocalLibraryPath")) ?? "",
          theme:
            ((await store.get<AppSettings["theme"]>("theme")) ?? "auto"),
        };
        setSettings({ ...DEFAULT_SETTINGS, ...loaded });
      } catch {
        setSettings(DEFAULT_SETTINGS);
      } finally {
        setSettingsReady(true);
      }
    })();
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    const apply = (mode: "light" | "dark") => {
      root.classList.toggle("dark", mode === "dark");
    };
    if (settings.theme === "auto") {
      const mq = window.matchMedia("(prefers-color-scheme: dark)");
      apply(mq.matches ? "dark" : "light");
      const listener = (e: MediaQueryListEvent) =>
        apply(e.matches ? "dark" : "light");
      mq.addEventListener("change", listener);
      return () => mq.removeEventListener("change", listener);
    }
    apply(settings.theme);
  }, [settings.theme]);

  const { rescan } = useLibrary();
  useCopy();
  useSteam();

  const visibleGames = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    return games.filter((g) => {
      if (filter === "available" && !g.isAvailable) return false;
      if (filter === "installed" && !g.isInstalled) return false;
      if (q && !g.title.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [games, filter, searchQuery]);

  if (!settingsReady) {
    return (
      <div className="h-screen w-screen flex items-center justify-center text-sm text-neutral-500">
        Loading…
      </div>
    );
  }

  return (
    <div className="h-screen w-screen flex flex-col bg-neutral-50 dark:bg-neutral-950 text-neutral-900 dark:text-neutral-100">
      <Titlebar />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar
          onRescan={rescan}
          onOpenSettings={() => setView("settings")}
          onOpenLibrary={() => setView("library")}
          currentView={view}
        />
        <main className="flex-1 flex flex-col overflow-hidden">
          {view === "library" ? (
            <>
              <Toolbar />
              <div className="flex-1 overflow-auto">
                <GameGrid games={visibleGames} />
              </div>
            </>
          ) : (
            <Settings
              settings={settings}
              onChange={async (next) => {
                setSettings(next);
                const store = await Store.load("settings.json");
                await store.set("vaultDriveLetter", next.vaultDriveLetter);
                await store.set(
                  "defaultLocalLibraryPath",
                  next.defaultLocalLibraryPath,
                );
                await store.set("theme", next.theme);
                await store.save();
              }}
            />
          )}
        </main>
      </div>
      {activeCopy && <ProgressPanel job={activeCopy} />}
    </div>
  );
}
