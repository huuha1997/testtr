"use client";

import { StatusBadge } from "./status-badge";
import type { RunState } from "@/lib/hooks/use-run";

export function Header({ status, runId }: { status: RunState["status"]; runId?: string }) {
  return (
    <header className="flex h-14 shrink-0 items-center justify-between border-b border-border-primary bg-bg-secondary px-6">
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-2">
          <div className="h-6 w-6 rounded-md bg-accent flex items-center justify-center">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="text-white">
              <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
            </svg>
          </div>
          <span className="text-sm font-semibold tracking-tight">Agentic</span>
        </div>
        {runId && (
          <span className="text-xs text-text-tertiary font-mono ml-2">
            {runId.slice(0, 8)}
          </span>
        )}
      </div>
      <StatusBadge status={status} />
    </header>
  );
}
