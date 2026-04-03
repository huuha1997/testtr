"use client";

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import type { RunState } from "@/lib/hooks/use-run";

interface ActionCardsProps {
  state: RunState;
  onApproveStitch: () => void;
  onSelectStack: (stackId: string) => void;
  onApproveDeploy: () => void;
  onRejectDeploy: () => void;
}

function CardWrapper({
  children,
  borderColor,
}: {
  children: React.ReactNode;
  borderColor: string;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, y: -12, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, y: -8, scale: 0.98 }}
      transition={{ duration: 0.3, ease: [0.25, 0.46, 0.45, 0.94] }}
      className={`rounded-xl border ${borderColor} bg-bg-secondary p-4`}
    >
      {children}
    </motion.div>
  );
}

function ActionButton({
  children,
  onClick,
  variant = "primary",
  disabled,
}: {
  children: React.ReactNode;
  onClick: () => void;
  variant?: "primary" | "secondary" | "danger";
  disabled?: boolean;
}) {
  const styles = {
    primary: "bg-accent text-white hover:bg-accent-hover",
    secondary: "border border-border-primary bg-bg-tertiary text-text-secondary hover:bg-bg-hover hover:text-text-primary",
    danger: "border border-error/30 bg-error-muted text-error hover:bg-error/20",
  };

  return (
    <motion.button
      whileHover={{ scale: 1.02 }}
      whileTap={{ scale: 0.97 }}
      onClick={onClick}
      disabled={disabled}
      className={`rounded-lg px-4 py-2 text-sm font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${styles[variant]}`}
    >
      {children}
    </motion.button>
  );
}

export function ActionCards({
  state,
  onApproveStitch,
  onSelectStack,
  onApproveDeploy,
  onRejectDeploy,
}: ActionCardsProps) {
  const [stackId, setStackId] = useState("nextjs-tailwind");
  const { status, stitch, previewUrl, prUrl, loading } = state;

  return (
    <AnimatePresence mode="wait">
      {/* Mockup Ready Banner */}
      {status === "mockup_ready" && (
        <CardWrapper key="mockup-ready" borderColor="border-warning/40">
          <div className="flex items-center gap-3">
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-warning-muted">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-warning">
                <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
                <circle cx="8.5" cy="8.5" r="1.5" />
                <polyline points="21 15 16 10 5 21" />
              </svg>
            </div>
            <div>
              <p className="text-sm font-medium text-text-primary">Mockups are ready</p>
              <p className="text-xs text-text-secondary">Select a variant below to continue</p>
            </div>
          </div>
        </CardWrapper>
      )}

      {/* Stitch Generating */}
      {(status === "mockup_selected" || status === "stitch_generating") && (
        <CardWrapper key="stitch-gen" borderColor="border-violet-500/40">
          <div className="flex items-center gap-3">
            <div className="h-4 w-4 rounded-full border-2 border-violet-400 border-t-transparent animate-spin" />
            <p className="text-sm text-violet-300">Generating design from selected mockup...</p>
          </div>
        </CardWrapper>
      )}

      {/* Stitch Ready — Approve */}
      {status === "stitch_ready" && (
        <CardWrapper key="stitch-ready" borderColor="border-accent/40">
          <div className="flex flex-col gap-3">
            <div className="flex items-center gap-3">
              <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-accent-muted">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-accent-hover">
                  <path d="M12 20h9" />
                  <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
                </svg>
              </div>
              <div>
                <p className="text-sm font-medium text-text-primary">Design Ready</p>
                <p className="text-xs text-text-secondary">Review the generated design and approve</p>
              </div>
            </div>

            {stitch?.designUrl && (
              <a
                href={stitch.designUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 text-xs text-accent-hover hover:underline"
              >
                {stitch.designUrl}
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                  <polyline points="15 3 21 3 21 9" />
                  <line x1="10" y1="14" x2="21" y2="3" />
                </svg>
              </a>
            )}

            {stitch?.text && (
              <p className="rounded-lg bg-bg-primary p-3 text-xs leading-relaxed text-text-secondary line-clamp-4">
                {stitch.text}
              </p>
            )}

            <div className="flex gap-2">
              <ActionButton onClick={onApproveStitch} disabled={loading}>
                Approve Design
              </ActionButton>
              {stitch?.designUrl && (
                <ActionButton
                  variant="secondary"
                  onClick={() => window.open(stitch.designUrl, "_blank")}
                >
                  Open in Browser
                </ActionButton>
              )}
            </div>
          </div>
        </CardWrapper>
      )}

      {/* Stack Selection */}
      {status === "stitch_approved" && (
        <CardWrapper key="stack-select" borderColor="border-info/40">
          <div className="flex flex-col gap-3">
            <div className="flex items-center gap-3">
              <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-info-muted">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-info">
                  <polyline points="16 18 22 12 16 6" />
                  <polyline points="8 6 2 12 8 18" />
                </svg>
              </div>
              <div>
                <p className="text-sm font-medium text-text-primary">Select Tech Stack</p>
                <p className="text-xs text-text-secondary">Choose the stack to generate code with</p>
              </div>
            </div>
            <div className="flex gap-2">
              <input
                type="text"
                value={stackId}
                onChange={(e) => setStackId(e.target.value)}
                className="flex-1 rounded-lg border border-border-primary bg-bg-primary px-3 py-2 text-sm text-text-primary focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent/50"
              />
              <ActionButton onClick={() => onSelectStack(stackId)} disabled={loading}>
                Start Build
              </ActionButton>
            </div>
          </div>
        </CardWrapper>
      )}

      {/* Processing states */}
      {(status === "spec_generating" ||
        status === "codegen_running" ||
        status === "ci_running" ||
        status === "pr_ready" ||
        status === "prod_deploying" ||
        status === "stack_selected" ||
        status === "contract_locked") && (
        <CardWrapper key="processing" borderColor="border-border-secondary">
          <div className="flex items-center gap-3">
            <div className="h-4 w-4 rounded-full border-2 border-accent border-t-transparent animate-spin" />
            <p className="text-sm text-text-secondary">
              {status === "spec_generating" && "Generating specification..."}
              {status === "codegen_running" && "Writing code..."}
              {status === "ci_running" && "Running quality gates..."}
              {status === "pr_ready" && "Pull request created"}
              {status === "prod_deploying" && "Deploying to production..."}
              {(status === "stack_selected" || status === "contract_locked") && "Processing..."}
            </p>
          </div>
        </CardWrapper>
      )}

      {/* Deploy Approval */}
      {(status === "preview_deployed" || status === "awaiting_approval") && (
        <CardWrapper key="deploy-approval" borderColor="border-success/40">
          <div className="flex flex-col gap-3">
            <div className="flex items-center gap-3">
              <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-success-muted">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-success">
                  <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                  <polyline points="22 4 12 14.01 9 11.01" />
                </svg>
              </div>
              <div>
                <p className="text-sm font-medium text-text-primary">Ready for Review</p>
                <p className="text-xs text-text-secondary">Preview is live — approve to ship to production</p>
              </div>
            </div>

            {(previewUrl || prUrl) && (
              <div className="flex flex-col gap-1">
                {previewUrl && (
                  <a
                    href={previewUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-1 text-xs text-success hover:underline"
                  >
                    Preview: {previewUrl}
                    <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                      <polyline points="15 3 21 3 21 9" />
                      <line x1="10" y1="14" x2="21" y2="3" />
                    </svg>
                  </a>
                )}
                {prUrl && (
                  <a
                    href={prUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-1 text-xs text-info hover:underline"
                  >
                    PR: {prUrl}
                    <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                      <polyline points="15 3 21 3 21 9" />
                      <line x1="10" y1="14" x2="21" y2="3" />
                    </svg>
                  </a>
                )}
              </div>
            )}

            <div className="flex gap-2">
              <ActionButton onClick={onApproveDeploy} disabled={loading}>
                Approve Deploy
              </ActionButton>
              <ActionButton variant="danger" onClick={onRejectDeploy} disabled={loading}>
                Reject
              </ActionButton>
            </div>
          </div>
        </CardWrapper>
      )}

      {/* Done */}
      {status === "done" && (
        <CardWrapper key="done" borderColor="border-success/40">
          <div className="flex items-center gap-3">
            <div className="flex h-8 w-8 items-center justify-center rounded-full bg-success-muted">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="text-success">
                <polyline points="20 6 9 17 4 12" />
              </svg>
            </div>
            <div>
              <p className="text-sm font-medium text-success">Deployment Complete</p>
              <p className="text-xs text-text-secondary">Your project is live in production</p>
            </div>
          </div>
        </CardWrapper>
      )}

      {/* Error */}
      {(status === "error" || status === "failed_retryable" || status === "failed_final") && state.error && (
        <CardWrapper key="error" borderColor="border-error/40">
          <div className="flex items-center gap-3">
            <div className="flex h-8 w-8 items-center justify-center rounded-full bg-error-muted">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-error">
                <circle cx="12" cy="12" r="10" />
                <line x1="15" y1="9" x2="9" y2="15" />
                <line x1="9" y1="9" x2="15" y2="15" />
              </svg>
            </div>
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium text-error">Error</p>
              <p className="truncate text-xs text-text-secondary">{state.error}</p>
            </div>
          </div>
        </CardWrapper>
      )}
    </AnimatePresence>
  );
}
