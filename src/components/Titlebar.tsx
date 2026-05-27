export function Titlebar() {
  return (
    <header
      data-tauri-drag-region
      className="h-10 shrink-0 flex items-center px-4 border-b border-neutral-800 bg-neutral-100 dark:bg-neutral-900 select-none"
    >
      <div className="flex items-center gap-2" data-tauri-drag-region>
        <div className="w-3 h-3 rounded-sm bg-accent-blue" />
        <span className="text-sm font-semibold tracking-tight">Steam Vault Sync</span>
      </div>
    </header>
  );
}
