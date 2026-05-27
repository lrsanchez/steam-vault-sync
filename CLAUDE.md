# VaultSync — Claude Code Constitution

## Project overview

VaultSync is a desktop application for managing a personal Steam game vault stored on external SSDs. It allows the user to browse their full game library across multiple SSDs, copy games to the local machine's internal drive, auto-register them into Steam, and remove local copies without ever touching the vault.

## Tech stack

- **Framework**: Tauri 2.x (Rust backend + React/Vite/TypeScript frontend)
- **Frontend**: React 18, Vite, TypeScript, Tailwind CSS
- **Database**: SQLite (one `vaultsync.db` per SSD, stored at the root of each SSD)
- **Steam API**: Used for game metadata (name, cover art, size, AppID)
- **OS target**: Windows only

## Repository structure

```
vaultsync/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs
│   │   ├── commands/
│   │   │   ├── drives.rs       # SSD detection, local Steam library scan
│   │   │   ├── catalog.rs      # Read/write vaultsync.db on SSD
│   │   │   ├── copy.rs         # File copy with progress streaming
│   │   │   ├── steam.rs        # Steam VDF parsing, AppID lookup, registration
│   │   │   └── metadata.rs     # Steam API calls for cover art / game info
│   │   └── lib.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/
│   ├── App.tsx
│   ├── main.tsx
│   ├── components/
│   │   ├── Titlebar.tsx
│   │   ├── Sidebar.tsx
│   │   ├── Toolbar.tsx
│   │   ├── GameGrid.tsx
│   │   ├── GameCard.tsx
│   │   └── ProgressPanel.tsx
│   ├── hooks/
│   │   ├── useLibrary.ts
│   │   ├── useCopy.ts
│   │   └── useSteam.ts
│   ├── store/
│   │   └── library.ts          # Zustand store
│   └── types/
│       └── index.ts
├── CLAUDE.md
└── package.json
```

## Core data model

### `vaultsync.db` schema (lives on each SSD root)

```sql
CREATE TABLE games (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  app_id      TEXT,                    -- Steam AppID (nullable until resolved)
  title       TEXT NOT NULL,
  folder_name TEXT NOT NULL UNIQUE,    -- exact folder name on SSD
  size_gb     REAL,
  cover_url   TEXT,                    -- cached Steam CDN URL
  added_at    DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE ssd_meta (
  key   TEXT PRIMARY KEY,
  value TEXT
);
-- ssd_meta stores: ssd_name, ssd_uuid, last_scanned
```

### In-memory app state (Zustand)

```typescript
interface AppState {
  ssds: SSD[]                  // all known SSDs
  games: Game[]                // merged catalog from all SSDs
  localLibraries: LocalLibrary[] // detected Steam library paths on local machine
  copyQueue: CopyJob[]
  activeCopy: CopyJob | null
  filter: 'all' | 'available' | 'installed'
  searchQuery: string
}

interface SSD {
  id: string                   // uuid from ssd_meta
  name: string                 // user-defined label e.g. "SSD Vault 1"
  driveLetter: string          // e.g. "S"
  connected: boolean
  totalGames: number
}

interface Game {
  id: number
  appId: string | null
  title: string
  folderName: string
  sizeGb: number
  coverUrl: string | null
  ssdId: string
  ssdDriveLetter: string
  isAvailable: boolean         // SSD is currently connected
  isInstalled: boolean         // found in any local Steam library
  installedPath: string | null // which local library path it's in
}

interface LocalLibrary {
  path: string                 // e.g. "C:\Program Files (x86)\Steam\steamapps"
  driveLetter: string
  games: string[]              // folder names present
}

interface CopyJob {
  gameId: number
  title: string
  sourcePath: string
  destLibraryPath: string
  totalBytes: number
  copiedBytes: number
  speedBps: number
  etaSeconds: number
  status: 'queued' | 'copying' | 'done' | 'error'
}
```

## Tauri commands (Rust)

### drives.rs

```rust
// Detect if S: drive is mounted and read its vaultsync.db
#[tauri::command]
async fn scan_vault_ssd(drive_letter: String) -> Result<SSDInfo, String>

// Scan local machine for all Steam library paths via libraryfolders.vdf
// Default VDF path: C:\Program Files (x86)\Steam\steamapps\libraryfolders.vdf
// Parse all libraryfolders entries and return their paths
#[tauri::command]
async fn scan_local_steam_libraries() -> Result<Vec<LocalLibrary>, String>
```

### catalog.rs

```rust
// Read all games from vaultsync.db on the SSD
#[tauri::command]
async fn get_ssd_catalog(drive_letter: String) -> Result<Vec<Game>, String>

// Write/update a game record (used after Steam API metadata fetch)
#[tauri::command]
async fn upsert_game(drive_letter: String, game: GameRecord) -> Result<(), String>

// Scan SSD folder structure and sync new game folders into vaultsync.db
// A game folder is any folder directly under S:\SteamLibrary\steamapps\common\
#[tauri::command]
async fn rescan_ssd(drive_letter: String) -> Result<Vec<Game>, String>
```

### copy.rs

```rust
// Copy game folder from SSD to selected local Steam library path
// Emits progress events: { copied_bytes, total_bytes, speed_bps, eta_seconds }
// Uses chunked read/write (4MB chunks) for speed
#[tauri::command]
async fn copy_game(
  app_handle: tauri::AppHandle,
  source_path: String,          // e.g. S:\SteamLibrary\steamapps\common\Elden Ring
  dest_library_path: String,    // e.g. D:\SteamApps\steamapps\common
  game_title: String,
) -> Result<(), String>

// Remove game from local Steam library (never touches SSD)
#[tauri::command]
async fn remove_local_game(library_path: String, folder_name: String) -> Result<(), String>
```

### steam.rs

```rust
// Parse libraryfolders.vdf to extract all Steam library paths
#[tauri::command]
async fn parse_library_folders_vdf(vdf_path: String) -> Result<Vec<String>, String>

// After copy completes, register game into Steam using steam:// URI
// Runs: steam://install/<app_id> or steam://validate/<app_id>
#[tauri::command]
async fn register_game_in_steam(app_id: String) -> Result<(), String>

// Check which vault games are present in local libraries
// Returns map of folder_name -> installed_path
#[tauri::command]
async fn check_installed_games(
  vault_games: Vec<String>,        // folder names
  local_libraries: Vec<String>,    // library paths
) -> Result<HashMap<String, String>, String>
```

### metadata.rs

```rust
// Fetch game metadata from Steam API
// GET https://store.steampowered.com/api/appdetails?appids=<appid>
// Requires Steam API key in settings
#[tauri::command]
async fn fetch_steam_metadata(app_id: String, api_key: String) -> Result<GameMetadata, String>

// Search Steam for a game by name (to resolve AppID from folder name)
// GET https://api.steampowered.com/ISteamApps/GetAppList/v2/
// Then fuzzy match folder name against app list
#[tauri::command]
async fn resolve_app_id(folder_name: String) -> Result<Option<String>, String>
```

## Frontend components

### GameCard.tsx

- Cover art (Steam CDN) with fallback to game icon placeholder
- SSD badge (colored, e.g. "SSD 1" / "SSD 2")
- "Local" badge when installed
- Game title + size in GB
- **Copy to PC** button → opens library picker if multiple local Steam libraries exist
- **Remove** button (only shown when installed) → confirms then removes local copy
- Grayed out / disabled when SSD is not connected

### ProgressPanel.tsx

- Shown at bottom of app when a copy is active
- Game title, % complete, progress bar
- Speed (GB/s), ETA (seconds remaining), copied/total GB
- Cancel button

### Sidebar.tsx

- All games (total count)
- Per-SSD entries with game count and connected indicator
- Installed on this PC (count)
- Rescan drives action
- Settings link

### Settings page

- Steam API key input (stored in Tauri secure store)
- SSD Vault drive letter (default: S:)
- Default local Steam library path for copies
- App theme (auto / light / dark)

## Key behaviors

### On app startup
1. Check if `S:` is mounted
2. If yes, read `S:\vaultsync.db` → load catalog
3. Scan local machine for Steam library paths via `libraryfolders.vdf`
4. Cross-reference vault game folder names against local library folder names → mark installed games
5. Fetch missing metadata from Steam API in background (non-blocking)

### SSD hotplug
- Poll `S:` drive presence every 3 seconds
- On connect: load catalog, merge into state, update availability flags
- On disconnect: mark all SSD games as unavailable (grayed out), keep in catalog

### Copy flow
1. User clicks "Copy to PC" on an available game
2. If multiple local Steam libraries detected → show picker modal
3. Copy starts: `S:\SteamLibrary\steamapps\common\<folder>` → `<dest>\steamapps\common\<folder>`
4. Progress events stream to frontend every 500ms
5. On completion: call `register_game_in_steam(app_id)` → opens Steam URI
6. Mark game as installed in local state

### Remove flow
1. User clicks remove (trash icon) on an installed game
2. Confirmation dialog: "Remove [title] from this PC? Vault copy is safe."
3. Delete `<local_library>\steamapps\common\<folder>` recursively
4. Mark game as not installed in state

### Catalog rescan
- User can trigger manual rescan via sidebar
- Walks `S:\SteamLibrary\steamapps\common\` for new folders
- New folders added to `vaultsync.db`, queued for metadata fetch
- Removed folders deleted from DB

## SSD folder convention

VaultSync expects games on the SSD to follow standard Steam layout:

```
S:\
  SteamLibrary\
    steamapps\
      common\
        Elden Ring\
        Cyberpunk 2077\
        Red Dead Redemption 2\
        ...
  vaultsync.db
```

If SSD was previously used as a Steam library this structure already exists.

## Error handling

- SSD not found on startup → show "Connect your SSD Vault (S:)" empty state
- Steam API key missing → metadata shows placeholder, user prompted to add key in settings
- Copy failure → show error in progress panel, partial copy cleaned up
- Steam not installed → skip `register_game_in_steam`, show manual instruction

## Build & dev

```bash
# Install dependencies
npm install

# Dev mode (Tauri + Vite HMR)
npm run tauri dev

# Production build
npm run tauri build
```

## Design reference

- Dark/light mode support via Tauri window theme
- Game grid: `repeat(auto-fill, minmax(148px, 1fr))`
- Color accents: blue (#378ADD) for actions, green (#1D9E75) for installed/connected, coral for SSD 2 badge
- SSD 1 badge: purple tint; SSD 2 badge: coral tint
- Progress bar: blue fill on gray track, 4px height
- Font: system-ui / Segoe UI on Windows

## Claude Code notes

- Start with Tauri 2.x scaffold: `npm create tauri-app@latest`
- Use `tauri-plugin-fs` for file system access with Windows path support
- Use `tauri-plugin-shell` for opening Steam URIs
- Use `tauri-plugin-store` for persisting settings (API key, preferences)
- SQLite via `rusqlite` crate
- VDF parsing: use `keyvalues-parser` crate or implement simple recursive parser
- Copy progress: use Tauri event emitter (`app_handle.emit_all`) from async Rust task
- Zustand for frontend state management
- React Query for async metadata fetching
