"use client";

import { useState } from "react";
import { motion } from "framer-motion";
import { PipelineSteps } from "./pipeline-steps";
import { RunList } from "./run-list";
import type { RunState } from "@/lib/hooks/use-run";
import type { RunHistoryEntry } from "@/lib/hooks/use-history";

type Tab = "pipeline" | "history";

interface SidebarProps {
  state: RunState;
  onCreateRun: (prompt: string) => void;
  loading: boolean;
  history: RunHistoryEntry[];
  onSelectRun: (runId: string) => void;
  onRemoveRun: (runId: string) => void;
}

export function Sidebar({ state, onCreateRun, loading, history, onSelectRun, onRemoveRun }: SidebarProps) {
  const [prompt, setPrompt] = useState("Build a modern SaaS landing page for a design studio");
  const [tab, setTab] = useState<Tab>("pipeline");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!prompt.trim() || loading) return;
    onCreateRun(prompt.trim());
  };

  return (
    <aside className="flex w-80 shrink-0 flex-col border-r border-border-primary bg-bg-secondary">
      {/* Prompt Section */}
      <div className="flex flex-col gap-3 border-b border-border-primary p-4">
        <label className="text-[10px] font-semibold uppercase tracking-widest text-text-tertiary">
          Prompt
        </label>
        <form onSubmit={handleSubmit} className="flex flex-col gap-3">
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder="Describe what you want to build..."
            rows={3}
            className="resize-none rounded-lg border border-border-primary bg-bg-primary px-3 py-2.5 text-sm text-text-primary placeholder:text-text-tertiary focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent/50 transition-colors"
          />
          <motion.button
            type="submit"
            disabled={loading || !prompt.trim()}
            whileHover={{ scale: 1.01 }}
            whileTap={{ scale: 0.98 }}
            className="relative flex items-center justify-center gap-2 rounded-lg bg-accent px-4 py-2.5 text-sm font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-40 disabled:cursor-not-allowed overflow-hidden"
          >
            {loading && (
              <motion.div
                className="absolute inset-0 bg-gradient-to-r from-transparent via-white/10 to-transparent"
                animate={{ x: ["-100%", "100%"] }}
                transition={{ duration: 1.5, repeat: Infinity, ease: "linear" }}
              />
            )}
            {loading ? (
              <>
                <svg className="h-4 w-4 animate-spin" viewBox="0 0 24 24" fill="none">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                </svg>
                Creating...
              </>
            ) : (
              <>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <line x1="12" y1="5" x2="12" y2="19" />
                  <line x1="5" y1="12" x2="19" y2="12" />
                </svg>
                Create Run
              </>
            )}
          </motion.button>
        </form>
      </div>

      {/* Tab Switcher */}
      <div className="flex border-b border-border-primary">
        <button
          onClick={() => setTab("pipeline")}
          className={`flex-1 py-2.5 text-[10px] font-semibold uppercase tracking-widest transition-colors ${
            tab === "pipeline"
              ? "text-text-primary border-b-2 border-accent"
              : "text-text-tertiary hover:text-text-secondary"
          }`}
        >
          Pipeline
        </button>
        <button
          onClick={() => setTab("history")}
          className={`flex-1 py-2.5 text-[10px] font-semibold uppercase tracking-widest transition-colors relative ${
            tab === "history"
              ? "text-text-primary border-b-2 border-accent"
              : "text-text-tertiary hover:text-text-secondary"
          }`}
        >
          History
          {history.length > 0 && (
            <span className="ml-1.5 inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-bg-tertiary px-1 text-[9px] font-medium text-text-tertiary">
              {history.length}
            </span>
          )}
        </button>
      </div>

      {/* Tab Content */}
      <div className="flex flex-1 flex-col overflow-hidden">
        {tab === "pipeline" && (
          <div className="flex flex-1 flex-col overflow-hidden px-4 py-3">
            {/* Run Info */}
            {state.run && (
              <div className="mb-3 rounded-lg bg-bg-primary px-3 py-2 border border-border-primary">
                <span className="text-[10px] text-text-tertiary font-mono">
                  {state.run.id}
                </span>
              </div>
            )}
            <div className="flex-1 overflow-y-auto">
              <PipelineSteps steps={state.steps} status={state.status} />
            </div>
          </div>
        )}

        {tab === "history" && (
          <div className="flex-1 overflow-y-auto px-2 py-2">
            <RunList
              entries={history}
              activeRunId={state.run?.id}
              onSelect={onSelectRun}
              onRemove={onRemoveRun}
            />
          </div>
        )}
      </div>
    </aside>
  );
}
