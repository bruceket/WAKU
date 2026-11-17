//! Smoke test stdio: avvia `waku-mcp` e parla JSON-RPC MCP su stdin/stdout,
//! senza desktop (LOCALAPPDATA vuota -> `no_session`).

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Proc {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
}

impl Proc {
    fn start(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("waku-smoke-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_waku-mcp"))
            .env("LOCALAPPDATA", &dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let out = BufReader::new(child.stdout.take().unwrap());
        Proc { child, stdin, out }
    }

    fn send(&mut self, v: Value) {
        writeln!(self.stdin, "{v}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn recv(&mut self, id: u64) -> Value {
        loop {
            let mut line = String::new();
            assert!(self.out.read_line(&mut line).unwrap() > 0, "stdout chiuso");
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["id"] == id {
                return v;
            }
        }
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn check_tools_and_summary(p: &mut Proc, meta: Value) {
    p.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":meta}}));
    let r = p.recv(2);
    let names: Vec<_> = r["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("{r}"))
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["bim_envelope_command", "bim_undo", "bim_redo"]);

    p.send(json!({"jsonrpc":"2.0","id":3,"method":"resources/read",
        "params":{"uri":"waku://model/summary","_meta":meta}}));
    let r = p.recv(3);
    let msg = r["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("{r}"));
    assert_eq!(
        msg,
        "no_session: Nessuna sessione Waku aperta. Avvia Waku e concedi l'accesso AI."
    );
    assert_eq!(r["error"]["data"]["code"], "no_session");

    p.send(json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
        "params":{"name":"bim_undo","arguments":{"expected_revision":0},"_meta":meta}}));
    let r = p.recv(4);
    assert_eq!(r["result"]["isError"], true, "{r}");
    assert!(
        r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("no_session: "),
        "{r}"
    );
}

#[test]
fn stdio_initialize_2025_11_25() {
    let mut p = Proc::start("init");
    p.send(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-11-25","capabilities":{},
        "clientInfo":{"name":"smoke","version":"0"}}}),
    );
    let r = p.recv(1);
    assert_eq!(r["result"]["protocolVersion"], "2025-11-25", "{r}");
    assert_eq!(r["result"]["serverInfo"]["name"], "waku-mcp");
    p.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    check_tools_and_summary(&mut p, json!({}));
}

#[test]
fn stdio_stateless_2026_07_28() {
    let mut p = Proc::start("stateless");
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name":"smoke","version":"0"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    p.send(json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":meta}}));
    let r = p.recv(1);
    let versions = r["result"]["supportedVersions"].to_string();
    assert!(versions.contains("2026-07-28"), "{r}");
    check_tools_and_summary(&mut p, meta);
}
