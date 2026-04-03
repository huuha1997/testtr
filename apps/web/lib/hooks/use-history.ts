"use client";

import { useCallback, useEffect, useState } from "react";

export interface RunHistoryEntry {
  runId: string;
  prompt: string;
  status: string;
  createdAt: string;
  previewUrl?: string;
  prUrl?: string;
}

const STORAGE_KEY = "agentic_run_history";
const MAX_ENTRIES = 50;

function load(): RunHistoryEntry[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function save(entries: RunHistoryEntry[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(entries.slice(0, MAX_ENTRIES)));
}

export function useHistory() {
  const [entries, setEntries] = useState<RunHistoryEntry[]>([]);

  useEffect(() => {
    setEntries(load());
  }, []);

  const addRun = useCallback((runId: string, prompt: string) => {
    setEntries((prev) => {
      const exists = prev.find((e) => e.runId === runId);
      if (exists) return prev;
      const next = [
        { runId, prompt, status: "draft", createdAt: new Date().toISOString() },
        ...prev,
      ];
      save(next);
      return next;
    });
  }, []);

  const updateRun = useCallback((runId: string, updates: Partial<Omit<RunHistoryEntry, "runId">>) => {
    setEntries((prev) => {
      const next = prev.map((e) =>
        e.runId === runId ? { ...e, ...updates } : e,
      );
      save(next);
      return next;
    });
  }, []);

  const removeRun = useCallback((runId: string) => {
    setEntries((prev) => {
      const next = prev.filter((e) => e.runId !== runId);
      save(next);
      return next;
    });
  }, []);

  return { entries, addRun, updateRun, removeRun };
}
