"use client";

import { useEffect, useRef } from "react";
import { motion, AnimatePresence } from "framer-motion";

interface LogEntry {
  time: string;
  text: string;
}

export function LogPanel({ logs }: { logs: LogEntry[] }) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs.length]);

  return (
    <div className="flex flex-col rounded-xl border border-border-primary bg-bg-secondary">
      <div className="flex items-center justify-between border-b border-border-primary px-4 py-2">
        <span className="text-[10px] font-semibold uppercase tracking-widest text-text-tertiary">
          Activity Log
        </span>
        <span className="text-[10px] text-text-tertiary">{logs.length} events</span>
      </div>
      <div className="h-44 overflow-y-auto p-3 font-mono text-[11px] leading-relaxed">
        {logs.length === 0 && (
          <p className="py-4 text-center text-text-tertiary">Waiting for events...</p>
        )}
        <AnimatePresence initial={false}>
          {logs.map((entry, i) => {
            const isError = entry.text.includes("FAILED") || entry.text.includes("ERROR");
            const isSuccess = entry.text.includes("COMPLETED") || entry.text.includes("passed");
            const isState = entry.text.startsWith("state →");

            return (
              <motion.div
                key={i}
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.15 }}
                className={`flex gap-2 rounded px-1.5 py-0.5 ${
                  isError
                    ? "text-error"
                    : isSuccess
                      ? "text-success"
                      : isState
                        ? "text-accent-hover"
                        : "text-text-secondary"
                }`}
              >
                <span className="shrink-0 text-text-tertiary">{entry.time}</span>
                <span className="break-all">{entry.text}</span>
              </motion.div>
            );
          })}
        </AnimatePresence>
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
