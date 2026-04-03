"use client";

import { Suspense, useCallback, useEffect, useRef, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Header } from "@/components/header";
import { Sidebar } from "@/components/sidebar";
import { ActionCards } from "@/components/action-cards";
import { MockupCards } from "@/components/mockup-cards";
import { LogPanel } from "@/components/log-panel";
import { EmptyState } from "@/components/empty-state";
import { useRun } from "@/lib/hooks/use-run";
import { useSse } from "@/lib/hooks/use-sse";
import { useHistory } from "@/lib/hooks/use-history";
import type { SseEvent } from "@/lib/types";

function App() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const { entries, addRun, updateRun, removeRun } = useHistory();

  const {
    state,
    createRun,
    loadRunState,
    selectMockup,
    approveStitch,
    selectStack,
    approveDeploy,
    rejectDeploy,
    handleSseEvent,
  } = useRun();

  const [sseRunId, setSseRunId] = useState<string | null>(null);
  const sseRunIdRef = useRef<string | null>(null);
  sseRunIdRef.current = sseRunId;

  const handleSseEventRef = useRef(handleSseEvent);
  handleSseEventRef.current = handleSseEvent;

  // Load run from URL param on mount
  useEffect(() => {
    const runId = searchParams.get("run");
    if (runId && !state.run) {
      loadRunState(runId).then((run) => {
        if (run) setSseRunId(runId);
      });
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Sync status changes to history
  useEffect(() => {
    if (state.run) {
      updateRun(state.run.id, {
        status: typeof state.status === "string" ? state.status : "idle",
        previewUrl: state.previewUrl || undefined,
        prUrl: state.prUrl || undefined,
      });
    }
  }, [state.run, state.status, state.previewUrl, state.prUrl, updateRun]);

  const onSseEvent = useCallback(
    (event: SseEvent) => {
      const id = sseRunIdRef.current;
      if (id) {
        handleSseEventRef.current(event, id);
      }
    },
    [],
  );

  useSse(sseRunId, onSseEvent);

  const handleCreateRun = useCallback(
    async (prompt: string) => {
      const id = await createRun(prompt);
      if (id) {
        addRun(id, prompt);
        setSseRunId(id);
        router.replace(`/?run=${id}`);
      }
    },
    [createRun, addRun, router],
  );

  const handleSelectRun = useCallback(
    async (runId: string) => {
      const run = await loadRunState(runId);
      if (run) {
        setSseRunId(runId);
        router.replace(`/?run=${runId}`);
      }
    },
    [loadRunState, router],
  );

  const hasMockups = Object.keys(state.mockups).length > 0;
  const showMockups =
    hasMockups &&
    (state.status === "mockup_ready" || state.status === "mockup_selected");

  const showEmpty =
    !hasMockups &&
    (state.status === "idle" ||
      state.status === "connecting" ||
      state.status === "mockup_generating" ||
      state.status === "draft");

  return (
    <div className="flex h-screen flex-col">
      <Header status={state.status} runId={state.run?.id} />

      <div className="flex flex-1 overflow-hidden">
        <Sidebar
          state={state}
          onCreateRun={handleCreateRun}
          loading={state.loading}
          history={entries}
          onSelectRun={handleSelectRun}
          onRemoveRun={removeRun}
        />

        <main className="flex flex-1 flex-col gap-4 overflow-y-auto p-6">
          <ActionCards
            state={state}
            onApproveStitch={approveStitch}
            onSelectStack={selectStack}
            onApproveDeploy={approveDeploy}
            onRejectDeploy={rejectDeploy}
          />

          {showMockups && (
            <MockupCards
              mockups={state.mockups}
              canSelect={state.status === "mockup_ready"}
              onSelect={selectMockup}
              loading={state.loading}
            />
          )}

          {showEmpty && <EmptyState status={state.status} />}

          <div className="flex-1" />

          <LogPanel logs={state.logs} />
        </main>
      </div>
    </div>
  );
}

export default function Home() {
  return (
    <Suspense>
      <App />
    </Suspense>
  );
}
