# Steam Vault Sync

> Manage your Steam game library across external SSDs. Browse everything
> on your vault drive from one place, copy what you want to play to your
> PC, launch in Steam, and clean it up when you're done — without ever
> touching the vault copy.

![Steam Vault Sync main UI](docs/screenshot.png)

## Why this exists

If you have a 2 TB+ Steam library that doesn't fit on your internal SSD,
the usual answer is to keep the games on an external drive and play them
from there. That works, but it's slow (USB SSD bandwidth + Steam's I/O
patterns) and the drive has to stay plugged in.

The better answer: treat the external SSD as a **vault** — a complete
read-only mirror of your library — and copy games to your fast internal
SSD only when you want to play. When you're done with a game, remove
the local copy. The vault is untouched and always there for next time.

Steam Vault Sync is the desktop app that makes this workflow nice.

## Features

- **Browse your vault** — cover art, titles, sizes for every game on
  the vault SSD, even when the SSD isn't plugged in. The catalog is
  cached on the SSD itself in a SQLite database (`vaultsync.db`).
- **One-click copy** — pick a destination Steam library on your PC and
  Steam Vault Sync copies the game folder. Progress, speed, and ETA
  shown in real time.
- **Pause / Resume / Cancel** — for the long copies. Pause stops disk
  I/O cleanly at chunk boundaries; cancel deletes the partial copy.
- **Auto-register with Steam** — when a copy completes, the app fires
  `steam://install/<appid>` so Steam picks up the new install
  automatically.
- **Launch directly** — installed games show a Launch Game button
  that opens them via `steam://rungameid/<appid>`.
- **Clean uninstall** — removing a local copy deletes the game folder,
  the `appmanifest_*.acf`, downloading cache, and workshop content —
  so Steam correctly sees the game as not installed.
- **System tray** — close to tray; tray tooltip shows live copy
  progress; right-click for Show / Exit.
- **Multi-library awareness** — sees every Steam library registered
  in `libraryfolders.vdf` and shows a per-drive breakdown of which
  vault games are installed where.
- **Cover art with zero config** — reads Steam's `appmanifest_*.acf`
  files directly off the SSD to map folder → AppID, then pulls covers
  from Steam's CDN. No API key required.
- **Hot-plug detection** — connect or disconnect the vault SSD and
  the UI updates within 3 seconds.

## Quick start (use the prebuilt binary)

1. Grab the latest release from the
   [Releases page](https://github.com/lrsanchez/steam-vault-sync/releases)
   — either the standalone `Steam Vault Sync.exe`, the NSIS
   installer, or the MSI.
2. The app requires **Microsoft Edge WebView2 Runtime**. On Windows 11
   it's pre-installed. On Windows 10 the installer will fetch it
   automatically if missing. For the standalone .exe on a stripped
   machine, install it from
   <https://developer.microsoft.com/microsoft-edge/webview2/>.
3. Run it. The app expects your vault SSD at drive letter **S:** by
   default — you can change this in Settings.

That's it. No Node, no Rust, no npm. The .exe is self-contained.

## Set up your vault SSD

Steam Vault Sync expects the standard Steam library layout on the SSD:

```
S:\
  SteamLibrary\
    steamapps\
      common\
        Elden Ring\
        Cyberpunk 2077\
        ...
      appmanifest_1245620.acf
      appmanifest_1091500.acf
      ...
  vaultsync.db          ← created automatically on first launch
```

The easiest way to get there:

1. Plug in your SSD.
2. In Steam → Settings → Storage → **Add Drive** → pick the SSD. Steam
   creates `SteamLibrary\steamapps\common\` for you.
3. Move or install games onto the SSD via Steam as usual.
4. Launch Steam Vault Sync. The first time you point it at the SSD it
   creates `vaultsync.db` and scans the games automatically.

You can also use an SSD that's been used as a Steam library on
another PC — just plug it in and the app will read the existing
`appmanifest_*.acf` files to identify everything.

## Typical workflow

1. **Browse** — open Steam Vault Sync, see your whole vault.
2. **Pick a game** — click **Copy to PC**. If you have multiple Steam
   libraries on your PC, pick one. Progress panel shows speed + ETA.
3. **Play** — when the copy finishes, Steam pops up and the game is
   ready. Or click the **Launch Game** button right in Steam Vault Sync.
4. **Done?** — click the **✕** on the game card to fully uninstall the
   local copy. The vault copy is untouched.

You can keep multiple games installed locally at once; the sidebar
shows totals per drive.

## Build from source

You need:

- **Node.js** 20+
- **Rust** stable (with the `x86_64-pc-windows-msvc` target)
- **Visual Studio 2022 Build Tools** with the "Desktop development
  with C++" workload (for the MSVC linker)
- **WebView2 Runtime** (almost certainly already installed)

Then:

```powershell
git clone https://github.com/lrsanchez/steam-vault-sync.git
cd steam-vault-sync

npm install

# Dev mode (HMR for React, recompile-on-save for Rust)
npm run tauri dev

# Release build — produces .exe, MSI, and NSIS installer
npm run tauri build
```

Build outputs land in `src-tauri/target/release/`:

- `vaultsync.exe` — the standalone binary
- `bundle/msi/Steam Vault Sync_0.1.0_x64_en-US.msi`
- `bundle/nsis/Steam Vault Sync_0.1.0_x64-setup.exe`

## Tech stack

- **[Tauri 2.x](https://tauri.app/)** — desktop shell, Rust backend +
  WebView2 frontend (much smaller bundles than Electron)
- **React 18 + TypeScript + Vite** — UI
- **Tailwind CSS** — styling
- **Zustand** — frontend state
- **SQLite** (via `rusqlite`) — per-SSD game catalog
- **keyvalues-parser** — Steam's VDF format

## Roadmap ideas

Things I might add (PRs welcome under the
[contribution terms](#contributing)):

- Support for non-Steam games (manual cover art entry)
- SteamGridDB integration for games whose `appmanifest_*.acf` is missing
- Multi-vault support (more than one SSD plugged in at the same time)
- Linux / macOS support (currently Windows-only)
- Bandwidth throttling for copies

## Contributing

Bug reports, feature requests, and small PRs are welcome. By submitting
a PR you agree your contribution is licensed under the same terms as
the rest of the project (see [LICENSE](LICENSE)).

For larger changes please open an issue first to discuss the direction.

## License

**[PolyForm Noncommercial 1.0.0](LICENSE)** — free for personal use,
hobby projects, research, education, and non-profits. Commercial use
(selling, paid hosting, monetized distribution) requires a separate
license — email
[leandrorsanchez@gmail.com](mailto:leandrorsanchez@gmail.com).

## Credits

Created by **Leandro Sanchez** and **Claude**.

Built with [Tauri](https://tauri.app/),
[React](https://react.dev/), and
[Claude Code](https://claude.com/claude-code).

Steam and the Steam logo are trademarks of Valve Corporation. This
project is not affiliated with or endorsed by Valve.
