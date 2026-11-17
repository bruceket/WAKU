//! Server MCP (rmcp 3.x) che inoltra tool e risorse al bridge del desktop.
//!
//! Ogni chiamata apre una connessione nuova al bridge (Hello con token letto
//! da file), invia una richiesta, legge una risposta e chiude: robusto ai
//! riavvii del desktop.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorCode,
    Implementation, JsonObject, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
    ResourceContents, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value, json};

use crate::bridge_client::{BridgeClient, ClientError, NO_SESSION_MESSAGE};
use crate::protocol::{BridgeRequest, BridgeResponse};

pub const SUMMARY_URI: &str = "waku://model/summary";
pub const TOOL_ENVELOPE: &str = "bim_envelope_command";
pub const TOOL_UNDO: &str = "bim_undo";
pub const TOOL_REDO: &str = "bim_redo";

/// Handler MCP di Waku.
#[derive(Clone, Debug)]
pub struct WakuMcp {
    pipe: String,
    token_dir: PathBuf,
}

impl WakuMcp {
    pub fn new(pipe: impl Into<String>, token_dir: impl Into<PathBuf>) -> Self {
        Self {
            pipe: pipe.into(),
            token_dir: token_dir.into(),
        }
    }

    /// Pipe e directory token predefinite dell'utente corrente.
    pub fn from_env() -> std::io::Result<Self> {
        let dir = crate::token::default_token_dir()
            .ok_or_else(|| std::io::Error::other("LOCALAPPDATA non impostata"))?;
        Ok(Self::new(crate::pipe_name()?, dir))
    }

    /// Definizioni dei tre tool esposti.
    pub fn tool_definitions() -> Vec<Tool> {
        let rev = json!({"type": "integer", "minimum": 0,
            "description": "Revisione attesa del modello (controllo di concorrenza ottimistico)."});
        let envelope = json!({
            "type": "object",
            "properties": {
                "request_id": {"type": "string", "description": "Id idempotente della richiesta."},
                "expected_revision": rev,
                "actions": {
                    "type": "array",
                    "description": "Azioni BIM (JSON con discriminante \"op\", es. CreateWall).",
                    "items": {"type": "object", "properties": {"op": {"type": "string"}},
                        "required": ["op"]}
                },
                "preview": {"type": "boolean", "default": false,
                    "description": "Se true valida senza applicare."}
            },
            "required": ["request_id", "expected_revision", "actions"],
            "additionalProperties": false
        });
        let rev_only = json!({
            "type": "object",
            "properties": {"expected_revision": rev},
            "required": ["expected_revision"],
            "additionalProperties": false
        });
        let obj = |v: Value| Arc::new(v.as_object().cloned().unwrap_or_default());
        vec![
            Tool::new(
                TOOL_ENVELOPE,
                "Applica (o anteprima) un insieme atomico di azioni BIM al modello aperto in Waku. \
                 Restituisce {revision, created, preview}.",
                obj(envelope),
            ),
            Tool::new(
                TOOL_UNDO,
                "Annulla l'ultima modifica.",
                obj(rev_only.clone()),
            ),
            Tool::new(
                TOOL_REDO,
                "Ripristina l'ultima modifica annullata.",
                obj(rev_only),
            ),
        ]
    }

    /// Esegue un tool. Errori del bridge e argomenti invalidi diventano
    /// risultati con `isError: true` e testo `"{code}: {message}"`.
    pub async fn call(&self, name: &str, args: JsonObject) -> CallToolResult {
        let req = match parse_request(name, args) {
            Ok(r) => r,
            Err(msg) => return tool_error("invalid", &msg),
        };
        match self.roundtrip(req).await {
            Ok(BridgeResponse::Ok {
                revision,
                created,
                preview,
            }) => CallToolResult::structured(
                json!({"revision": revision, "created": created, "preview": preview}),
            ),
            Ok(other) => tool_error("protocol", &format!("risposta inattesa: {other:?}")),
            Err(e) => client_error(&e),
        }
    }

    /// Legge `waku://model/summary` dal desktop.
    pub async fn read_summary(&self) -> Result<ReadResourceResult, ErrorData> {
        match self.roundtrip(BridgeRequest::Summary).await {
            Ok(BridgeResponse::Summary(v)) => Ok(ReadResourceResult::new(vec![
                ResourceContents::text(v.to_string(), SUMMARY_URI)
                    .with_mime_type("application/json"),
            ])),
            Ok(other) => Err(ErrorData::internal_error(
                format!("protocol: risposta inattesa: {other:?}"),
                None,
            )),
            Err(e) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                e.to_string(),
                Some(json!({"code": e.code()})),
            )),
        }
    }

    async fn roundtrip(&self, req: BridgeRequest) -> Result<BridgeResponse, ClientError> {
        let mut c = BridgeClient::connect(&self.pipe, &self.token_dir).await?;
        c.request(req).await
    }
}

fn tool_error(code: &str, message: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(format!("{code}: {message}"))])
}

fn client_error(e: &ClientError) -> CallToolResult {
    match e {
        ClientError::Bridge { code, message } => tool_error(code, message),
        ClientError::NoSession => tool_error("no_session", NO_SESSION_MESSAGE),
        other => tool_error(other.code(), &other.to_string()),
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeArgs {
    request_id: String,
    expected_revision: u64,
    actions: Vec<commands::Action>,
    #[serde(default)]
    preview: bool,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RevArgs {
    expected_revision: u64,
}

fn parse_request(name: &str, args: JsonObject) -> Result<BridgeRequest, String> {
    let v = Value::Object(args);
    let err = |e: serde_json::Error| format!("argomenti non validi per {name}: {e}");
    Ok(match name {
        TOOL_ENVELOPE => {
            let a: EnvelopeArgs = serde_json::from_value(v).map_err(err)?;
            BridgeRequest::Envelope {
                request_id: a.request_id,
                expected_revision: a.expected_revision,
                actions: a.actions,
                preview: a.preview,
            }
        }
        TOOL_UNDO => BridgeRequest::Undo {
            expected_revision: serde_json::from_value::<RevArgs>(v)
                .map_err(err)?
                .expected_revision,
        },
        TOOL_REDO => BridgeRequest::Redo {
            expected_revision: serde_json::from_value::<RevArgs>(v)
                .map_err(err)?
                .expected_revision,
        },
        other => return Err(format!("tool sconosciuto: {other}")),
    })
}

impl ServerHandler for WakuMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new("waku-mcp", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "Ponte verso il desktop Waku. Leggi waku://model/summary per la revisione corrente, \
             poi usa bim_envelope_command con expected_revision. Le scritture richiedono che \
             l'utente abbia concesso l'accesso AI nel desktop.",
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(Self::tool_definitions()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        Self::tool_definitions()
            .into_iter()
            .find(|t| t.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = request.arguments.unwrap_or_default();
        Ok(self.call(&request.name, args).await.into())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new(SUMMARY_URI, "model-summary")
                .with_description(
                    "Riepilogo JSON del modello aperto (revisione, livelli, muri, spazi).",
                )
                .with_mime_type("application/json"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        if request.uri != SUMMARY_URI {
            return Err(ErrorData::resource_not_found(
                format!("risorsa sconosciuta: {}", request.uri),
                None,
            ));
        }
        Ok(self.read_summary().await?.into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::token::{Token, write_token_file};
    use crate::{CancellationToken, Policy, ServeConfig, pipe_name_with_suffix, serve_with};
    use commands::{CommandService, spawn_writer};
    use domain::fixtures::sample_room;

    struct Fx {
        mcp: WakuMcp,
        policy: Arc<Policy>,
        level: u64,
        stop: CancellationToken,
        dir: PathBuf,
    }

    impl Drop for Fx {
        fn drop(&mut self) {
            self.stop.cancel();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    async fn start(suffix: &str) -> Fx {
        let (m, ids) = sample_room();
        let handle = spawn_writer(CommandService::new(m));
        let tag = format!("mcp-{suffix}-{}", std::process::id());
        let name = pipe_name_with_suffix(&tag).unwrap();
        let dir = std::env::temp_dir().join(format!("waku-{tag}"));
        let token = Token::generate().unwrap();
        write_token_file(&dir, &token).unwrap();
        let policy = Arc::new(Policy::new());
        let stop = CancellationToken::new();
        let (n, p, c) = (name.clone(), policy.clone(), stop.clone());
        tokio::spawn(async move {
            serve_with(&n, ServeConfig::default(), handle, p, token, c)
                .await
                .unwrap()
        });
        // attende che la pipe esista
        for _ in 0..300 {
            if tokio::net::windows::named_pipe::ClientOptions::new()
                .open(&name)
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        Fx {
            mcp: WakuMcp::new(name, &dir),
            policy,
            level: ids.level.get(),
            stop,
            dir,
        }
    }

    fn args(v: Value) -> JsonObject {
        v.as_object().unwrap().clone()
    }

    fn text(r: &CallToolResult) -> String {
        r.content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect()
    }

    #[test]
    fn tools_list_has_three_tools() {
        let tools = WakuMcp::tool_definitions();
        let names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        assert_eq!(names, [TOOL_ENVELOPE, TOOL_UNDO, TOOL_REDO]);
        let env = &tools[0].input_schema;
        assert_eq!(
            env["required"],
            json!(["request_id", "expected_revision", "actions"])
        );
        assert_eq!(env["properties"]["expected_revision"]["type"], "integer");
        assert_eq!(env["properties"]["actions"]["type"], "array");
        assert_eq!(env["properties"]["preview"]["type"], "boolean");
        for t in &tools[1..] {
            assert_eq!(t.input_schema["required"], json!(["expected_revision"]));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn summary_resource_roundtrip() {
        let fx = start("summary").await;
        let r = fx.mcp.read_summary().await.unwrap();
        let ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        } = &r.contents[0]
        else {
            panic!("contenuto non testuale");
        };
        assert_eq!(uri, SUMMARY_URI);
        assert_eq!(mime_type.as_deref(), Some("application/json"));
        let v: Value = serde_json::from_str(text).unwrap();
        assert_eq!(v["revision"], 0);
        assert!(v["walls"].as_u64().unwrap() > 0, "{v}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn no_session_error_message() {
        let dir = std::env::temp_dir().join("waku-mcp-nosession-missing");
        let mcp = WakuMcp::new(pipe_name_with_suffix("mcp-none-xyz").unwrap(), dir);
        let r = mcp
            .call(TOOL_UNDO, args(json!({"expected_revision": 0})))
            .await;
        assert_eq!(r.is_error, Some(true));
        assert_eq!(
            text(&r),
            "no_session: Nessuna sessione Waku aperta. Avvia Waku e concedi l'accesso AI."
        );
        let e = mcp.read_summary().await.unwrap_err();
        assert!(e.message.contains("Nessuna sessione Waku aperta"), "{e:?}");
    }

    fn wall(fx: &Fx, preview: bool) -> JsonObject {
        args(
            json!({"request_id":"r1","expected_revision":0,"preview":preview,
            "actions":[{"op":"CreateWall","level":fx.level,"start":[0.0,5000.0],
                "end":[3000.0,5000.0],"thickness_mm":100.0,"height_mm":2700.0}]}),
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn envelope_forbidden_then_ok_then_undo_redo() {
        let fx = start("envelope").await;
        let r = fx.mcp.call(TOOL_ENVELOPE, wall(&fx, false)).await;
        assert_eq!(r.is_error, Some(true));
        assert!(text(&r).starts_with("forbidden: "), "{}", text(&r));

        fx.policy.grant();
        let r = fx.mcp.call(TOOL_ENVELOPE, wall(&fx, false)).await;
        assert_eq!(r.is_error, Some(false), "{}", text(&r));
        let v = r.structured_content.clone().unwrap();
        assert_eq!(v["revision"], 1);
        assert_eq!(v["preview"], false);
        assert_eq!(v["created"].as_array().unwrap().len(), 1);

        let r = fx
            .mcp
            .call(TOOL_UNDO, args(json!({"expected_revision": 1})))
            .await;
        assert_eq!(r.structured_content.unwrap()["revision"], 2);
        let r = fx
            .mcp
            .call(TOOL_REDO, args(json!({"expected_revision": 2})))
            .await;
        assert_eq!(r.structured_content.unwrap()["revision"], 3);

        let r = fx
            .mcp
            .call(TOOL_UNDO, args(json!({"expected_revision": 0})))
            .await;
        assert!(text(&r).starts_with("conflict: "), "{}", text(&r));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_arguments_are_tool_errors() {
        let fx = start("badargs").await;
        let r = fx.mcp.call(TOOL_UNDO, args(json!({}))).await;
        assert_eq!(r.is_error, Some(true));
        assert!(text(&r).starts_with("invalid: "), "{}", text(&r));
        let r = fx.mcp.call("nope", args(json!({}))).await;
        assert_eq!(r.is_error, Some(true));
    }
}
