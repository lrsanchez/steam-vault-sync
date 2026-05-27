import { create } from "zustand";
import type {
  CopyJob,
  Game,
  LibraryFilter,
  LocalLibrary,
  SSD,
} from "@/types";

interface AppState {
  ssds: SSD[];
  games: Game[];
  localLibraries: LocalLibrary[];
  copyQueue: CopyJob[];
  activeCopy: CopyJob | null;
  filter: LibraryFilter;
  searchQuery: string;
  setSsds: (ssds: SSD[]) => void;
  upsertSsd: (ssd: SSD) => void;
  setSsdConnected: (id: string, connected: boolean) => void;
  setGames: (games: Game[]) => void;
  mergeGames: (games: Game[]) => void;
  updateGame: (id: number, patch: Partial<Game>) => void;
  setLocalLibraries: (libs: LocalLibrary[]) => void;
  setFilter: (filter: LibraryFilter) => void;
  setSearchQuery: (q: string) => void;
  enqueueCopy: (job: CopyJob) => void;
  setActiveCopy: (job: CopyJob | null) => void;
  updateActiveCopy: (patch: Partial<CopyJob>) => void;
  completeActiveCopy: () => void;
  failActiveCopy: (error: string) => void;
  markInstalled: (folderName: string, path: string | null) => void;
}

export const useLibraryStore = create<AppState>((set) => ({
  ssds: [],
  games: [],
  localLibraries: [],
  copyQueue: [],
  activeCopy: null,
  filter: "all",
  searchQuery: "",

  setSsds: (ssds) => set({ ssds }),
  upsertSsd: (ssd) =>
    set((state) => {
      const idx = state.ssds.findIndex((s) => s.id === ssd.id);
      if (idx === -1) return { ssds: [...state.ssds, ssd] };
      const next = [...state.ssds];
      next[idx] = ssd;
      return { ssds: next };
    }),
  setSsdConnected: (id, connected) =>
    set((state) => ({
      ssds: state.ssds.map((s) => (s.id === id ? { ...s, connected } : s)),
      games: state.games.map((g) =>
        g.ssdId === id ? { ...g, isAvailable: connected } : g,
      ),
    })),

  setGames: (games) => set({ games }),
  mergeGames: (incoming) =>
    set((state) => {
      const byKey = new Map<string, Game>();
      for (const g of state.games) byKey.set(`${g.ssdId}:${g.folderName}`, g);
      for (const g of incoming) byKey.set(`${g.ssdId}:${g.folderName}`, g);
      return { games: Array.from(byKey.values()) };
    }),
  updateGame: (id, patch) =>
    set((state) => ({
      games: state.games.map((g) => (g.id === id ? { ...g, ...patch } : g)),
    })),

  setLocalLibraries: (libs) => set({ localLibraries: libs }),
  setFilter: (filter) => set({ filter }),
  setSearchQuery: (q) => set({ searchQuery: q }),

  enqueueCopy: (job) =>
    set((state) => ({ copyQueue: [...state.copyQueue, job] })),
  setActiveCopy: (job) => set({ activeCopy: job }),
  updateActiveCopy: (patch) =>
    set((state) => ({
      activeCopy: state.activeCopy ? { ...state.activeCopy, ...patch } : null,
    })),
  completeActiveCopy: () =>
    set((state) => ({
      activeCopy: null,
      copyQueue: state.copyQueue.filter(
        (j) => j.gameId !== state.activeCopy?.gameId,
      ),
    })),
  failActiveCopy: (error) =>
    set((state) => ({
      activeCopy: state.activeCopy
        ? { ...state.activeCopy, status: "error", error }
        : null,
    })),

  markInstalled: (folderName, path) =>
    set((state) => ({
      games: state.games.map((g) =>
        g.folderName === folderName
          ? { ...g, isInstalled: path !== null, installedPath: path }
          : g,
      ),
    })),
}));
