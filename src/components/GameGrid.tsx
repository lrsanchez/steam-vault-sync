import { GameCard } from "./GameCard";
import { useLibraryStore } from "@/store/library";
import type { Game } from "@/types";

export function GameGrid({ games }: { games: Game[] }) {
  const allGames = useLibraryStore((s) => s.games);
  const ssds = useLibraryStore((s) => s.ssds);
  const selectedVaultId = useLibraryStore((s) => s.selectedVaultId);
  const filter = useLibraryStore((s) => s.filter);
  const searchQuery = useLibraryStore((s) => s.searchQuery);

  if (games.length === 0) {
    // Tailor the empty-state message to why the grid is empty: no
    // vaults at all, a filter that excludes everything, or a vault
    // that happens to be empty.
    let message: string;
    if (allGames.length === 0) {
      message =
        ssds.length === 0
          ? "Connect your SSD Vault to see your games."
          : "Your vault is empty. Add games to S:\\SteamLibrary\\steamapps\\common\\ and click Rescan drives.";
    } else if (selectedVaultId) {
      const vault = ssds.find((s) => s.id === selectedVaultId);
      message = vault
        ? `No games on ${vault.name} match the current filter.`
        : "No games match the current filter.";
    } else if (searchQuery.trim()) {
      message = `No games match "${searchQuery.trim()}".`;
    } else if (filter !== "all") {
      message = `No games match the ${filter} filter.`;
    } else {
      message = "No games to display.";
    }
    return (
      <div className="h-full flex items-center justify-center text-sm text-neutral-500 px-6 text-center">
        {message}
      </div>
    );
  }

  return (
    <div className="p-4 grid grid-cols-games gap-3">
      {games.map((g) => (
        <GameCard key={`${g.ssdId}:${g.id}`} game={g} />
      ))}
    </div>
  );
}
