"use client";

import { motion, AnimatePresence } from "framer-motion";
import type { RunHistoryEntry } from "@/lib/hooks/use-history";

const statusDot: Record<string, string> = {
  done: "bg-success",
  error: "bg-error",
  failed_retryable: "bg-error",
  failed_final: "bg-error",
  cancelled: "bg-text-tertiary",
};

function formatTime(iso: string) {
  try {
    const d = new Date(iso);
    const now = new Date();
    const diff = now.getTime() - d.getTime();
    if (diff < 60_000) return "just now";
    if (diff < 3600_000) return `${Math.floor(diff / 60_000)}m ago`;
    if (diff < 86400_000) return `${Math.floor(diff / 3600_000)}h ago`;
    return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
  } catch {
    return "";
  }
}

interface RunListProps {
  entries: RunHistoryEntry[];
  activeRunId?: string;
  onSelect: (runId: string) => void;
  onRemove: (runId: string) => void;
}

export function RunList({ entries, activeRunId, onSelect, onRemove }: RunListProps) {
  if (entries.length === 0) {
    return (
      <p className="py-4 text-center text-xs text-text-tertiary">
        No runs yet
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-0.5">
      <AnimatePresence initial={false}>
        {entries.map((entry) => {
          const isActive = entry.runId === activeRunId;
          const dotColor = statusDot[entry.status] ?? "bg-accent";
          const isProcessing = !["done", "error", "failed_retryable", "failed_final", "cancelled", "draft"].includes(entry.status);

          return (
            <motion.div
              key={entry.runId}
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              onClick={() => onSelect(entry.runId)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => { if (e.key === "Enter") onSelect(entry.runId); }}
              className={`group relative flex cursor-pointer flex-col gap-1 rounded-lg px-3 py-2.5 text-left transition-colors ${
                isActive
                  ? "bg-accent/10 border border-accent/20"
                  : "hover:bg-bg-hover border border-transparent"
              }`}
            >
              {/* Remove button */}
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onRemove(entry.runId);
                }}
                className="absolute right-2 top-2 hidden rounded p-0.5 text-text-tertiary hover:bg-bg-active hover:text-text-secondary group-hover:block"
              >
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>

              {/* Prompt */}
              <span className="line-clamp-2 text-xs text-text-primary leading-relaxed pr-4">
                {entry.prompt}
              </span>

              {/* Meta row */}
              <div className="flex items-center gap-2">
                {/* Status dot */}
                <span className="relative flex h-2 w-2 shrink-0">
                  {isProcessing && (
                    <span className={`animate-ping absolute inline-flex h-full w-full rounded-full opacity-75 ${dotColor}`} />
                  )}
                  <span className={`relative inline-flex h-2 w-2 rounded-full ${dotColor}`} />
                </span>

                <span className="text-[10px] text-text-tertiary font-mono">
                  {entry.runId.slice(0, 8)}
                </span>

                <span className="text-[10px] text-text-tertiary ml-auto">
                  {formatTime(entry.createdAt)}
                </span>
              </div>

              {/* URLs */}
              {entry.previewUrl && (
                <a
                  href={entry.previewUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  onClick={(e) => e.stopPropagation()}
                  className="inline-flex items-center gap-1 text-[10px] text-success hover:underline truncate"
                >
                  <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                    <polyline points="15 3 21 3 21 9" />
                    <line x1="10" y1="14" x2="21" y2="3" />
                  </svg>
                  {entry.previewUrl.replace(/^https?:\/\//, "")}
                </a>
              )}
            </motion.div>
          );
        })}
      </AnimatePresence>
    </div>
  );
}
