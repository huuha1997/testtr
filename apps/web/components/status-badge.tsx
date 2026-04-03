"use client";

import { motion, AnimatePresence } from "framer-motion";
import type { RunState } from "@/lib/hooks/use-run";

const statusConfig: Record<string, { label: string; color: string; bg: string; pulse?: boolean }> = {
  idle: { label: "Ready", color: "text-text-tertiary", bg: "bg-bg-tertiary" },
  connecting: { label: "Connecting...", color: "text-info", bg: "bg-info-muted", pulse: true },
  draft: { label: "Draft", color: "text-text-secondary", bg: "bg-bg-tertiary" },
  mockup_generating: { label: "Generating Mockups", color: "text-violet-400", bg: "bg-violet-500/10", pulse: true },
  mockup_ready: { label: "Mockups Ready", color: "text-warning", bg: "bg-warning-muted" },
  mockup_selected: { label: "Mockup Selected", color: "text-accent-hover", bg: "bg-accent-muted", pulse: true },
  stitch_generating: { label: "Generating Design", color: "text-violet-400", bg: "bg-violet-500/10", pulse: true },
  stitch_ready: { label: "Design Ready", color: "text-accent-hover", bg: "bg-accent-muted" },
  stitch_approved: { label: "Design Approved", color: "text-info", bg: "bg-info-muted" },
  stack_selected: { label: "Building", color: "text-info", bg: "bg-info-muted", pulse: true },
  contract_locked: { label: "Processing", color: "text-info", bg: "bg-info-muted", pulse: true },
  spec_generating: { label: "Generating Spec", color: "text-violet-400", bg: "bg-violet-500/10", pulse: true },
  codegen_running: { label: "Writing Code", color: "text-cyan-400", bg: "bg-cyan-500/10", pulse: true },
  ci_running: { label: "Running CI", color: "text-amber-400", bg: "bg-amber-500/10", pulse: true },
  pr_ready: { label: "PR Created", color: "text-success", bg: "bg-success-muted" },
  preview_deployed: { label: "Preview Live", color: "text-success", bg: "bg-success-muted" },
  awaiting_approval: { label: "Awaiting Approval", color: "text-success", bg: "bg-success-muted" },
  prod_deploying: { label: "Deploying", color: "text-success", bg: "bg-success-muted", pulse: true },
  done: { label: "Complete", color: "text-success", bg: "bg-success-muted" },
  error: { label: "Error", color: "text-error", bg: "bg-error-muted" },
  failed_retryable: { label: "Failed", color: "text-error", bg: "bg-error-muted" },
  failed_final: { label: "Failed", color: "text-error", bg: "bg-error-muted" },
  cancelled: { label: "Cancelled", color: "text-text-tertiary", bg: "bg-bg-tertiary" },
};

export function StatusBadge({ status }: { status: RunState["status"] }) {
  const config = statusConfig[status] ?? statusConfig.idle;

  return (
    <AnimatePresence mode="wait">
      <motion.div
        key={status}
        initial={{ opacity: 0, scale: 0.9 }}
        animate={{ opacity: 1, scale: 1 }}
        exit={{ opacity: 0, scale: 0.9 }}
        transition={{ duration: 0.2 }}
        className={`inline-flex items-center gap-2 rounded-full px-3 py-1.5 text-xs font-medium ${config.bg} ${config.color}`}
      >
        {config.pulse && (
          <span className="relative flex h-2 w-2">
            <span className={`animate-ping absolute inline-flex h-full w-full rounded-full opacity-75 ${config.color === "text-error" ? "bg-error" : config.color === "text-success" ? "bg-success" : config.color === "text-warning" ? "bg-warning" : "bg-accent"}`} />
            <span className={`relative inline-flex h-2 w-2 rounded-full ${config.color === "text-error" ? "bg-error" : config.color === "text-success" ? "bg-success" : config.color === "text-warning" ? "bg-warning" : "bg-accent"}`} />
          </span>
        )}
        {!config.pulse && (
          <span className={`h-2 w-2 rounded-full ${config.color === "text-error" ? "bg-error" : config.color === "text-success" ? "bg-success" : config.color === "text-warning" ? "bg-warning" : config.color === "text-text-tertiary" ? "bg-text-tertiary" : "bg-accent"}`} />
        )}
        {config.label}
      </motion.div>
    </AnimatePresence>
  );
}
