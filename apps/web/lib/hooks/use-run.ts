"use client";

import { useCallback, useReducer } from "react";
import type { Run, RunStatus, SseEvent, MockupVariant, StitchOutput } from "../types";
import { api } from "../api";

interface LogEntry {
  time: string;
  text: string;
}

export interface RunState {
  run: Run | null;
  status: RunStatus | "idle" | "connecting" | "error";
  steps: Record<string, string>;
  mockups: Record<string, MockupVariant>;
  stitch: StitchOutput | null;
  previewUrl: string;
  prUrl: string;
  logs: LogEntry[];
  loading: boolean;
  error: string | null;
}

type Action =
  | { type: "SET_LOADING"; loading: boolean }
  | { type: "SET_RUN"; run: Run }
  | { type: "SET_STATUS"; status: RunState["status"] }
  | { type: "SET_ERROR"; error: string }
  | { type: "SET_STEP"; key: string; status: string }
  | { type: "SET_MOCKUPS"; mockups: Record<string, MockupVariant> }
  | { type: "SET_STITCH"; stitch: StitchOutput }
  | { type: "SET_PREVIEW_URL"; url: string }
  | { type: "SET_PR_URL"; url: string }
  | { type: "ADD_LOG"; entry: LogEntry }
  | { type: "RESET" };

const initialState: RunState = {
  run: null,
  status: "idle",
  steps: {},
  mockups: {},
  stitch: null,
  previewUrl: "",
  prUrl: "",
  logs: [],
  loading: false,
  error: null,
};

function reducer(state: RunState, action: Action): RunState {
  switch (action.type) {
    case "SET_LOADING":
      return { ...state, loading: action.loading, error: null };
    case "SET_RUN":
      return { ...state, run: action.run, status: action.run.status, loading: false };
    case "SET_STATUS":
      return { ...state, status: action.status };
    case "SET_ERROR":
      return { ...state, status: "error", error: action.error, loading: false };
    case "SET_STEP":
      return { ...state, steps: { ...state.steps, [action.key]: action.status } };
    case "SET_MOCKUPS":
      return { ...state, mockups: action.mockups };
    case "SET_STITCH":
      return { ...state, stitch: action.stitch };
    case "SET_PREVIEW_URL":
      return { ...state, previewUrl: action.url };
    case "SET_PR_URL":
      return { ...state, prUrl: action.url };
    case "ADD_LOG": {
      const logs = [...state.logs, action.entry];
      return { ...state, logs: logs.length > 200 ? logs.slice(-150) : logs };
    }
    case "RESET":
      return initialState;
    default:
      return state;
  }
}

function fmtTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString("en-US", { hour12: false });
  } catch {
    return "";
  }
}

function extractField(msg: string, field: string): string | null {
  const needle = `${field}=`;
  const idx = msg.indexOf(needle);
  if (idx === -1) return null;
  const rest = msg.slice(idx + needle.length);
  const end = rest.search(/[,\s]/);
  const val = end === -1 ? rest.trim() : rest.slice(0, end).trim();
  return val || null;
}

// Map run status → pipeline steps that should be marked
const STATUS_TO_STEPS: Record<string, Array<{ key: string; status: string }>> = {
  mockup_generating: [{ key: "mockup_generation", status: "running" }],
  mockup_ready: [{ key: "mockup_generation", status: "completed" }],
  mockup_selected: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
  ],
  stitch_generating: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "running" },
  ],
  stitch_ready: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "completed" },
  ],
  stitch_approved: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "completed" },
    { key: "stitch_approval", status: "completed" },
  ],
  stack_selected: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "completed" },
    { key: "stitch_approval", status: "completed" },
  ],
  spec_generating: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "completed" },
    { key: "stitch_approval", status: "completed" },
    { key: "spec_generation", status: "running" },
  ],
  codegen_running: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "completed" },
    { key: "stitch_approval", status: "completed" },
    { key: "spec_generation", status: "completed" },
    { key: "codegen", status: "running" },
  ],
  ci_running: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "completed" },
    { key: "stitch_approval", status: "completed" },
    { key: "spec_generation", status: "completed" },
    { key: "codegen", status: "completed" },
    { key: "ci_gate_lint", status: "running" },
  ],
  pr_ready: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "completed" },
    { key: "stitch_approval", status: "completed" },
    { key: "spec_generation", status: "completed" },
    { key: "codegen", status: "completed" },
    { key: "pr_create", status: "completed" },
  ],
  preview_deployed: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "completed" },
    { key: "stitch_approval", status: "completed" },
    { key: "spec_generation", status: "completed" },
    { key: "codegen", status: "completed" },
    { key: "pr_create", status: "completed" },
    { key: "preview_deploy", status: "completed" },
  ],
  awaiting_approval: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "completed" },
    { key: "stitch_approval", status: "completed" },
    { key: "spec_generation", status: "completed" },
    { key: "codegen", status: "completed" },
    { key: "pr_create", status: "completed" },
    { key: "preview_deploy", status: "completed" },
    { key: "deploy_approval", status: "running" },
  ],
  prod_deploying: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "completed" },
    { key: "stitch_approval", status: "completed" },
    { key: "spec_generation", status: "completed" },
    { key: "codegen", status: "completed" },
    { key: "pr_create", status: "completed" },
    { key: "preview_deploy", status: "completed" },
    { key: "deploy_approval", status: "completed" },
  ],
  done: [
    { key: "mockup_generation", status: "completed" },
    { key: "mockup_selection", status: "completed" },
    { key: "stitch_generation", status: "completed" },
    { key: "stitch_approval", status: "completed" },
    { key: "spec_generation", status: "completed" },
    { key: "codegen", status: "completed" },
    { key: "pr_create", status: "completed" },
    { key: "preview_deploy", status: "completed" },
    { key: "deploy_approval", status: "completed" },
  ],
};

function applyStatusSteps(dispatch: React.Dispatch<Action>, status: string) {
  const stepUpdates = STATUS_TO_STEPS[status];
  if (stepUpdates) {
    for (const { key, status: s } of stepUpdates) {
      dispatch({ type: "SET_STEP", key, status: s });
    }
  }
}

export function useRun() {
  const [state, dispatch] = useReducer(reducer, initialState);

  // Load existing run state from API (for URL-based restore)
  const loadRunState = useCallback(async (runId: string) => {
    dispatch({ type: "RESET" });
    dispatch({ type: "SET_LOADING", loading: true });
    try {
      const run = await api.getRun(runId);
      dispatch({ type: "SET_RUN", run });

      // Apply pipeline steps based on current status
      applyStatusSteps(dispatch, run.status);

      // Also fetch actual steps from API and overlay them
      try {
        const { steps } = await api.listSteps(runId);
        for (const step of steps) {
          dispatch({ type: "SET_STEP", key: step.step_key, status: step.status });
        }
      } catch {
        // steps endpoint may fail, status-based steps are enough
      }

      // Load preview/PR URLs
      try {
        const { steps } = await api.listSteps(runId);
        for (const step of steps) {
          if (step.step_key === "preview_deploy" && step.detail) {
            const url = extractField(step.detail, "preview_url");
            if (url) dispatch({ type: "SET_PREVIEW_URL", url });
          }
          if (step.step_key === "pr_create" && step.detail) {
            const url = extractField(step.detail, "pr_url");
            if (url) dispatch({ type: "SET_PR_URL", url });
          }
        }
      } catch {
        // ok
      }

      return run;
    } catch (err) {
      dispatch({ type: "SET_ERROR", error: String(err) });
      return null;
    }
  }, []);

  const createRun = useCallback(async (prompt: string) => {
    dispatch({ type: "RESET" });
    dispatch({ type: "SET_LOADING", loading: true });
    try {
      const { run } = await api.createRun(prompt);
      dispatch({ type: "SET_RUN", run });
      return run.id;
    } catch (err) {
      dispatch({ type: "SET_ERROR", error: String(err) });
      return null;
    }
  }, []);

  const selectMockup = useCallback(async (mockupId: string) => {
    if (!state.run) return;
    dispatch({ type: "SET_LOADING", loading: true });
    try {
      const { run } = await api.selectMockup(state.run.id, mockupId);
      dispatch({ type: "SET_RUN", run });
    } catch (err) {
      dispatch({ type: "SET_ERROR", error: String(err) });
    }
  }, [state.run]);

  const approveStitch = useCallback(async () => {
    if (!state.run) return;
    dispatch({ type: "SET_LOADING", loading: true });
    try {
      const { run } = await api.approveStitch(state.run.id);
      dispatch({ type: "SET_RUN", run });
    } catch (err) {
      dispatch({ type: "SET_ERROR", error: String(err) });
    }
  }, [state.run]);

  const selectStack = useCallback(async (stackId: string) => {
    if (!state.run) return;
    dispatch({ type: "SET_LOADING", loading: true });
    try {
      const { run } = await api.selectStack(state.run.id, stackId);
      dispatch({ type: "SET_RUN", run });
    } catch (err) {
      dispatch({ type: "SET_ERROR", error: String(err) });
    }
  }, [state.run]);

  const approveDeploy = useCallback(async () => {
    if (!state.run) return;
    dispatch({ type: "SET_LOADING", loading: true });
    try {
      const { run } = await api.approveDeploy(state.run.id);
      dispatch({ type: "SET_RUN", run });
    } catch (err) {
      dispatch({ type: "SET_ERROR", error: String(err) });
    }
  }, [state.run]);

  const rejectDeploy = useCallback(async () => {
    if (!state.run) return;
    dispatch({ type: "SET_LOADING", loading: true });
    try {
      const { run } = await api.rejectDeploy(state.run.id);
      dispatch({ type: "SET_RUN", run });
    } catch (err) {
      dispatch({ type: "SET_ERROR", error: String(err) });
    }
  }, [state.run]);

  const loadMockups = useCallback(async (runId: string) => {
    try {
      const { steps } = await api.listSteps(runId);
      const mockupStep = steps.find(
        (s) => s.step_key === "mockup_generation" && s.status === "completed" && s.detail,
      );
      if (!mockupStep?.detail) return;

      const val = JSON.parse(mockupStep.detail);
      const mockupsData = val.mockups;
      if (!mockupsData) return;

      const result: Record<string, MockupVariant> = {};
      for (const id of ["A", "B", "C"]) {
        const entry = mockupsData[id];
        if (!entry) continue;

        let text = "";
        const banana = entry.banana_mockup ?? entry;
        const geminiText = banana?.candidates?.[0]?.content?.parts?.[0]?.text;
        if (geminiText) {
          text = geminiText.length > 300 ? geminiText.slice(0, 297) + "..." : geminiText;
        } else if (entry.raw_text) {
          text = entry.raw_text.length > 300 ? entry.raw_text.slice(0, 297) + "..." : entry.raw_text;
        }

        let designLink = "";
        const stitch = entry.stitch_screen;
        if (stitch) {
          const url = stitch?.result?.content?.[0]?.text;
          if (url && (url.includes("stitch.withgoogle.com") || url.startsWith("http"))) {
            designLink = url;
          }
          const uri = stitch?.result?.content?.[0]?.resource?.uri;
          if (!designLink && uri) designLink = uri;
          if (!designLink && stitch?.url) designLink = stitch.url;
        }

        result[id] = {
          imageBase64: entry.image_base64 ?? undefined,
          text,
          designLink,
        };
      }

      dispatch({ type: "SET_MOCKUPS", mockups: result });
    } catch (err) {
      console.error("[loadMockups]", err);
    }
  }, []);

  const loadStitchOutput = useCallback(async (runId: string) => {
    try {
      const { steps } = await api.listSteps(runId);
      const stitchStep = steps.find((s) => s.step_key === "stitch_generation" && s.detail);
      if (!stitchStep?.detail) return;

      const val = JSON.parse(stitchStep.detail);
      let text = "";
      if (val.stitch_output?.result?.content?.[0]?.text) {
        text = val.stitch_output.result.content[0].text;
      }
      const selectedMockupId = val.selected_mockup_id ?? "";
      const designUrl = val.stitch_url ?? "";

      dispatch({
        type: "SET_STITCH",
        stitch: { text, designUrl, selectedMockupId },
      });
    } catch (err) {
      console.error("[loadStitchOutput]", err);
    }
  }, []);

  const loadPreviewUrls = useCallback(async (runId: string) => {
    try {
      const { steps } = await api.listSteps(runId);
      for (const step of steps) {
        if (step.step_key === "preview_deploy" && step.detail) {
          const url = extractField(step.detail, "preview_url");
          if (url) dispatch({ type: "SET_PREVIEW_URL", url });
        }
        if (step.step_key === "pr_create" && step.detail) {
          const url = extractField(step.detail, "pr_url");
          if (url) dispatch({ type: "SET_PR_URL", url });
        }
      }
    } catch (err) {
      console.error("[loadPreviewUrls]", err);
    }
  }, []);

  const handleSseEvent = useCallback(
    (event: SseEvent, runId: string) => {
      switch (event.type) {
        case "heartbeat":
          break;
        case "state_changed": {
          dispatch({ type: "SET_STATUS", status: event.status });
          dispatch({
            type: "ADD_LOG",
            entry: { time: fmtTime(event.at), text: `state → ${event.status}` },
          });

          // Map state_changed → pipeline step updates
          applyStatusSteps(dispatch, event.status);

          if (event.status === "mockup_ready") {
            setTimeout(() => loadMockups(runId), 2000);
          }
          if (event.status === "stitch_ready") {
            loadStitchOutput(runId);
          }
          if (event.status === "preview_deployed" || event.status === "awaiting_approval") {
            loadPreviewUrls(runId);
          }
          break;
        }
        case "step_log": {
          dispatch({
            type: "ADD_LOG",
            entry: { time: fmtTime(event.at), text: event.message },
          });
          // Extract step status from "key=status" format
          const eq = event.message.indexOf("=");
          if (eq > 0) {
            const key = event.message.slice(0, eq).trim();
            const rest = event.message.slice(eq + 1);
            const status = rest.split(/[\s(]/)[0];
            if (key && status) {
              dispatch({ type: "SET_STEP", key, status });
            }
          }
          // Extract URLs from step logs
          const previewUrl = extractField(event.message, "preview_url");
          if (previewUrl) dispatch({ type: "SET_PREVIEW_URL", url: previewUrl });
          const prUrl = extractField(event.message, "pr_url");
          if (prUrl) dispatch({ type: "SET_PR_URL", url: prUrl });
          break;
        }
        case "artifact_ready":
          dispatch({
            type: "ADD_LOG",
            entry: { time: fmtTime(event.at), text: `artifact: ${event.artifact_key}` },
          });
          break;
        case "gate_result":
          dispatch({
            type: "ADD_LOG",
            entry: {
              time: fmtTime(event.at),
              text: `gate ${event.gate} ${event.passed ? "passed" : "FAILED"}`,
            },
          });
          break;
        case "run_failed":
          dispatch({ type: "SET_STATUS", status: "error" });
          dispatch({
            type: "ADD_LOG",
            entry: { time: fmtTime(event.at), text: `FAILED: ${event.reason}` },
          });
          break;
        case "run_completed":
          dispatch({ type: "SET_STATUS", status: "done" });
          dispatch({
            type: "ADD_LOG",
            entry: { time: fmtTime(event.at), text: "RUN COMPLETED" },
          });
          break;
      }
    },
    [loadMockups, loadStitchOutput, loadPreviewUrls],
  );

  return {
    state,
    dispatch,
    createRun,
    loadRunState,
    selectMockup,
    approveStitch,
    selectStack,
    approveDeploy,
    rejectDeploy,
    handleSseEvent,
  };
}
