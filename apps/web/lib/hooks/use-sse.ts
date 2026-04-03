"use client";

import { useCallback, useEffect, useRef } from "react";
import type { SseEvent } from "../types";
import { api } from "../api";

export function useSse(
  runId: string | null,
  onEvent: (event: SseEvent) => void,
) {
  const abortRef = useRef<AbortController | null>(null);
  const onEventRef = useRef(onEvent);
  onEventRef.current = onEvent;

  const connect = useCallback(
    (id: string) => {
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;

      const url = api.eventsUrl(id);

      (async () => {
        try {
          const res = await fetch(url, {
            headers: { Accept: "text/event-stream" },
            signal: controller.signal,
          });

          if (!res.ok || !res.body) {
            console.error("[SSE] failed to connect:", res.status);
            return;
          }

          console.log("[SSE] connected to", url);

          const reader = res.body.getReader();
          const decoder = new TextDecoder();
          let buffer = "";
          let dataLines: string[] = [];

          while (true) {
            const { done, value } = await reader.read();
            if (done) break;

            buffer += decoder.decode(value, { stream: true });
            const lines = buffer.split("\n");
            buffer = lines.pop() ?? "";

            for (const line of lines) {
              const trimmed = line.trim();

              if (trimmed === "") {
                if (dataLines.length > 0) {
                  const payload = dataLines.join("\n");
                  dataLines = [];
                  try {
                    const event: SseEvent = JSON.parse(payload);
                    console.log("[SSE] event:", event.type, event);
                    onEventRef.current(event);
                  } catch {
                    console.warn("[SSE] malformed payload:", payload);
                  }
                }
                continue;
              }

              if (trimmed.startsWith("data:")) {
                dataLines.push(trimmed.slice(5).trimStart());
              }
              // ignore "event:" lines, we use the "type" field in JSON
            }
          }
          console.log("[SSE] stream ended");
        } catch (err) {
          if (err instanceof DOMException && err.name === "AbortError") return;
          console.error("[SSE] error:", err);
        }
      })();
    },
    [],
  );

  const disconnect = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
  }, []);

  useEffect(() => {
    if (runId) {
      connect(runId);
    }
    return () => disconnect();
  }, [runId, connect, disconnect]);

  return { disconnect };
}
