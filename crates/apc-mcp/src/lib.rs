//! The MCP server: tools mirror the service's method registry (writes go through the
//! command bus, so they are logged, undoable and show up live in the open app);
//! resources are read-only views (script versions, cut list, scene buckets, match report).

use std::sync::{Arc, Mutex};

use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Value};

use apc_app::{methods, Access, Session};

#[derive(Clone)]
pub struct CutawayMcp {
    pub session: Arc<Mutex<Session>>,
}

impl CutawayMcp {
    pub fn new(session: Session) -> Self {
        CutawayMcp {
            session: Arc::new(Mutex::new(session)),
        }
    }

    /// Calls a method on the session from a blocking thread.
    async fn call(&self, name: String, params: Value) -> Result<Value, apc_app::AppError> {
        let s = self.session.clone();
        tokio::task::spawn_blocking(move || {
            let mut g = s
                .lock()
                .map_err(|_| apc_app::AppError::Other("session lock poisoned".into()))?;
            g.call(&name, &params)
        })
        .await
        .map_err(|e| apc_app::AppError::Other(e.to_string()))?
    }
}

fn object(v: Value) -> Arc<JsonObject> {
    match v {
        Value::Object(m) => Arc::new(m),
        _ => Arc::new(serde_json::Map::new()),
    }
}

/// The MCP tool list (destructive tools say so and need `confirm: true`).
pub fn tools() -> Vec<Tool> {
    methods()
        .into_iter()
        .filter(|m| m.mcp)
        .map(|m| {
            let desc = match m.access {
                Access::Destructive => {
                    format!("{} Destructive: pass \"confirm\": true.", m.description)
                }
                _ => m.description.to_string(),
            };
            Tool::new(m.name, desc, object((m.schema)()))
        })
        .collect()
}

fn text_resource(uri: &str, text: String) -> ReadResourceResult {
    ReadResourceResult::new(vec![ResourceContents::text(text, uri)])
}

impl ServerHandler for CutawayMcp {
    fn get_info(&self) -> ServerConfig {
        let mut info = InitializeResult::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        );
        info.server_info = Implementation::new(apc_core::brand::SLUG, env!("CARGO_PKG_VERSION"));
        info.instructions = Some(format!(
            "{} edits audio play recordings into script-matched rough cuts. Open an episode with open_project (or start the server with --project), check project_status, run run_alignment after importing, then pick takes, spot sounds and export. Every write is undoable (undo).",
            apc_core::brand::DISPLAY_NAME
        ));
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let name = request.name.to_string();
        if !methods().iter().any(|m| m.mcp && m.name == name) {
            return Err(McpError::invalid_params(
                format!("unknown tool '{name}'"),
                None,
            ));
        }
        let params = request.arguments.map(Value::Object).unwrap_or(json!({}));
        let result = match self.call(name, params).await {
            Ok(v) => {
                let mut r = CallToolResult::success(vec![ContentBlock::text(
                    serde_json::to_string_pretty(&v).unwrap_or_default(),
                )]);
                r.structured_content = Some(json!({ "result": v }));
                r
            }
            Err(e) => {
                CallToolResult::error(vec![ContentBlock::text(format!("{} ({})", e, e.code()))])
            }
        };
        Ok(result.into())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let s = self.session.clone();
        let list = tokio::task::spawn_blocking(move || -> Vec<Resource> {
            let Ok(mut g) = s.lock() else { return vec![] };
            let Some(svc) = g.service.as_mut() else {
                return vec![];
            };
            let _ = svc.bus.refresh();
            let mut out = vec![
                Resource::new("cutaway://cutlist", "Cut list").with_mime_type("text/markdown"),
                Resource::new("cutaway://report", "Match report").with_mime_type("text/markdown"),
            ];
            let mut versions: Vec<&apc_core::ScriptVersion> =
                svc.doc().script_versions.values().collect();
            versions.sort_by_key(|v| v.number);
            for v in versions {
                out.push(
                    Resource::new(
                        format!("cutaway://script/{}", v.number),
                        format!("Script v{:03} {}", v.number, v.name),
                    )
                    .with_mime_type("text/plain"),
                );
            }
            for sc in svc.doc().scenes_in_order() {
                out.push(
                    Resource::new(
                        format!("cutaway://bucket/{}", sc.number),
                        format!("Sound bucket S{} {}", sc.number, sc.heading),
                    )
                    .with_mime_type("application/json"),
                );
            }
            out
        })
        .await
        .unwrap_or_default();
        Ok(ListResourcesResult::with_all_items(list))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let uri = request.uri.clone();
        let s = self.session.clone();
        let r = tokio::task::spawn_blocking(move || -> Result<String, String> {
            let mut g = s.lock().map_err(|_| "lock".to_string())?;
            let svc = g.service.as_mut().ok_or("no project is open")?;
            let _ = svc.bus.refresh();
            let rest = uri.strip_prefix("cutaway://").ok_or("unknown resource")?;
            if rest == "cutlist" {
                let tl =
                    apc_export::EpisodeTimeline::build(svc.doc(), &apc_export::ExportScope::All);
                let rows = apc_export::cutlist::rows(svc.doc(), &svc.paths, &tl);
                return Ok(apc_export::cutlist::markdown(&svc.name(), &rows));
            }
            if rest == "report" {
                return svc
                    .call("match_report", &json!({}))
                    .map(|v| v["markdown"].as_str().unwrap_or_default().to_string())
                    .map_err(|e| e.to_string());
            }
            if let Some(n) = rest.strip_prefix("script/") {
                let n: u32 = n.parse().map_err(|_| "bad version")?;
                let v = svc
                    .doc()
                    .script_versions
                    .values()
                    .find(|v| v.number == n)
                    .ok_or("no such version")?;
                return std::fs::read_to_string(svc.paths.resolve(&v.file))
                    .map_err(|e| e.to_string());
            }
            if let Some(sc) = rest.strip_prefix("bucket/") {
                return svc
                    .call("list_bucket", &json!({ "scene": sc }))
                    .map(|v| serde_json::to_string_pretty(&v).unwrap_or_default())
                    .map_err(|e| e.to_string());
            }
            Err("unknown resource".into())
        })
        .await
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        match r {
            Ok(text) => Ok(text_resource(&request.uri, text).into()),
            Err(e) => Err(McpError::resource_not_found(e, None)),
        }
    }
}

/// Runs the server on stdio until the client disconnects.
pub async fn serve_stdio(session: Session) -> anyhow::Result<()> {
    let server = CutawayMcp::new(session);
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

/// Blocking entry point for the CLI (`cutaway mcp`).
pub fn run_stdio(session: Session) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(serve_stdio(session))
}
