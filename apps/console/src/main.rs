use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use anyhow::{Context, Result, anyhow};
use base64::Engine as _;
use contracts::{
    CreateRunRequest, CreateRunResponse, ListConnectionsResponse, ListRunStepsResponse,
    MetricsSummaryResponse, RejectDeployRequest, Run, RunTimelineResponse,
    SelectMockupRequest, SseEvent, SelectStackRequest, TransitionRunResponse,
};
use reqwest::blocking::{Client, Response};
use serde::Serialize;

slint::slint! {
    import { VerticalBox, HorizontalBox, Button, LineEdit, TextEdit, ScrollView } from "std-widgets.slint";

    export component AppWindow inherits Window {
        title: "Agentic Console";
        width: 1400px;
        height: 920px;
        background: #0f172a;

        in-out property <string> base_url: "http://localhost:8080";
        in-out property <string> prompt_text: "Build a modern SaaS landing page for a design studio";
        in-out property <string> run_id: "";
        in-out property <string> mockup_id: "A";
        in-out property <string> stack_id: "nextjs-tailwind";
        in-out property <string> status_text: "Idle";
        in-out property <string> output_text: "";
        in-out property <string> steps_text: "";
        in-out property <image> mockup_image_a;
        in-out property <image> mockup_image_b;
        in-out property <image> mockup_image_c;
        in-out property <bool> has_mockup: false;
        in-out property <string> preview_url: "";
        in-out property <string> pr_url: "";

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
            spacing: 0px; padding: 0px;

            // ── Header bar ──
            Rectangle {
                height: 48px;
                background: #1e293b;
                HorizontalBox {
                    padding-left: 20px; padding-right: 20px; spacing: 16px;
                    Text { text: "AGENTIC"; color: #38bdf8; font-size: 14px; font-weight: 800; vertical-alignment: center; letter-spacing: 2px; }
                    Text { text: "Console"; color: #94a3b8; font-size: 14px; font-weight: 400; vertical-alignment: center; }
                    Rectangle { horizontal-stretch: 1; }
                    // Status pill
                    Rectangle {
                        width: 180px; height: 28px; border-radius: 14px;
                        background: root.status_text == "Error" ? #991b1b :
                                    root.status_text == "Done" ? #166534 :
                                    root.status_text == "Idle" ? #334155 :
                                    root.status_text == "mockup_ready" ? #854d0e :
                                    root.status_text == "awaiting_approval" ? #166534 :
                                    root.status_text == "SSE Connected" ? #075985 : #581c87;
                        Text {
                            text: root.status_text;
                            color: #e2e8f0; font-size: 11px; font-weight: 600;
                            horizontal-alignment: center; vertical-alignment: center;
                        }
                    }
                }
            }

            // ── Main ──
            HorizontalBox {
                padding: 12px; spacing: 12px;

                // ── Left sidebar ──
                Rectangle {
                    width: 320px;
                    border-radius: 12px; background: #1e293b;
                    VerticalBox {
                        padding: 16px; spacing: 14px;

                        // Prompt + Run
                        Text { text: "PROMPT"; font-size: 9px; color: #64748b; font-weight: 700; letter-spacing: 1.5px; }
                        LineEdit { text <=> root.prompt_text; }
                        Button { text: "Create Run"; clicked => { root.create_run(root.prompt_text); } }

                        // Run ID
                        Rectangle { height: 1px; background: #334155; }
                        Text { text: "RUN"; font-size: 9px; color: #64748b; font-weight: 700; letter-spacing: 1.5px; }
                        LineEdit { text <=> root.run_id; }
                        HorizontalBox {
                            spacing: 6px;
                            Button { text: "Refresh"; clicked => { root.refresh_run(root.run_id); } }
                            Button { text: "Steps"; clicked => { root.load_steps(root.run_id); } }
                            Button { text: "Timeline"; clicked => { root.load_timeline(root.run_id); } }
                        }

                        // Pipeline status
                        Rectangle { height: 1px; background: #334155; }
                        Text { text: "PIPELINE"; font-size: 9px; color: #64748b; font-weight: 700; letter-spacing: 1.5px; }
                        ScrollView {
                            vertical-stretch: 1;
                            Text {
                                text: root.steps_text == "" ? "Waiting..." : root.steps_text;
                                font-size: 11px; color: #cbd5e1; wrap: word-wrap;
                            }
                        }

                        // Controls
                        Rectangle { height: 1px; background: #334155; }
                        HorizontalBox {
                            spacing: 6px;
                            Button { text: "Stop"; clicked => { root.stop_sse(); } }
                            Button { text: "Metrics"; clicked => { root.load_metrics(); } }
                        }
                    }
                }

                // ── Center: mockups + actions ──
                VerticalBox {
                    horizontal-stretch: 1; spacing: 12px;

                    // ACTION CARDS
                    if root.status_text == "mockup_ready": Rectangle {
                        height: 44px; border-radius: 10px;
                        background: #422006;
                        border-color: #f59e0b; border-width: 1px;
                        HorizontalBox {
                            padding-left: 16px; padding-right: 16px; spacing: 10px;
                            Text { text: "Mockup ready — pick one to continue"; color: #fbbf24; font-size: 13px; font-weight: 600; vertical-alignment: center; horizontal-stretch: 1; }
                        }
                    }

                    if root.status_text == "mockup_selected": Rectangle {
                        height: 60px; border-radius: 10px;
                        background: #172554;
                        border-color: #3b82f6; border-width: 1px;
                        HorizontalBox {
                            padding-left: 16px; padding-right: 16px; spacing: 10px;
                            Text { text: "STACK"; color: #60a5fa; font-size: 10px; font-weight: 700; vertical-alignment: center; }
                            LineEdit { text <=> root.stack_id; horizontal-stretch: 1; }
                            Button { text: "Start Build"; clicked => { root.select_stack(root.run_id, root.stack_id); } }
                        }
                    }

                    if root.status_text == "preview_deployed" || root.status_text == "awaiting_approval": Rectangle {
                        height: 140px; border-radius: 12px;
                        background: #052e16;
                        border-color: #22c55e; border-width: 2px;
                        VerticalBox {
                            padding: 20px; spacing: 12px;
                            Text { text: "READY FOR REVIEW"; font-size: 11px; color: #22c55e; font-weight: 700; letter-spacing: 1.5px; }
                            Text {
                                text: root.preview_url != "" ? root.preview_url : root.pr_url != "" ? root.pr_url : "Preview deployed — approve to ship to production";
                                font-size: 14px; color: #4ade80; wrap: word-wrap; font-weight: 600;
                            }
                            HorizontalBox {
                                spacing: 12px;
                                Button { text: "Approve Deploy"; clicked => { root.approve_deploy(root.run_id); } }
                                Button { text: "Reject"; clicked => { root.reject_deploy(root.run_id); } }
                            }
                        }
                    }

                    // 3 MOCKUP IMAGES (only during selection phase)
                    if root.has_mockup && (root.status_text == "mockup_ready" || root.status_text == "mockup_selected"): HorizontalBox {
                        spacing: 10px;
                        vertical-stretch: 1;

                        Rectangle {
                            horizontal-stretch: 1; border-radius: 10px; background: #1e293b;
                            border-color: root.status_text == "mockup_ready" ? #475569 : #334155; border-width: 1px;
                            VerticalBox {
                                padding: 8px; spacing: 6px;
                                Text { text: "A"; font-size: 12px; color: #94a3b8; font-weight: 800; horizontal-alignment: center; }
                                Image { source: root.mockup_image_a; horizontal-stretch: 1; vertical-stretch: 1; image-fit: contain; }
                                if root.status_text == "mockup_ready": Button { text: "Select A"; clicked => { root.select_mockup(root.run_id, "A"); } }
                            }
                        }

                        Rectangle {
                            horizontal-stretch: 1; border-radius: 10px; background: #1e293b;
                            border-color: root.status_text == "mockup_ready" ? #475569 : #334155; border-width: 1px;
                            VerticalBox {
                                padding: 8px; spacing: 6px;
                                Text { text: "B"; font-size: 12px; color: #94a3b8; font-weight: 800; horizontal-alignment: center; }
                                Image { source: root.mockup_image_b; horizontal-stretch: 1; vertical-stretch: 1; image-fit: contain; }
                                if root.status_text == "mockup_ready": Button { text: "Select B"; clicked => { root.select_mockup(root.run_id, "B"); } }
                            }
                        }

                        Rectangle {
                            horizontal-stretch: 1; border-radius: 10px; background: #1e293b;
                            border-color: root.status_text == "mockup_ready" ? #475569 : #334155; border-width: 1px;
                            VerticalBox {
                                padding: 8px; spacing: 6px;
                                Text { text: "C"; font-size: 12px; color: #94a3b8; font-weight: 800; horizontal-alignment: center; }
                                Image { source: root.mockup_image_c; horizontal-stretch: 1; vertical-stretch: 1; image-fit: contain; }
                                if root.status_text == "mockup_ready": Button { text: "Select C"; clicked => { root.select_mockup(root.run_id, "C"); } }
                            }
                        }
                    }

                    // Empty state when no mockups
                    if !root.has_mockup: Rectangle {
                        vertical-stretch: 1; border-radius: 10px; background: #1e293b;
                        Text {
                            text: root.status_text == "Idle" ? "Create a run to get started"
                                : root.status_text == "Creating run..." ? "Creating..."
                                : "Generating mockups...";
                            color: #475569; font-size: 16px; font-weight: 600;
                            horizontal-alignment: center; vertical-alignment: center;
                        }
                    }

                    // LOG
                    Rectangle {
                        height: 180px; border-radius: 10px; background: #1e293b;
                        border-color: #334155; border-width: 1px;
                        VerticalBox {
                            padding: 10px; spacing: 4px;
                            Text { text: "LOG"; font-size: 9px; color: #64748b; font-weight: 700; letter-spacing: 1.5px; }
                            TextEdit {
                                text <=> root.output_text;
                                read-only: true; wrap: word-wrap;
                                vertical-stretch: 1; font-size: 11px;
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── API client ───────────────────────────────────────────────────────────────

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
        parse_json(self.client.get(url).send().context("request failed")?)
    }

    fn select_mockup(&self, run_id: &str, mockup_id: String) -> Result<TransitionRunResponse> {
        self.post_json(&format!("/api/runs/{run_id}/select-mockup"), &SelectMockupRequest { mockup_id })
    }

    fn select_stack(&self, run_id: &str, stack_id: String) -> Result<TransitionRunResponse> {
        self.post_json(&format!("/api/runs/{run_id}/select-stack"), &SelectStackRequest { stack_id })
    }

    fn list_steps(&self, run_id: &str) -> Result<ListRunStepsResponse> {
        let url = format!("{}/api/runs/{run_id}/steps", self.base_url);
        parse_json(self.client.get(url).send().context("request failed")?)
    }

    fn timeline(&self, run_id: &str) -> Result<RunTimelineResponse> {
        let url = format!("{}/api/runs/{run_id}/timeline", self.base_url);
        parse_json(self.client.get(url).send().context("request failed")?)
    }

    fn approve_deploy(&self, run_id: &str) -> Result<TransitionRunResponse> {
        let url = format!("{}/api/runs/{run_id}/approve-deploy", self.base_url);
        parse_json(self.client.post(url).send().context("request failed")?)
    }

    fn reject_deploy(&self, run_id: &str) -> Result<TransitionRunResponse> {
        self.post_json(
            &format!("/api/runs/{}/reject-deploy", run_id),
            &RejectDeployRequest { reason: Some("Rejected from console".to_string()) },
        )
    }

    fn list_connections(&self) -> Result<ListConnectionsResponse> {
        let url = format!("{}/api/connections", self.base_url);
        parse_json(self.client.get(url).send().context("request failed")?)
    }

    fn metrics(&self) -> Result<MetricsSummaryResponse> {
        let url = format!("{}/api/metrics/summary", self.base_url);
        parse_json(self.client.get(url).send().context("request failed")?)
    }

    fn open_events(&self, run_id: &str) -> Result<Response> {
        let url = format!("{}/api/runs/{run_id}/events", self.base_url);
        ensure_success(
            self.stream_client
                .get(url)
                .header("accept", "text/event-stream")
                .send()
                .context("request failed")?,
        )
    }

    fn post_json<TReq: Serialize, TResp: serde::de::DeserializeOwned>(
        &self, path: &str, body: &TReq,
    ) -> Result<TResp> {
        let url = format!("{}{}", self.base_url, path);
        parse_json(self.client.post(url).json(body).send().context("request failed")?)
    }
}

fn parse_json<T: serde::de::DeserializeOwned>(response: Response) -> Result<T> {
    ensure_success(response)?.json().context("invalid json response")
}

fn ensure_success(response: Response) -> Result<Response> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_else(|_| "<failed to read body>".to_string());
        return Err(anyhow!("HTTP {}: {}", status.as_u16(), body));
    }
    Ok(response)
}

fn pretty<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "<serialize error>".to_string())
}

// ── UI helpers ───────────────────────────────────────────────────────────────

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
    // Filter out lines with large base64 blobs
    if line.len() > 500 && (line.contains("image_base64") || line.contains("base64")) {
        let truncated = format!("{}... [image data truncated]", &line[..line.len().min(120)]);
        return append_output(weak, truncated);
    }
    // Truncate any very long line
    let line = if line.len() > 300 {
        format!("{}...", &line[..297])
    } else {
        line
    };
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade() {
            let current = ui.get_output_text().to_string();
            let mut next = if current.is_empty() {
                line
            } else {
                format!("{}\n{}", current, line)
            };
            let max_len = 8_000usize;
            if next.len() > max_len {
                next = next[next.len().saturating_sub(max_len)..].to_string();
            }
            ui.set_output_text(next.into());
        }
    });
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
            let _ = slint::invoke_from_event_loop({
                let weak = weak.clone();
                move || {
                    if let Some(ui) = weak.upgrade() {
                        if let Some(run_id) = maybe_run_id {
                            ui.set_run_id(run_id.into());
                        }
                        ui.set_status_text("Done".into());
                        ui.set_output_text(output.into());
                    }
                }
            });
        }
        Err(err) => {
            set_status_and_output(&weak, "Error".to_string(), format!("ERROR: {}", err));
        }
    });
}

fn require_run_id(run_id: String) -> Result<String> {
    let run_id = run_id.trim().to_string();
    if run_id.is_empty() { return Err(anyhow!("run_id is empty")); }
    Ok(run_id)
}

// ── Step tracking ────────────────────────────────────────────────────────────

type StepMap = Arc<Mutex<HashMap<String, String>>>;

fn format_steps_display(map: &HashMap<String, String>) -> String {
    if map.is_empty() { return String::new(); }
    let order = [
        "mockup_generation", "spec_generation", "codegen",
        "ci_gate_lint", "ci_gate_typecheck", "ci_gate_build",
        "ci_gate_e2e", "ci_gate_visual", "ci_gate_a11y",
        "pr_create", "preview_deploy", "deploy_approval", "self_heal",
    ];
    let mut lines: Vec<String> = order.iter()
        .filter_map(|&k| {
            let v = map.get(k)?;
            let icon = match v.as_str() {
                "completed" | "passed" => "✓",
                "failed" | "failed_final" => "✗",
                "waiting" => "⏸",
                "retrying" => "↺",
                _ => "→",
            };
            Some(format!("{} {}  {}", icon, k, v))
        })
        .collect();
    // Add any step not in the known order
    for (k, v) in map {
        if !order.contains(&k.as_str()) {
            let icon = match v.as_str() {
                "completed" | "passed" => "✓",
                "failed" | "failed_final" => "✗",
                _ => "→",
            };
            lines.push(format!("{} {}  {}", icon, k, v));
        }
    }
    lines.join("\n")
}

fn update_steps_ui(weak: &slint::Weak<AppWindow>, map: &HashMap<String, String>) {
    let text = format_steps_display(map);
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_steps_text(text.into());
        }
    });
}

// ── Mockup image loading ─────────────────────────────────────────────────────

fn decode_mockup_image(b64: &str) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    use image::ImageReader;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let cursor = std::io::Cursor::new(bytes);
    let decoded = ImageReader::new(cursor).with_guessed_format().ok()?.decode().ok()?;
    let rgba = decoded.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba.as_raw(), w, h))
}

fn try_set_mockup_images(weak: &slint::Weak<AppWindow>, api: &OrchestratorApi, run_id: &str) {
    eprintln!("[mockup] fetching steps for {}", run_id);
    let Ok(steps) = api.list_steps(run_id) else {
        eprintln!("[mockup] failed to fetch steps");
        return;
    };
    for step in &steps.steps {
        if step.step_key != "mockup_generation" { continue; }
        let Some(ref detail) = step.detail else {
            eprintln!("[mockup] step has no detail");
            continue;
        };
        eprintln!("[mockup] detail len={}, starts with: {}", detail.len(), &detail[..detail.len().min(80)]);

        // New format: JSON {"processed_at":"...","mockups":{"A":{...},"B":{...},"C":{...}}}
        let val: serde_json::Value = match serde_json::from_str(detail) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[mockup] JSON parse failed: {}", e);
                continue;
            }
        };
        let Some(mockups) = val.get("mockups").and_then(|m| m.as_object()) else {
            eprintln!("[mockup] no 'mockups' key in JSON");
            continue;
        };
        eprintln!("[mockup] found {} mockups: {:?}", mockups.len(), mockups.keys().collect::<Vec<_>>());

        let ids = [("A", 0u8), ("B", 1), ("C", 2)];
        let mut buffers: [Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>>; 3] =
            [None, None, None];

        for (id, idx) in ids {
            let Some(entry) = mockups.get(id) else {
                eprintln!("[mockup] missing variant {}", id);
                continue;
            };
            let Some(b64) = entry.get("image_base64").and_then(|v| v.as_str()) else {
                eprintln!("[mockup] variant {} has no image_base64", id);
                continue;
            };
            eprintln!("[mockup] decoding variant {} (b64 len={})", id, b64.len());
            buffers[idx as usize] = decode_mockup_image(b64);
            eprintln!("[mockup] variant {} decoded: {}", id, buffers[idx as usize].is_some());
        }

        let [buf_a, buf_b, buf_c] = buffers;
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                if let Some(b) = buf_a { ui.set_mockup_image_a(slint::Image::from_rgba8(b)); }
                if let Some(b) = buf_b { ui.set_mockup_image_b(slint::Image::from_rgba8(b)); }
                if let Some(b) = buf_c { ui.set_mockup_image_c(slint::Image::from_rgba8(b)); }
                ui.set_has_mockup(true);
            }
        });
        return;
    }
}

fn fetch_and_set_preview_url(weak: &slint::Weak<AppWindow>, api: &OrchestratorApi, run_id: &str) {
    let Ok(steps) = api.list_steps(run_id) else { return; };
    let extract_field = |detail: &str, field: &str| -> Option<String> {
        let needle = format!("{}=", field);
        let start = detail.find(&needle)? + needle.len();
        let rest = &detail[start..];
        let end = rest.find(',').unwrap_or(rest.len());
        let val = rest[..end].trim().to_string();
        if val.is_empty() { None } else { Some(val) }
    };
    for step in &steps.steps {
        if step.step_key == "preview_deploy" {
            if let Some(ref detail) = step.detail {
                if let Some(url) = extract_field(detail, "preview_url") {
                    let weak = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = weak.upgrade() { ui.set_preview_url(url.into()); }
                    });
                }
            }
        }
        if step.step_key == "pr_create" {
            if let Some(ref detail) = step.detail {
                if let Some(url) = extract_field(detail, "pr_url") {
                    let weak = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = weak.upgrade() { ui.set_pr_url(url.into()); }
                    });
                }
            }
        }
    }
}

// ── SSE formatting ───────────────────────────────────────────────────────────

fn fmt_time(at: &chrono::DateTime<chrono::Utc>) -> String {
    at.format("%H:%M:%S").to_string()
}

fn format_sse_event(parsed: &SseEvent) -> Option<String> {
    match parsed {
        SseEvent::Heartbeat { .. } => None,
        SseEvent::StateChanged { at, status } => {
            Some(format!("● [{}] state → {}", fmt_time(at), status.as_str()))
        }
        SseEvent::StepLog { at, message } => {
            Some(format!("  [{}] {}", fmt_time(at), message))
        }
        SseEvent::ArtifactReady { at, artifact_key } => {
            Some(format!("📦 [{}] artifact: {}", fmt_time(at), artifact_key))
        }
        SseEvent::GateResult { at, gate, passed } => {
            let icon = if *passed { "✓" } else { "✗" };
            Some(format!("{} [{}] gate {} {}", icon, fmt_time(at), gate,
                if *passed { "passed" } else { "FAILED" }))
        }
        SseEvent::RunFailed { at, reason } => {
            Some(format!("✗ [{}] RUN FAILED: {}", fmt_time(at), reason))
        }
        SseEvent::RunCompleted { at } => {
            Some(format!("✓ [{}] RUN COMPLETED", fmt_time(at)))
        }
    }
}

// ── main ─────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let ui = AppWindow::new().context("failed to create ui")?;
    let weak = ui.as_weak();
    let sse_stop = Arc::new(AtomicBool::new(false));
    let step_map: StepMap = Arc::new(Mutex::new(HashMap::new()));

    // Create Run
    {
        let weak = weak.clone();
        ui.on_create_run(move |prompt| {
            let Some(ui) = weak.upgrade() else { return; };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let prompt = prompt.to_string();
            // Reset steps/mockup
            {
                let weak2 = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = weak2.upgrade() {
                        u.set_steps_text("".into());
                        u.set_has_mockup(false);
                        u.set_output_text("".into());
                        u.set_status_text("Creating run...".into());
                    }
                });
            }
            let weak_t = weak.clone();
            thread::spawn(move || {
                match (|| -> Result<(String, String)> {
                    let api = OrchestratorApi::new(base_url)?;
                    let res = api.create_run(prompt)?;
                    Ok((res.run.id.to_string(), format!("Run created:\n{}", pretty(&res))))
                })() {
                    Ok((run_id, output)) => {
                        let _ = slint::invoke_from_event_loop({
                            let weak = weak_t.clone();
                            move || {
                                if let Some(ui) = weak.upgrade() {
                                    ui.set_run_id(run_id.clone().into());
                                    ui.set_output_text(output.into());
                                    // Auto-start SSE so pipeline events show immediately
                                    ui.invoke_start_sse(run_id.into());
                                }
                            }
                        });
                    }
                    Err(err) => {
                        set_status_and_output(&weak_t, "Error".to_string(), format!("ERROR: {}", err));
                    }
                }
            });
        });
    }

    // Refresh Run
    {
        let weak = weak.clone();
        ui.on_refresh_run(move |run_id| {
            let Some(ui) = weak.upgrade() else { return; };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            run_action(&weak, "Refreshing", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let run = api.get_run(&run_id)?;
                Ok((Some(run_id), format!("Run:\n{}", pretty(&run))))
            });
        });
    }

    // Select Mockup
    {
        let weak = weak.clone();
        ui.on_select_mockup(move |run_id, mockup_id| {
            let Some(ui) = weak.upgrade() else { return; };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let (run_id, mockup_id) = (run_id.to_string(), mockup_id.to_string());
            run_action(&weak, "Selecting mockup", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let res = api.select_mockup(&run_id, mockup_id)?;
                Ok((Some(run_id), format!("Mockup selected:\n{}", pretty(&res))))
            });
        });
    }

    // Select Stack
    {
        let weak = weak.clone();
        ui.on_select_stack(move |run_id, stack_id| {
            let Some(ui) = weak.upgrade() else { return; };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let (run_id, stack_id) = (run_id.to_string(), stack_id.to_string());
            run_action(&weak, "Selecting stack", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let res = api.select_stack(&run_id, stack_id)?;
                Ok((Some(run_id), format!("Stack selected:\n{}", pretty(&res))))
            });
        });
    }

    // Load Steps
    {
        let weak = weak.clone();
        ui.on_load_steps(move |run_id| {
            let Some(ui) = weak.upgrade() else { return; };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            run_action(&weak, "Loading steps", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let res = api.list_steps(&run_id)?;
                Ok((Some(run_id), format!("Steps:\n{}", pretty(&res))))
            });
        });
    }

    // Load Timeline
    {
        let weak = weak.clone();
        ui.on_load_timeline(move |run_id| {
            let Some(ui) = weak.upgrade() else { return; };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            run_action(&weak, "Loading timeline", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let res = api.timeline(&run_id)?;
                Ok((Some(run_id), format!("Timeline:\n{}", pretty(&res))))
            });
        });
    }

    // Approve Deploy
    {
        let weak = weak.clone();
        ui.on_approve_deploy(move |run_id| {
            let Some(ui) = weak.upgrade() else { return; };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            run_action(&weak, "Approving deploy", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let res = api.approve_deploy(&run_id)?;
                Ok((Some(run_id), format!("Deploy approved:\n{}", pretty(&res))))
            });
        });
    }

    // Reject Deploy
    {
        let weak = weak.clone();
        ui.on_reject_deploy(move |run_id| {
            let Some(ui) = weak.upgrade() else { return; };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();
            run_action(&weak, "Rejecting deploy", move || {
                let run_id = require_run_id(run_id)?;
                let api = OrchestratorApi::new(base_url)?;
                let res = api.reject_deploy(&run_id)?;
                Ok((Some(run_id), format!("Deploy rejected:\n{}", pretty(&res))))
            });
        });
    }

    // Load Connections
    {
        let weak = weak.clone();
        ui.on_load_connections(move || {
            let Some(ui) = weak.upgrade() else { return; };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            run_action(&weak, "Loading connections", move || {
                let api = OrchestratorApi::new(base_url)?;
                Ok((None, format!("Connections:\n{}", pretty(&api.list_connections()?))))
            });
        });
    }

    // Load Metrics
    {
        let weak = weak.clone();
        ui.on_load_metrics(move || {
            let Some(ui) = weak.upgrade() else { return; };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            run_action(&weak, "Loading metrics", move || {
                let api = OrchestratorApi::new(base_url)?;
                Ok((None, format!("Metrics:\n{}", pretty(&api.metrics()?))))
            });
        });
    }

    // Start SSE
    {
        let weak = weak.clone();
        let sse_stop = Arc::clone(&sse_stop);
        let step_map = Arc::clone(&step_map);
        ui.on_start_sse(move |run_id| {
            let Some(ui) = weak.upgrade() else { return; };
            let base_url = ui.get_base_url().to_string();
            drop(ui);
            let run_id = run_id.to_string();

            sse_stop.store(false, Ordering::SeqCst);

            // Reset state
            {
                let mut map = step_map.lock().unwrap();
                map.clear();
            }
            let weak2 = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = weak2.upgrade() {
                    u.set_steps_text("".into());
                    u.set_has_mockup(false);
                    u.set_output_text("".into());
                    u.set_preview_url("".into());
                    u.set_pr_url("".into());
                    u.set_status_text("SSE Connecting".into());
                }
            });

            let weak_t = weak.clone();
            let sse_stop_t = Arc::clone(&sse_stop);
            let step_map_t = Arc::clone(&step_map);

            thread::spawn(move || {
                let run_id = match require_run_id(run_id) {
                    Ok(v) => v,
                    Err(e) => {
                        set_status_and_output(&weak_t, "Error".into(), format!("ERROR: {}", e));
                        return;
                    }
                };
                let api = match OrchestratorApi::new(base_url) {
                    Ok(v) => v,
                    Err(e) => {
                        set_status_and_output(&weak_t, "Error".into(), format!("ERROR: {}", e));
                        return;
                    }
                };
                let response = match api.open_events(&run_id) {
                    Ok(v) => v,
                    Err(e) => {
                        set_status_and_output(&weak_t, "Error".into(), format!("ERROR: {}", e));
                        return;
                    }
                };

                {
                    let w = weak_t.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(u) = w.upgrade() {
                            u.set_status_text("SSE Connected".into());
                        }
                    });
                }

                let mut reader = BufReader::new(response);
                let mut line = String::new();
                let mut current_event = String::from("message");
                let mut data_lines: Vec<String> = Vec::new();

                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(e) => {
                            set_status_and_output(&weak_t, "Error".into(),
                                format!("SSE read error: {}", e));
                            return;
                        }
                    }
                    if sse_stop_t.load(Ordering::SeqCst) { break; }

                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    if trimmed.is_empty() {
                        if !data_lines.is_empty() {
                            let payload = data_lines.join("\n");
                            data_lines.clear();

                            if let Ok(event) = serde_json::from_str::<SseEvent>(&payload) {
                                // Update step status panel
                                match &event {
                                    SseEvent::StepLog { message, .. } => {
                                        // Extract preview_url / pr_url from deploy step logs
                                        let extract_field = |msg: &str, field: &str| -> Option<String> {
                                            let needle = format!("{}=", field);
                                            let start = msg.find(&needle)? + needle.len();
                                            let rest = &msg[start..];
                                            let end = rest.find(',').unwrap_or(rest.len());
                                            let val = rest[..end].trim().to_string();
                                            if val.is_empty() { None } else { Some(val) }
                                        };
                                        if let Some(url) = extract_field(message, "preview_url") {
                                            let w = weak_t.clone();
                                            let _ = slint::invoke_from_event_loop(move || {
                                                if let Some(u) = w.upgrade() { u.set_preview_url(url.into()); }
                                            });
                                        }
                                        if let Some(url) = extract_field(message, "pr_url") {
                                            let w = weak_t.clone();
                                            let _ = slint::invoke_from_event_loop(move || {
                                                if let Some(u) = w.upgrade() { u.set_pr_url(url.into()); }
                                            });
                                        }
                                        if let Some(eq) = message.find('=') {
                                            let key = message[..eq].trim().to_string();
                                            let rest = &message[eq + 1..];
                                            let status = rest
                                                .split(|c: char| c == ' ' || c == '(')
                                                .next()
                                                .unwrap_or("")
                                                .to_string();
                                            if !key.is_empty() && !status.is_empty() {
                                                let mut map = step_map_t.lock().unwrap();
                                                map.insert(key, status);
                                                update_steps_ui(&weak_t, &map);
                                            }
                                        }
                                    }
                                    SseEvent::StateChanged { status, .. } => {
                                        // Update header badge
                                        let s = status.as_str().to_string();
                                        let w = weak_t.clone();
                                        let _ = slint::invoke_from_event_loop(move || {
                                            if let Some(u) = w.upgrade() {
                                                u.set_status_text(s.into());
                                            }
                                        });
                                        // Load mockup image when ready
                                        eprintln!("[SSE] state_changed → {}", status.as_str());
                                        if status.as_str() == "mockup_ready" {
                                            let api2 = api.clone();
                                            let run_id2 = run_id.clone();
                                            let weak2 = weak_t.clone();
                                            thread::spawn(move || {
                                                try_set_mockup_images(&weak2, &api2, &run_id2);
                                            });
                                        }
                                        if status.as_str() == "preview_deployed"
                                            || status.as_str() == "awaiting_approval"
                                        {
                                            let api2 = api.clone();
                                            let run_id2 = run_id.clone();
                                            let weak2 = weak_t.clone();
                                            thread::spawn(move || {
                                                fetch_and_set_preview_url(&weak2, &api2, &run_id2);
                                            });
                                        }
                                    }
                                    SseEvent::RunFailed { .. } => {
                                        let w = weak_t.clone();
                                        let _ = slint::invoke_from_event_loop(move || {
                                            if let Some(u) = w.upgrade() {
                                                u.set_status_text("Error".into());
                                            }
                                        });
                                    }
                                    SseEvent::RunCompleted { .. } => {
                                        let w = weak_t.clone();
                                        let _ = slint::invoke_from_event_loop(move || {
                                            if let Some(u) = w.upgrade() {
                                                u.set_status_text("Done".into());
                                            }
                                        });
                                    }
                                    _ => {}
                                }

                                if let Some(line) = format_sse_event(&event) {
                                    append_output(&weak_t, line);
                                }
                            } else {
                                // Unknown event — show raw
                                append_output(&weak_t, format!("[{}] {}", current_event, payload));
                            }

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

                let stopped = sse_stop_t.load(Ordering::SeqCst);
                let w = weak_t.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = w.upgrade() {
                        if stopped {
                            u.set_status_text("SSE Stopped".into());
                        } else {
                            u.set_status_text("SSE Closed".into());
                        }
                    }
                });
            });
        });
    }

    // Stop SSE
    {
        let weak = weak.clone();
        let sse_stop = Arc::clone(&sse_stop);
        ui.on_stop_sse(move || {
            sse_stop.store(true, Ordering::SeqCst);
            let w = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = w.upgrade() {
                    u.set_status_text("SSE Stopped".into());
                }
            });
        });
    }

    ui.run().context("ui runtime failed")?;
    Ok(())
}
