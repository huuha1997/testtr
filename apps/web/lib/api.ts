import type { Run, RunStep, RunTimelineItem, MetricsSummary } from "./types";

const BASE = "";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...init?.headers,
    },
  });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`HTTP ${res.status}: ${body}`);
  }
  return res.json();
}

export const api = {
  createRun(prompt: string) {
    return request<{ run: Run }>("/api/runs", {
      method: "POST",
      body: JSON.stringify({ prompt }),
    });
  },

  getRun(runId: string) {
    return request<Run>(`/api/runs/${runId}`);
  },

  selectMockup(runId: string, mockupId: string) {
    return request<{ run: Run }>(`/api/runs/${runId}/select-mockup`, {
      method: "POST",
      body: JSON.stringify({ mockup_id: mockupId }),
    });
  },

  approveStitch(runId: string) {
    return request<{ run: Run }>(`/api/runs/${runId}/approve-stitch`, {
      method: "POST",
      body: JSON.stringify({ screen_id: null }),
    });
  },

  selectStack(runId: string, stackId: string) {
    return request<{ run: Run }>(`/api/runs/${runId}/select-stack`, {
      method: "POST",
      body: JSON.stringify({ stack_id: stackId }),
    });
  },

  approveDeploy(runId: string) {
    return request<{ run: Run }>(`/api/runs/${runId}/approve-deploy`, {
      method: "POST",
    });
  },

  rejectDeploy(runId: string, reason?: string) {
    return request<{ run: Run }>(`/api/runs/${runId}/reject-deploy`, {
      method: "POST",
      body: JSON.stringify({ reason: reason ?? "Rejected from web UI" }),
    });
  },

  listSteps(runId: string) {
    return request<{ run_id: string; steps: RunStep[] }>(`/api/runs/${runId}/steps`);
  },

  timeline(runId: string) {
    return request<{ run_id: string; items: RunTimelineItem[] }>(`/api/runs/${runId}/timeline`);
  },

  metrics() {
    return request<MetricsSummary>("/api/metrics/summary");
  },

  eventsUrl(runId: string) {
    return `${BASE}/api/runs/${runId}/events`;
  },
};
