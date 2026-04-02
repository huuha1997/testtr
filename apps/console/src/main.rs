use std::{
    io::{BufRead, BufReader},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use anyhow::{Context, Result, anyhow};
use contracts::{
    CreateRunRequest, CreateRunResponse, ListConnectionsResponse, ListRunStepsResponse,
    MetricsSummaryResponse, RejectDeployRequest, Run, RunTimelineResponse, SelectMockupRequest, SseEvent,
    SelectStackRequest, TransitionRunResponse,
};
use reqwest::blocking::{Client, Response};
use serde::Serialize;

slint::slint! {
    import { VerticalBox, HorizontalBox, Button, LineEdit } from "std-widgets.slint";

    export component AppWindow inherits Window {
        title: "Agentic Console";
        width: 1100px;
        height: 760px;

        in-out property <string> base_url: "http://localhost:8080";
        in-out property <string> prompt_text: "Build a modern SaaS landing page for a design studio";
        in-out property <string> run_id: "";
        in-out property <string> mockup_id: "A";
        in-out property <string> stack_id: "nextjs-tailwind";
        in-out property <string> status_text: "Idle";
        in-out property <string> output_text: "Ready";

        callback create_run(string);
        callback refresh_run(string);
        callback select_mockup(string, string);
        callback select_stack(string, string);
        callback load_steps(string);
        callback load_timeline(string);
        callback approve_deploy(string);
        callback reject_deploy(string);
        callback load_connections();
        callback load_metrics();
        callback start_sse(string);
        callback stop_sse();

        VerticalBox {
            spacing: 10px;
            padding: 14px;

            Text {
                text: "Agentic Console (Slint)";
                font-size: 24px;
                font-weight: 700;
            }

            HorizontalBox {
                spacing: 8px;
                Text { text: "API"; width: 60px; }
                LineEdit { text <=> root.base_url; }
                Button { text: "Connections"; clicked => { root.load_connections(); } }
                Button { text: "Metrics"; clicked => { root.load_metrics(); } }
            }

            HorizontalBox {
                spacing: 8px;
                Text { text: "Prompt"; width: 60px; }
                LineEdit { text <=> root.prompt_text; }
                Button { text: "Create Run"; clicked => { root.create_run(root.prompt_text); } }
            }

            HorizontalBox {
                spacing: 8px;
                Text { text: "Run ID"; width: 60px; }
                LineEdit { text <=> root.run_id; }
                Button { text: "Refresh"; clicked => { root.refresh_run(root.run_id); } }
                Button { text: "Steps"; clicked => { root.load_steps(root.run_id); } }
                Button { text: "Timeline"; clicked => { root.load_timeline(root.run_id); } }
            }

            HorizontalBox {
                spacing: 8px;
                Text { text: "Mockup"; width: 60px; }
                LineEdit { text <=> root.mockup_id; width: 140px; }
                Button {
                    text: "Select Mockup";
                    clicked => { root.select_mockup(root.run_id, root.mockup_id); }
                }

                Text { text: "Stack"; width: 48px; }
                LineEdit { text <=> root.stack_id; width: 180px; }
                Button {
                    text: "Select Stack";
                    clicked => { root.select_stack(root.run_id, root.stack_id); }
                }
            }

            HorizontalBox {
                spacing: 8px;
                Button { text: "Approve Deploy"; clicked => { root.approve_deploy(root.run_id); } }
                Button { text: "Reject Deploy"; clicked => { root.reject_deploy(root.run_id); } }
                Button { text: "Start SSE"; clicked => { root.start_sse(root.run_id); } }
                Button { text: "Stop SSE"; clicked => { root.stop_sse(); } }
                Text { text: root.status_text; }
            }

            Rectangle {
                border-color: #cfd6df;
                border-width: 1px;
                border-radius: 8px;
                background: #f8fafc;
                min-height: 420px;

                VerticalBox {
                    padding: 12px;
                    Text {
                        text: root.output_text;
                        wrap: word-wrap;
                        vertical-stretch: 1;
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
struct OrchestratorApi {
    base_url: String,
    client: Client,
    stream_client: Client,
}

impl OrchestratorApi {
    fn new(base_url: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .context("failed to build http client")?;
        let stream_client = Client::builder()
            .build()
            .context("failed to build stream http client")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
            stream_client,
        })
    }

    fn create_run(&self, prompt: String) -> Result<CreateRunResponse> {
        self.post_json("/api/runs", &CreateRunRequest { prompt })
    }

    fn get_run(&self, run_id: &str) -> Result<Run> {
        let url = format!("{}/api/runs/{}", self.base_url, run_id);
        let response = self.client.get(url).send().context("request failed")?;
        parse_json(response)
    }

    fn select_mockup(&self, run_id: &str, mockup_id: String) -> Result<TransitionRunResponse> {
        let path = format!("/api/runs/{run_id}/select-mockup");
        self.post_json(&path, &SelectMockupRequest { mockup_id })
    }

    fn select_stack(&self, run_id: &str, stack_id: String) -> Result<TransitionRunResponse> {
        let path = format!("/api/runs/{run_id}/select-stack");
        self.post_json(&path, &SelectStackRequest { stack_id })
    }

    fn list_steps(&self, run_id: &str) -> Result<ListRunStepsResponse> {
        let url = format!("{}/api/runs/{run_id}/steps", self.base_url);
        let response = self.client.get(url).send().context("request failed")?;
        parse_json(response)
    }

    fn timeline(&self, run_id: &str) -> Result<RunTimelineResponse> {
        let url = format!("{}/api/runs/{run_id}/timeline", self.base_url);
        let response = self.client.get(url).send().context("request failed")?;
        parse_json(response)
    }

    fn approve_deploy(&self, run_id: &str) -> Result<TransitionRunResponse> {
        let path = format!("/api/runs/{run_id}/approve-deploy");
        let url = format!("{}{}", self.base_url, path);
        let response = self.client.post(url).send().context("request failed")?;
        parse_json(response)
    }

    fn reject_deploy(&self, run_id: &str) -> Result<TransitionRunResponse> {
        let path = format!("/api/runs/{run_id}/reject-deploy");
        self.post_json(
            &path,
            &RejectDeployRequest {
                reason: Some("Rejected from Slint console".to_string()),
            },
        )
    }

    fn list_connections(&self) -> Result<ListConnectionsResponse> {
        let url = format!("{}/api/connections", self.base_url);
        let response = self.client.get(url).send().context("request failed")?;
        parse_json(response)
    }

    fn metrics(&self) -> Result<MetricsSummaryResponse> {
        let url = format!("{}/api/metrics/summary", self.base_url);
        let response = self.client.get(url).send().context("request failed")?;
        parse_json(response)
    }

    fn open_events(&self, run_id: &str) -> Result<Response> {
        let url = format!("{}/api/runs/{run_id}/events", self.base_url);
        let response = self
            .stream_client
            .get(url)
            .header("accept", "text/event-stream")
            .send()
            .context("request failed")?;
        ensure_success(response)
    }

    fn post_json<TReq: Serialize, TResp: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &TReq,
    ) -> Result<TResp> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(url)
            .json(body)
            .send()
            .context("request failed")?;
        parse_json(response)
    }
}

fn parse_json<T: serde::de::DeserializeOwned>(response: Response) -> Result<T> {
    let response = ensure_success(response)?;
    response.json().context("invalid json response")
}

fn pretty<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "<serialize error>".to_string())
}

fn ensure_success(response: Response) -> Result<Response> {
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "<failed to read error body>".to_string());
        return Err(anyhow!("http {}: {}", status.as_u16(), body));
    }
    Ok(response)
}

fn set_status_and_output(weak: &slint::Weak<AppWindow>, status: String, output: String) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_status_text(status.into());
            ui.set_output_text(output.into());
        }
    });
}

fn append_output(weak: &slint::Weak<AppWindow>, line: String) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade() {
            let current = ui.get_output_text().to_string();
            let mut next = if current.is_empty() || current == "Ready" {
                line
            } else {
                format!("{}\n{}", current, line)
            };
            // Prevent unbounded memory growth in long streams.
            let max_len = 24_000usize;
            if next.len() > max_len {
                let start = next.len().saturating_sub(max_len);
                next = next[start..].to_string();
            }
            ui.set_output_text(next.into());
        }
    });
}

fn format_sse_line(event_name: &str, payload: &str) -> Option<String> {
    match serde_json::from_str::<SseEvent>(payload) {
        Ok(SseEvent::Heartbeat { .. }) => None,
        Ok(SseEvent::StateChanged { at, status }) => {
            Some(format!("[{}] state -> {}", at.to_rfc3339(), status.as_str()))
        }
        Ok(SseEvent::StepLog { at, message }) => Some(format!("[{}] step {}", at.to_rfc3339(), message)),
        Ok(SseEvent::ArtifactReady { at, artifact_key }) => {
            Some(format!("[{}] artifact {}", at.to_rfc3339(), artifact_key))
        }
        Ok(SseEvent::GateResult { at, gate, passed }) => Some(format!(
            "[{}] gate {} = {}",
            at.to_rfc3339(),
            gate,
            if passed { "passed" } else { "failed" }
        )),
        Ok(SseEvent::RunFailed { at, reason }) => {
            Some(format!("[{}] run failed: {}", at.to_rfc3339(), reason))
        }
        Ok(SseEvent::RunCompleted { at }) => Some(format!("[{}] run completed", at.to_rfc3339())),
        Err(_) => Some(format!("[{}] {}", event_name, payload)),
    }
}

fn run_action(
    weak: &slint::Weak<AppWindow>,
    loading: &'static str,
    action: impl FnOnce() -> Result<(Option<String>, String)> + Send + 'static,
) {
    set_status_and_output(weak, loading.to_string(), "Working...".to_string());
    let weak = weak.clone();
    thread::spawn(move || match action() {
        Ok((maybe_run_id, output)) => {
            let weak_inner = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak_inner.upgrade() {
                    if let Some(run_id) = maybe_run_id {
                        ui.set_run_id(run_id.into());
                    }
                    ui.set_status_text("Done".into());
                    ui.set_output_text(output.into());
                }
            });
        }
        Err(err) => {
            set_status_and_output(&weak, "Error".to_string(), format!("{}", err));
        }
    });
}

fn require_run_id(run_id: String) -> Result<String> {
    let run_id = run_id.trim().to_string();
    if run_id.is_empty() {
        return Err(anyhow!("run_id is empty"));
    }
    Ok(run_id)
}

fn main() -> Result<()> {
    let ui = AppWindow::new().context("failed to create ui")?;
    let weak = ui.as_weak();
    let sse_stop = Arc::new(AtomicBool::new(false));

    {
        let weak = weak.clone();
        ui.on_create_run(move |prompt| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let prompt = prompt.to_string();
            run_action(&weak, "Creating run", move || {
                let api = OrchestratorApi::new(base_url)?;
                let created = api.create_run(prompt)?;
                Ok((
                    Some(created.run.id.to_string()),
                    format!("Created run:\n{}", pretty(&created)),
                ))
            });
        });
    }

    {
        let weak = weak.clone();
        ui.on_refresh_run(move |run_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            run_action(&weak, "Refreshing run", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let run = api.get_run(&run_id)?;
                Ok((Some(run_id), format!("Run:\n{}", pretty(&run))))
            });
        });
    }

    {
        let weak = weak.clone();
        ui.on_select_mockup(move |run_id, mockup_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            let mockup_id = mockup_id.to_string();
            run_action(&weak, "Selecting mockup", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let resp = api.select_mockup(&run_id, mockup_id)?;
                Ok((Some(run_id), format!("Mockup selected:\n{}", pretty(&resp))))
            });
        });
    }

    {
        let weak = weak.clone();
        ui.on_select_stack(move |run_id, stack_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            let stack_id = stack_id.to_string();
            run_action(&weak, "Selecting stack", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let resp = api.select_stack(&run_id, stack_id)?;
                Ok((Some(run_id), format!("Stack selected:\n{}", pretty(&resp))))
            });
        });
    }

    {
        let weak = weak.clone();
        ui.on_load_steps(move |run_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            run_action(&weak, "Loading steps", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let resp = api.list_steps(&run_id)?;
                Ok((Some(run_id), format!("Steps:\n{}", pretty(&resp))))
            });
        });
    }

    {
        let weak = weak.clone();
        ui.on_load_timeline(move |run_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            run_action(&weak, "Loading timeline", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let resp = api.timeline(&run_id)?;
                Ok((Some(run_id), format!("Timeline:\n{}", pretty(&resp))))
            });
        });
    }

    {
        let weak = weak.clone();
        ui.on_approve_deploy(move |run_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            run_action(&weak, "Approving deploy", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let resp = api.approve_deploy(&run_id)?;
                Ok((Some(run_id), format!("Deploy approved:\n{}", pretty(&resp))))
            });
        });
    }

    {
        let weak = weak.clone();
        ui.on_reject_deploy(move |run_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            run_action(&weak, "Rejecting deploy", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let resp = api.reject_deploy(&run_id)?;
                Ok((Some(run_id), format!("Deploy rejected:\n{}", pretty(&resp))))
            });
        });
    }

    {
        let weak = weak.clone();
        ui.on_load_connections(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            run_action(&weak, "Loading connections", move || {
                let api = OrchestratorApi::new(base_url)?;
                let resp = api.list_connections()?;
                Ok((None, format!("Connections:\n{}", pretty(&resp))))
            });
        });
    }

    {
        let weak = weak.clone();
        ui.on_load_metrics(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            run_action(&weak, "Loading metrics", move || {
                let api = OrchestratorApi::new(base_url)?;
                let resp = api.metrics()?;
                Ok((None, format!("Metrics:\n{}", pretty(&resp))))
            });
        });
    }

    {
        let weak = weak.clone();
        let sse_stop = Arc::clone(&sse_stop);
        ui.on_start_sse(move |run_id| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            sse_stop.store(false, Ordering::SeqCst);
            set_status_and_output(
                &weak,
                "Connecting SSE".to_string(),
                "Opening event stream...".to_string(),
            );

            let weak_thread = weak.clone();
            let sse_stop_thread = Arc::clone(&sse_stop);
            thread::spawn(move || {
                let run_id = match require_run_id(run_id) {
                    Ok(v) => v,
                    Err(err) => {
                        set_status_and_output(
                            &weak_thread,
                            "Error".to_string(),
                            format!("Cannot start SSE: {}", err),
                        );
                        return;
                    }
                };
                let api = match OrchestratorApi::new(base_url) {
                    Ok(v) => v,
                    Err(err) => {
                        set_status_and_output(
                            &weak_thread,
                            "Error".to_string(),
                            format!("Cannot start SSE: {}", err),
                        );
                        return;
                    }
                };
                let response = match api.open_events(&run_id) {
                    Ok(v) => v,
                    Err(err) => {
                        set_status_and_output(
                            &weak_thread,
                            "Error".to_string(),
                            format!("Cannot open SSE: {}", err),
                        );
                        return;
                    }
                };

                set_status_and_output(
                    &weak_thread,
                    "SSE Connected".to_string(),
                    format!("Streaming run {} events...", run_id),
                );

                let mut reader = BufReader::new(response);
                let mut line = String::new();
                let mut current_event = String::from("message");
                let mut data_lines: Vec<String> = Vec::new();

                loop {
                    line.clear();
                    let read = match reader.read_line(&mut line) {
                        Ok(n) => n,
                        Err(err) => {
                            set_status_and_output(
                                &weak_thread,
                                "Error".to_string(),
                                format!("SSE read error: {}", err),
                            );
                            return;
                        }
                    };
                    if read == 0 {
                        break;
                    }
                    if sse_stop_thread.load(Ordering::SeqCst) {
                        break;
                    }

                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    if trimmed.is_empty() {
                        if !data_lines.is_empty() {
                            let payload = data_lines.join("\n");
                            if let Some(line) = format_sse_line(&current_event, &payload) {
                                append_output(&weak_thread, line);
                            }
                            data_lines.clear();
                            current_event = "message".to_string();
                        }
                        continue;
                    }

                    if let Some(v) = trimmed.strip_prefix("event:") {
                        current_event = v.trim().to_string();
                    } else if let Some(v) = trimmed.strip_prefix("data:") {
                        data_lines.push(v.trim_start().to_string());
                    }
                }

                if sse_stop_thread.load(Ordering::SeqCst) {
                    set_status_and_output(
                        &weak_thread,
                        "SSE Stopped".to_string(),
                        "Stopped event stream".to_string(),
                    );
                } else {
                    set_status_and_output(
                        &weak_thread,
                        "SSE Closed".to_string(),
                        "Event stream closed by server".to_string(),
                    );
                }
            });
        });
    }

    {
        let weak = weak.clone();
        let sse_stop = Arc::clone(&sse_stop);
        ui.on_stop_sse(move || {
            sse_stop.store(true, Ordering::SeqCst);
            set_status_and_output(
                &weak,
                "Stopping SSE".to_string(),
                "Waiting for stream loop to stop...".to_string(),
            );
        });
    }

    ui.run().context("ui runtime failed")?;
    Ok(())
}
