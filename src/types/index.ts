export interface SSD {
  id: string;
  name: string;
  driveLetter: string;
  connected: boolean;
  totalGames: number;
}

export interface Game {
  id: number;
  appId: string | null;
  title: string;
  folderName: string;
  sizeGb: number;
  coverUrl: string | null;
  ssdId: string;
  ssdDriveLetter: string;
  isAvailable: boolean;
  isInstalled: boolean;
  installedPath: string | null;
}

export interface LocalLibrary {
  path: string;
  driveLetter: string;
  games: string[];
}

export type CopyStatus = "queued" | "copying" | "paused" | "done" | "error" | "cancelled";

export interface CopyJob {
  gameId: number;
  title: string;
  sourcePath: string;
  destLibraryPath: string;
  totalBytes: number;
  copiedBytes: number;
  speedBps: number;
  etaSeconds: number;
  status: CopyStatus;
  error?: string;
}

export type LibraryFilter = "all" | "available" | "installed";

export interface GameRecord {
  appId: string | null;
  title: string;
  folderName: string;
  sizeGb: number;
  coverUrl: string | null;
}

export interface GameMetadata {
  appId: string;
  name: string;
  headerImage: string | null;
  libraryCover: string | null;
  shortDescription: string | null;
}

export interface AppSettings {
  vaultDriveLetter: string;
  defaultLocalLibraryPath: string;
  theme: "auto" | "light" | "dark";
}

export const DEFAULT_SETTINGS: AppSettings = {
  vaultDriveLetter: "S",
  defaultLocalLibraryPath: "",
  theme: "auto",
};
