export type RunStatus =
  | "draft"
  | "mockup_generating"
  | "mockup_ready"
  | "mockup_selected"
  | "stitch_generating"
  | "stitch_ready"
  | "stitch_approved"
  | "stack_selected"
  | "contract_locked"
  | "spec_generating"
  | "codegen_running"
  | "ci_running"
  | "pr_ready"
  | "preview_deployed"
  | "awaiting_approval"
  | "prod_deploying"
  | "done"
  | "failed_retryable"
  | "failed_final"
  | "cancelled";

export interface Run {
  id: string;
  status: RunStatus;
  created_at: string;
}

export interface RunStep {
  step_key: string;
  status: string;
  detail?: string;
  updated_at: string;
}

export interface RunTimelineItem {
  at: string;
  kind: string;
  message: string;
}

export type SseEvent =
  | { type: "heartbeat"; at: string }
  | { type: "state_changed"; at: string; status: RunStatus }
  | { type: "step_log"; at: string; message: string }
  | { type: "artifact_ready"; at: string; artifact_key: string }
  | { type: "gate_result"; at: string; gate: string; passed: boolean }
  | { type: "run_failed"; at: string; reason: string }
  | { type: "run_completed"; at: string };

export interface MockupVariant {
  imageBase64?: string;
  text: string;
  designLink: string;
}

export interface StitchOutput {
  text: string;
  designUrl: string;
  selectedMockupId: string;
}

export interface MetricsSummary {
  total_runs: number;
  running_runs: number;
  failed_runs: number;
  done_runs: number;
  audit_logs: number;
}

// Pipeline step order for display
export const PIPELINE_STEPS = [
  "mockup_generation",
  "mockup_selection",
  "stitch_generation",
  "stitch_approval",
  "spec_generation",
  "codegen",
  "ci_gate_lint",
  "ci_gate_typecheck",
  "ci_gate_build",
  "ci_gate_e2e",
  "ci_gate_visual",
  "ci_gate_a11y",
  "pr_create",
  "preview_deploy",
  "deploy_approval",
  "self_heal",
] as const;

// Status phases for UI grouping
export const STATUS_PHASE: Record<string, string> = {
  draft: "idle",
  mockup_generating: "generating",
  mockup_ready: "action_needed",
  mockup_selected: "processing",
  stitch_generating: "processing",
  stitch_ready: "action_needed",
  stitch_approved: "action_needed",
  stack_selected: "processing",
  contract_locked: "processing",
  spec_generating: "processing",
  codegen_running: "processing",
  ci_running: "processing",
  pr_ready: "processing",
  preview_deployed: "action_needed",
  awaiting_approval: "action_needed",
  prod_deploying: "processing",
  done: "complete",
  failed_retryable: "error",
  failed_final: "error",
  cancelled: "error",
};
