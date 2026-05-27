import { useLibraryStore } from "@/store/library";

export function Toolbar() {
  const searchQuery = useLibraryStore((s) => s.searchQuery);
  const setSearchQuery = useLibraryStore((s) => s.setSearchQuery);
  const filter = useLibraryStore((s) => s.filter);

  return (
    <div className="h-12 shrink-0 border-b border-neutral-800 px-4 flex items-center gap-3 bg-neutral-50 dark:bg-neutral-950">
      <input
        type="text"
        value={searchQuery}
        onChange={(e) => setSearchQuery(e.target.value)}
        placeholder="Search your vault…"
        className="flex-1 max-w-md bg-neutral-100 dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800 rounded px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-accent-blue"
      />
      <div className="text-xs text-neutral-500 capitalize">{filter}</div>
    </div>
  );
}
