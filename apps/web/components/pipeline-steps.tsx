"use client";

import { motion, AnimatePresence } from "framer-motion";
import { PIPELINE_STEPS } from "@/lib/types";
import type { RunState } from "@/lib/hooks/use-run";

const stepLabels: Record<string, string> = {
  mockup_generation: "Generate Mockups",
  mockup_selection: "Select Mockup",
  stitch_generation: "Generate Design",
  stitch_approval: "Approve Design",
  spec_generation: "Generate Spec",
  codegen: "Write Code",
  ci_gate_lint: "Lint",
  ci_gate_typecheck: "Type Check",
  ci_gate_build: "Build",
  ci_gate_e2e: "E2E Tests",
  ci_gate_visual: "Visual Tests",
  ci_gate_a11y: "Accessibility",
  pr_create: "Create PR",
  preview_deploy: "Deploy Preview",
  deploy_approval: "Deploy Approval",
  self_heal: "Self Heal",
};

function StepIcon({ status }: { status: string }) {
  if (status === "completed" || status === "passed") {
    return (
      <motion.div
        initial={{ scale: 0 }}
        animate={{ scale: 1 }}
        className="flex h-5 w-5 items-center justify-center rounded-full bg-success/20"
      >
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" className="text-success">
          <polyline points="20 6 9 17 4 12" />
        </svg>
      </motion.div>
    );
  }

  if (status === "failed" || status === "failed_final") {
    return (
      <motion.div
        initial={{ scale: 0 }}
        animate={{ scale: 1 }}
        className="flex h-5 w-5 items-center justify-center rounded-full bg-error/20"
      >
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" className="text-error">
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </motion.div>
    );
  }

  if (status === "retrying") {
    return (
      <div className="flex h-5 w-5 items-center justify-center rounded-full bg-warning/20">
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="text-warning animate-spin">
          <polyline points="23 4 23 10 17 10" />
          <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
        </svg>
      </div>
    );
  }

  // Running / in-progress
  if (status && status !== "waiting") {
    return (
      <div className="flex h-5 w-5 items-center justify-center">
        <div className="h-4 w-4 rounded-full border-2 border-accent border-t-transparent animate-spin" />
      </div>
    );
  }

  // Waiting / pending
  return (
    <div className="flex h-5 w-5 items-center justify-center">
      <div className="h-2 w-2 rounded-full bg-border-secondary" />
    </div>
  );
}

export function PipelineSteps({
  steps,
  status,
}: {
  steps: Record<string, string>;
  status: RunState["status"];
}) {
  const hasSteps = Object.keys(steps).length > 0;

  if (!hasSteps && status === "idle") {
    return (
      <p className="py-8 text-center text-xs text-text-tertiary">
        Create a run to start the pipeline
      </p>
    );
  }

  if (!hasSteps) {
    // Show all pipeline steps as "waiting" instead of skeleton
    return (
      <div className="flex flex-col">
        {PIPELINE_STEPS.map((key) => (
          <div
            key={key}
            className="flex items-center gap-2.5 rounded-md px-2 py-1.5 text-xs opacity-40"
          >
            <div className="flex h-5 w-5 items-center justify-center">
              <div className="h-2 w-2 rounded-full bg-border-secondary" />
            </div>
            <span className="text-text-tertiary">{stepLabels[key] ?? key}</span>
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className="flex flex-col">
      {PIPELINE_STEPS.map((key) => {
        const stepStatus = steps[key];

        if (!stepStatus) {
          // Show as pending/inactive
          return (
            <div
              key={key}
              className="flex items-center gap-2.5 rounded-md px-2 py-1.5 text-xs opacity-30"
            >
              <div className="flex h-5 w-5 items-center justify-center">
                <div className="h-2 w-2 rounded-full bg-border-secondary" />
              </div>
              <span className="text-text-tertiary">{stepLabels[key] ?? key}</span>
            </div>
          );
        }

        return (
          <motion.div
            key={key}
            initial={{ opacity: 0, x: -12 }}
            animate={{ opacity: 1, x: 0 }}
            className="flex items-center gap-2.5 rounded-md px-2 py-1.5 text-xs"
          >
            <StepIcon status={stepStatus} />
            <span className="text-text-primary font-medium">{stepLabels[key] ?? key}</span>
            <span className="ml-auto text-[10px] text-text-tertiary font-mono">
              {stepStatus}
            </span>
          </motion.div>
        );
      })}

      {/* Extra steps not in standard order */}
      {Object.entries(steps)
        .filter(([k]) => !(PIPELINE_STEPS as readonly string[]).includes(k))
        .map(([key, stepStatus]) => (
          <motion.div
            key={key}
            initial={{ opacity: 0, x: -12 }}
            animate={{ opacity: 1, x: 0 }}
            className="flex items-center gap-2.5 rounded-md px-2 py-1.5 text-xs"
          >
            <StepIcon status={stepStatus} />
            <span className="text-text-primary font-medium">{key}</span>
            <span className="ml-auto text-[10px] text-text-tertiary font-mono">{stepStatus}</span>
          </motion.div>
        ))}
    </div>
  );
}
