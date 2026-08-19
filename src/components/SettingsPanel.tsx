import { useEffect, useState } from "react";
import { Check, Save } from "lucide-react";

import { ipc } from "../services/tauri-ipc";

/**
 * Settings (Investment layer). Reads/writes user config persisted in the local SQLite
 * cache, so the app compounds in utility over time.
 */
export function SettingsPanel({ repoPath }: { repoPath: string | null }) {
  const [alias, setAlias] = useState("");
  const [filter, setFilter] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    ipc.getConfig("repo_alias").then((v) => setAlias(v ?? "")).catch(() => undefined);
    ipc.getConfig("timeline_filter").then((v) => setFilter(v ?? "")).catch(() => undefined);
  }, [repoPath]);

  const save = async () => {
    await ipc.setConfig("repo_alias", alias).catch(() => undefined);
    await ipc.setConfig("timeline_filter", filter).catch(() => undefined);
    setSaved(true);
    window.setTimeout(() => setSaved(false), 1500);
  };

  return (
    <div className="mx-auto max-w-xl space-y-6 p-6">
      <Field
        label="Repository alias"
        hint="A friendly name shown across the dashboard."
        value={alias}
        onChange={setAlias}
        placeholder="e.g. Payments Service"
      />
      <Field
        label="Timeline author filter"
        hint="Comma-separated emails to de-emphasize (bots, CI). Persisted locally."
        value={filter}
        onChange={setFilter}
        placeholder="e.g. bot@ci.example.com"
      />

      <div className="flex items-center gap-3">
        <button
          onClick={save}
          className="flex items-center gap-2 rounded-md bg-emerald-500 px-4 py-2 text-sm font-medium text-emerald-950 hover:bg-emerald-400"
        >
          {saved ? <Check className="h-4 w-4" /> : <Save className="h-4 w-4" />}
          {saved ? "Saved" : "Save settings"}
        </button>
        <span className="text-xs text-zinc-500">Stored in the local SQLite cache.</span>
      </div>
    </div>
  );
}

function Field({
  label,
  hint,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  hint: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  return (
    <label className="block">
      <span className="text-sm font-medium text-zinc-200">{label}</span>
      <span className="mt-0.5 block text-xs text-zinc-500">{hint}</span>
      <input
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.currentTarget.value)}
        className="mt-2 w-full rounded-md border border-zinc-700 bg-zinc-900 px-3 py-2 text-sm text-zinc-100 outline-none placeholder:text-zinc-600 focus:border-emerald-500"
      />
    </label>
  );
}
