import { useLibraryStore } from "@/store/library";

export function Toolbar() {
  const searchQuery = useLibraryStore((s) => s.searchQuery);
  const setSearchQuery = useLibraryStore((s) => s.setSearchQuery);
  const filter = useLibraryStore((s) => s.filter);
  const selectedVaultId = useLibraryStore((s) => s.selectedVaultId);
  const setSelectedVault = useLibraryStore((s) => s.setSelectedVault);
  const ssds = useLibraryStore((s) => s.ssds);

  const selectedVault = selectedVaultId
    ? ssds.find((s) => s.id === selectedVaultId)
    : null;

  return (
    <div className="h-12 shrink-0 border-b border-neutral-800 px-4 flex items-center gap-3 bg-neutral-50 dark:bg-neutral-950">
      <input
        type="text"
        value={searchQuery}
        onChange={(e) => setSearchQuery(e.target.value)}
        placeholder="Search your vault…"
        className="flex-1 max-w-md bg-neutral-100 dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800 rounded px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-accent-blue"
      />

      {selectedVault && (
        <button
          onClick={() => setSelectedVault(null)}
          title="Clear vault filter (show games from all vaults)"
          className="flex items-center gap-1 text-xs px-2 py-1 rounded bg-accent-blue/15 text-accent-blue hover:bg-accent-blue/25 transition-colors"
        >
          <span>{selectedVault.name}</span>
          <span className="opacity-60">✕</span>
        </button>
      )}

      <div className="text-xs text-neutral-500 capitalize">{filter}</div>
    </div>
  );
}
