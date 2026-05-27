import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { AppSettings } from "@/types";

interface SettingsProps {
  settings: AppSettings;
  onChange: (next: AppSettings) => void | Promise<void>;
}

export function Settings({ settings, onChange }: SettingsProps) {
  const [draft, setDraft] = useState<AppSettings>(settings);
  const [saving, setSaving] = useState(false);

  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) =>
    setDraft((prev) => ({ ...prev, [key]: value }));

  const browseLibrary = async () => {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string") {
      update("defaultLocalLibraryPath", picked);
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      await onChange(draft);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex-1 overflow-auto p-6">
      <h1 className="text-lg font-semibold mb-4">Settings</h1>

      <div className="max-w-xl space-y-5">
        <Field label="SSD Vault drive letter">
          <input
            type="text"
            maxLength={1}
            value={draft.vaultDriveLetter}
            onChange={(e) =>
              update("vaultDriveLetter", e.target.value.toUpperCase())
            }
            className="input w-20"
          />
        </Field>

        <Field label="Default local Steam library path">
          <div className="flex gap-2">
            <input
              type="text"
              value={draft.defaultLocalLibraryPath}
              onChange={(e) => update("defaultLocalLibraryPath", e.target.value)}
              className="input flex-1"
              placeholder="C:\Program Files (x86)\Steam\steamapps"
            />
            <button className="btn-ghost" onClick={browseLibrary}>
              Browse…
            </button>
          </div>
        </Field>

        <Field label="Theme">
          <select
            value={draft.theme}
            onChange={(e) =>
              update("theme", e.target.value as AppSettings["theme"])
            }
            className="input"
          >
            <option value="auto">Auto</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </Field>

        <div className="pt-2">
          <button className="btn-primary" onClick={save} disabled={saving}>
            {saving ? "Saving…" : "Save settings"}
          </button>
        </div>

        <div className="mt-10 pt-5 border-t border-neutral-800 text-xs text-neutral-500 space-y-1">
          <div className="font-medium text-neutral-300">Steam Vault Sync</div>
          <div>Version 0.3.0</div>
          <div>Created by Leandro Sanchez and Claude</div>
        </div>
      </div>

      <style>{`
        .input {
          background: rgb(23 23 23);
          border: 1px solid rgb(38 38 38);
          border-radius: 0.25rem;
          padding: 0.375rem 0.625rem;
          font-size: 0.875rem;
          color: inherit;
          outline: none;
        }
        .input:focus { border-color: #378ADD; }
      `}</style>
    </div>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1">
      <label className="text-sm font-medium">{label}</label>
      {hint && <div className="text-xs text-neutral-500">{hint}</div>}
      {children}
    </div>
  );
}
