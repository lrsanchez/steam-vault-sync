import { useCopy } from "@/hooks/useCopy";
import { useLibraryStore } from "@/store/library";
import type { CopyJob } from "@/types";

function formatBytes(bytes: number): string {
  const gb = bytes / 1_073_741_824;
  return `${gb.toFixed(2)} GB`;
}

function formatSpeed(bps: number): string {
  const gbps = bps / 1_073_741_824;
  if (gbps >= 0.1) return `${gbps.toFixed(2)} GB/s`;
  const mbps = bps / 1_048_576;
  return `${mbps.toFixed(1)} MB/s`;
}

function formatEta(seconds: number): string {
  if (seconds <= 0) return "—";
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

export function ProgressPanel({ job }: { job: CopyJob }) {
  const completeActiveCopy = useLibraryStore((s) => s.completeActiveCopy);
  const { pauseCopy, resumeCopy, cancelCopy } = useCopy();
  const pct =
    job.totalBytes > 0
      ? Math.min(100, (job.copiedBytes / job.totalBytes) * 100)
      : 0;

  const isPaused = job.status === "paused";
  const isError = job.status === "error";
  const isActive = job.status === "copying" || job.status === "queued";

  return (
    <div className="shrink-0 border-t border-neutral-800 bg-neutral-100 dark:bg-neutral-900 px-4 py-3">
      <div className="flex items-center justify-between mb-2">
        <div className="text-sm font-medium truncate">
          {job.title}
          {isPaused && (
            <span className="ml-2 text-amber-400 text-xs">Paused</span>
          )}
          {isError && (
            <span className="ml-2 text-red-400 text-xs">
              Failed: {job.error}
            </span>
          )}
        </div>
        <div className="flex items-center gap-3 text-xs text-neutral-400">
          <span>{pct.toFixed(1)}%</span>
          <span>
            {formatBytes(job.copiedBytes)} / {formatBytes(job.totalBytes)}
          </span>
          <span>{formatSpeed(isPaused ? 0 : job.speedBps)}</span>
          <span>ETA {isPaused ? "—" : formatEta(job.etaSeconds)}</span>

          {isActive && (
            <button
              className="btn-ghost text-xs"
              onClick={() => pauseCopy()}
              title="Pause this copy"
            >
              Pause
            </button>
          )}
          {isPaused && (
            <button
              className="btn-ghost text-xs text-accent-green"
              onClick={() => resumeCopy()}
              title="Resume this copy"
            >
              Resume
            </button>
          )}
          {(isActive || isPaused) && (
            <button
              className="btn-ghost text-xs text-red-400 hover:bg-red-500/10"
              onClick={() => cancelCopy()}
              title="Cancel and delete the partial copy"
            >
              Cancel
            </button>
          )}
          {isError && (
            <button
              className="btn-ghost text-xs"
              onClick={() => completeActiveCopy()}
            >
              Dismiss
            </button>
          )}
        </div>
      </div>
      <div className="progress-track">
        <div
          className={
            isPaused
              ? "h-full bg-amber-500/70 transition-[width] duration-300"
              : "progress-fill"
          }
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
