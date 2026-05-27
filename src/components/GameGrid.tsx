import { GameCard } from "./GameCard";
import type { Game } from "@/types";

export function GameGrid({ games }: { games: Game[] }) {
  if (games.length === 0) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-neutral-500">
        Connect your SSD Vault (S:) to see your games.
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
