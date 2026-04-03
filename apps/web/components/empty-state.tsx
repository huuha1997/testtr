"use client";

import { motion } from "framer-motion";
import type { RunState } from "@/lib/hooks/use-run";

export function EmptyState({ status }: { status: RunState["status"] }) {
  if (status === "idle") {
    return (
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        className="flex flex-1 flex-col items-center justify-center gap-4"
      >
        <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-bg-tertiary">
          <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="text-text-tertiary">
            <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
          </svg>
        </div>
        <div className="text-center">
          <p className="text-sm font-medium text-text-secondary">Create a run to get started</p>
          <p className="mt-1 text-xs text-text-tertiary">
            Enter a prompt and we&apos;ll generate mockups, designs, and deploy your project
          </p>
        </div>
      </motion.div>
    );
  }

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      className="flex flex-1 flex-col items-center justify-center gap-4"
    >
      <div className="relative">
        <div className="h-12 w-12 rounded-full border-2 border-accent/30 border-t-accent animate-spin" />
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="h-6 w-6 rounded-full border-2 border-violet-400/30 border-b-violet-400 animate-spin [animation-direction:reverse] [animation-duration:1.5s]" />
        </div>
      </div>
      <p className="text-sm text-text-secondary">
        {status === "mockup_generating" ? "Generating mockups..." : "Processing..."}
      </p>
    </motion.div>
  );
}
