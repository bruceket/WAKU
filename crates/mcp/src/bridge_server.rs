//! Server bridge su named pipe Windows.
//!
//! Sicurezza G0: la pipe usa la DACL predefinita (utente creatore, SYSTEM,
//! amministratori) con `reject_remote_clients(true)` e `first_pipe_instance(true)`;
//! ogni connessione deve autenticarsi con `Hello{token}` (confronto a tempo
//! costante). Tutte le scritture hanno `Origin::Ai` e richiedono il grant GUI,
//! tranne le preview (proposte). Su piattaforme non Windows `serve` restituisce
//! `BridgeError::Unsupported`.
//!
//! Robustezza: timeout handshake (5 s) e di inattivita' (300 s), massimo 16
//! connessioni (oltre: `busy`), accept loop che ritenta con backoff e termina
//! solo su shutdown.
//!
//! TOCTOU noto: la policy e' verificata all'arrivo della richiesta; una
//! scrittura gia' accodata al writer puo' completarsi dopo `Policy::revoke()`.

use std::sync::Arc;
use std::time::Duration;

use commands::{
    Command, CommandError, Origin, Request, RequestId, Response, Revision, WriterHandle,
    model_summary,
};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite};
#[cfg(windows)]
use tokio::sync::Semaphore;
use tokio::sync::watch;

use crate::policy::Policy;
use crate::protocol::*;
use crate::token::{Token, hex};

/// Timeout per ricevere `Hello` dopo la connessione.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// Inattivita' massima tra due richieste.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
/// Connessioni contemporanee massime; oltre si risponde `busy`.
pub const MAX_CONNECTIONS: usize = 16;
#[cfg(windows)]
const MIN_BACKOFF: Duration = Duration::from_millis(100);
#[cfg(windows)]
const MAX_BACKOFF: Duration = Duration::from_secs(2);

/// Parametri del server (configurabili per i test).
#[derive(Copy, Clone, Debug)]
pub struct ServeConfig {
    pub handshake_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_connections: usize,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            handshake_timeout: HANDSHAKE_TIMEOUT,
            idle_timeout: IDLE_TIMEOUT,
            max_connections: MAX_CONNECTIONS,
        }
    }
}

/// Segnale di arresto minimale (sostituto locale di `tokio_util::sync::CancellationToken`).
#[derive(Clone, Debug)]
pub struct CancellationToken {
    tx: Arc<watch::Sender<bool>>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            tx: Arc::new(watch::channel(false).0),
        }
    }
    pub fn cancel(&self) {
        self.tx.send_replace(true);
    }
    pub fn is_cancelled(&self) -> bool {
        *self.tx.borrow()
    }
    pub async fn cancelled(&self) {
        let mut rx = self.tx.subscribe();
        let _ = rx.wait_for(|v| *v).await;
    }
}

fn name_from_sid(sid: &str, suffix: &str) -> String {
    let digest = Sha256::digest(sid.as_bytes());
    format!(r"\\.\pipe\waku-{}{suffix}", &hex(&digest)[..16])
}

/// `\\.\pipe\waku-` + primi 16 hex di SHA-256 del SID dell'utente corrente.
pub fn pipe_name() -> std::io::Result<String> {
    Ok(name_from_sid(&sid::current_user_sid()?, ""))
}

/// Come `pipe_name` con un suffisso (test paralleli).
pub fn pipe_name_with_suffix(suffix: &str) -> std::io::Result<String> {
    Ok(name_from_sid(
        &sid::current_user_sid()?,
        &format!("-{suffix}"),
    ))
}

#[cfg(windows)]
mod sid {
    use std::io;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    pub fn current_user_sid() -> io::Result<String> {
        // SAFETY: chiamate Win32 documentate; buffer dimensionato dalla prima
        // chiamata e allineato a u64; handle e stringa liberati in ogni ramo.
        unsafe {
            let mut tok: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut len = 0u32;
            GetTokenInformation(tok, TokenUser, std::ptr::null_mut(), 0, &mut len);
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            let ok = GetTokenInformation(tok, TokenUser, buf.as_mut_ptr().cast(), len, &mut len);
            let err = io::Error::last_os_error();
            CloseHandle(tok);
            if ok == 0 {
                return Err(err);
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut s) == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut n = 0;
            while *s.add(n) != 0 {
                n += 1;
            }
            let out = String::from_utf16_lossy(std::slice::from_raw_parts(s, n));
            LocalFree(s.cast());
            Ok(out)
        }
    }
}

#[cfg(not(windows))]
mod sid {
    pub fn current_user_sid() -> std::io::Result<String> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "solo Windows",
        ))
    }
}

/// Avvia il server sulla pipe predefinita dell'utente.
pub async fn serve(
    handle: WriterHandle,
    policy: Arc<Policy>,
    token: Token,
    shutdown: CancellationToken,
) -> Result<(), BridgeError> {
    let name = pipe_name()?;
    serve_on(&name, handle, policy, token, shutdown).await
}

/// Stub non Windows (R5).
#[cfg(not(windows))]
pub async fn serve_with(
    _name: &str,
    _cfg: ServeConfig,
    _handle: WriterHandle,
    _policy: Arc<Policy>,
    _token: Token,
    _shutdown: CancellationToken,
) -> Result<(), BridgeError> {
    Err(BridgeError::Unsupported)
}

/// Stub non Windows: il bridge in G0 esiste solo su Windows (R5).
#[cfg(not(windows))]
pub async fn serve_on(
    _name: &str,
    _handle: WriterHandle,
    _policy: Arc<Policy>,
    _token: Token,
    _shutdown: CancellationToken,
) -> Result<(), BridgeError> {
    Err(BridgeError::Unsupported)
}

/// Avvia il server su una pipe con nome esplicito e configurazione predefinita.
#[cfg(windows)]
pub async fn serve_on(
    name: &str,
    handle: WriterHandle,
    policy: Arc<Policy>,
    token: Token,
    shutdown: CancellationToken,
) -> Result<(), BridgeError> {
    serve_with(
        name,
        ServeConfig::default(),
        handle,
        policy,
        token,
        shutdown,
    )
    .await
}

/// Attesa cancellabile; `false` se e' arrivato lo shutdown.
#[cfg(windows)]
async fn backoff_sleep(d: &mut Duration, shutdown: &CancellationToken) -> bool {
    let wait = *d;
    *d = (*d * 2).min(MAX_BACKOFF);
    tokio::select! {
        _ = shutdown.cancelled() => false,
        _ = tokio::time::sleep(wait) => true,
    }
}

/// Ciclo di accept resiliente: errori di `create`/`connect` (anche ripetuti)
/// non terminano il server; si ritenta con backoff 100 ms -> 2 s. Il ciclo
/// termina solo con `shutdown`. Oltre `max_connections` il client riceve
/// `busy` e la connessione viene chiusa.
#[cfg(windows)]
pub async fn serve_with(
    name: &str,
    cfg: ServeConfig,
    handle: WriterHandle,
    policy: Arc<Policy>,
    token: Token,
    shutdown: CancellationToken,
) -> Result<(), BridgeError> {
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
    let slots = Arc::new(Semaphore::new(cfg.max_connections));
    let mut server: Option<NamedPipeServer> = None;
    let mut first = true;
    let mut backoff = MIN_BACKOFF;
    loop {
        if server.is_none() {
            let mut opts = ServerOptions::new();
            opts.reject_remote_clients(true);
            if first {
                opts.first_pipe_instance(true);
            }
            match opts.create(name) {
                Ok(s) => {
                    server = Some(s);
                    first = false;
                }
                Err(_) => {
                    if !backoff_sleep(&mut backoff, &shutdown).await {
                        return Ok(());
                    }
                    continue;
                }
            }
        }
        let r = {
            let srv = server.as_ref().expect("istanza creata");
            tokio::select! {
                _ = shutdown.cancelled() => return Ok(()),
                r = srv.connect() => r,
            }
        };
        let conn = server.take().expect("istanza creata");
        if r.is_err() {
            drop(conn);
            if !backoff_sleep(&mut backoff, &shutdown).await {
                return Ok(());
            }
            continue;
        }
        backoff = MIN_BACKOFF;
        match slots.clone().try_acquire_owned() {
            Ok(permit) => {
                let (h, p, t) = (handle.clone(), policy.clone(), token.clone());
                tokio::spawn(async move {
                    let _ = handle_conn(conn, cfg, h, p, t).await;
                    drop(permit);
                });
            }
            Err(_) => {
                tokio::spawn(async move {
                    let mut conn = conn;
                    let busy = BridgeResponse::err("busy", "troppe connessioni attive");
                    let _ =
                        tokio::time::timeout(cfg.handshake_timeout, send(&mut conn, &busy)).await;
                });
            }
        }
    }
}

/// Lettura con timeout; un timeout equivale a chiusura (`Ok(None)`).
async fn read_timed<S: AsyncRead + Unpin>(
    s: &mut S,
    d: Duration,
) -> Result<Option<Vec<u8>>, BridgeError> {
    tokio::time::timeout(d, read_frame(s))
        .await
        .unwrap_or(Ok(None))
}

async fn send<S: AsyncWrite + Unpin>(s: &mut S, resp: &BridgeResponse) -> Result<(), BridgeError> {
    write_frame(s, &serde_json::to_vec(resp)?).await
}

/// Gestisce una connessione: handshake, poi richieste fino a EOF o errore.
pub(crate) async fn handle_conn<S: AsyncRead + AsyncWrite + Unpin>(
    mut s: S,
    cfg: ServeConfig,
    handle: WriterHandle,
    policy: Arc<Policy>,
    token: Token,
) -> Result<(), BridgeError> {
    // Handshake: il primo frame deve essere Hello con token valido.
    let first = match read_timed(&mut s, cfg.handshake_timeout).await {
        Ok(Some(b)) => b,
        Ok(None) => return Ok(()),
        Err(BridgeError::TooLarge(_)) => {
            return send(
                &mut s,
                &BridgeResponse::err("too_large", "frame oltre 1 MiB"),
            )
            .await;
        }
        Err(e) => return Err(e),
    };
    match serde_json::from_slice::<BridgeRequest>(&first) {
        Ok(BridgeRequest::Hello { token: t, protocol }) if token.matches(&t) => {
            if !supported_protocols().contains(&protocol) {
                let msg = format!(
                    "protocollo {protocol} non supportato; supportati: {:?}",
                    supported_protocols()
                );
                return send(&mut s, &BridgeResponse::err("unsupported_protocol", msg)).await;
            }
        }
        _ => {
            return send(
                &mut s,
                &BridgeResponse::err("bad_token", "handshake non valido"),
            )
            .await;
        }
    }
    let rev = blocking(&handle, Request::Snapshot).await;
    let ack = match rev {
        Some(Response::Snapshot(snap)) => BridgeResponse::Ok {
            revision: snap.revision.0,
            created: vec![],
            preview: false,
        },
        _ => BridgeResponse::err("invalid", "writer non disponibile"),
    };
    send(&mut s, &ack).await?;

    loop {
        let body = match read_timed(&mut s, cfg.idle_timeout).await {
            Ok(Some(b)) => b,
            Ok(None) => return Ok(()),
            Err(BridgeError::TooLarge(n)) => {
                // Corpo non letto: si risponde e si chiude la connessione.
                let msg = format!("frame di {n} byte oltre il limite di {MAX_FRAME}");
                return send(&mut s, &BridgeResponse::err("too_large", msg)).await;
            }
            Err(e) => return Err(e),
        };
        let resp = match serde_json::from_slice::<BridgeRequest>(&body) {
            Ok(req) => dispatch(req, &handle, &policy).await,
            Err(e) => BridgeResponse::err("invalid", format!("richiesta non valida: {e}")),
        };
        send(&mut s, &resp).await?;
    }
}

/// Esegue la chiamata bloccante al writer fuori dal runtime async.
async fn blocking(handle: &WriterHandle, req: Request) -> Option<Response> {
    let h = handle.clone();
    tokio::task::spawn_blocking(move || h.send(req).recv().ok())
        .await
        .ok()
        .flatten()
}

fn map_err(e: CommandError) -> BridgeResponse {
    let code = match e {
        CommandError::Conflict { .. } => "conflict",
        CommandError::Forbidden => "forbidden",
        _ => "invalid",
    };
    BridgeResponse::err(code, e.to_string())
}

async fn dispatch(req: BridgeRequest, handle: &WriterHandle, policy: &Policy) -> BridgeResponse {
    let forbidden = || BridgeResponse::err("forbidden", "scrittura AI non concessa dall'utente");
    let call = match req {
        BridgeRequest::Hello { .. } => {
            return BridgeResponse::err("invalid", "Hello gia' ricevuto");
        }
        BridgeRequest::Summary => {
            return match blocking(handle, Request::Snapshot).await {
                Some(Response::Snapshot(snap)) => BridgeResponse::Summary(model_summary(&snap)),
                _ => BridgeResponse::err("invalid", "writer non disponibile"),
            };
        }
        BridgeRequest::Envelope {
            request_id,
            expected_revision,
            actions,
            preview,
        } => {
            if !preview && !policy.allows_write() {
                return forbidden();
            }
            Request::Execute(Command {
                request_id: RequestId(request_id),
                expected: Revision(expected_revision),
                origin: Origin::Ai,
                actions,
                preview,
            })
        }
        BridgeRequest::Undo { expected_revision } => {
            if !policy.allows_write() {
                return forbidden();
            }
            Request::Undo {
                expected: Revision(expected_revision),
                origin: Origin::Ai,
            }
        }
        BridgeRequest::Redo { expected_revision } => {
            if !policy.allows_write() {
                return forbidden();
            }
            Request::Redo {
                expected: Revision(expected_revision),
                origin: Origin::Ai,
            }
        }
    };
    match blocking(handle, call).await {
        Some(Response::Done(Ok(o))) => BridgeResponse::Ok {
            revision: o.revision.0,
            created: o.created.into_iter().map(|id| id.get()).collect(),
            preview: o.preview,
        },
        Some(Response::Done(Err(e))) => map_err(e),
        _ => BridgeResponse::err("invalid", "writer non disponibile"),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use commands::{CommandService, spawn_writer};
    use domain::fixtures::sample_room;
    use serde_json::{Value, json};
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

    struct Fx {
        name: String,
        policy: Arc<Policy>,
        token: Token,
        level: u64,
        stop: CancellationToken,
    }

    impl Drop for Fx {
        fn drop(&mut self) {
            self.stop.cancel();
        }
    }

    fn start(suffix: &str) -> Fx {
        start_with(suffix, ServeConfig::default())
    }

    fn start_with(suffix: &str, cfg: ServeConfig) -> Fx {
        let (m, ids) = sample_room();
        let handle = spawn_writer(CommandService::new(m));
        let name = pipe_name_with_suffix(&format!("{suffix}-{}", std::process::id())).unwrap();
        let policy = Arc::new(Policy::new());
        let token = Token::generate().unwrap();
        let stop = CancellationToken::new();
        let (n, p, t, c) = (name.clone(), policy.clone(), token.clone(), stop.clone());
        tokio::spawn(async move { serve_with(&n, cfg, handle, p, t, c).await.unwrap() });
        Fx {
            name,
            policy,
            token,
            level: ids.level.get(),
            stop,
        }
    }

    async fn connect(name: &str) -> NamedPipeClient {
        for _ in 0..300 {
            if let Ok(c) = ClientOptions::new().open(name) {
                return c;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("pipe non disponibile");
    }

    async fn call(c: &mut NamedPipeClient, req: Value) -> Option<Value> {
        write_frame(c, &serde_json::to_vec(&req).unwrap())
            .await
            .ok()?;
        let body = read_frame(c).await.ok()??;
        Some(serde_json::from_slice(&body).unwrap())
    }

    async fn hello(fx: &Fx) -> NamedPipeClient {
        let mut c = connect(&fx.name).await;
        let r = call(
            &mut c,
            json!({"type":"Hello","token":fx.token.as_str(),"protocol":1}),
        )
        .await
        .unwrap();
        assert_eq!(r["type"], "Ok", "{r}");
        c
    }

    fn wall(fx: &Fx, id: &str, rev: u64, preview: bool) -> Value {
        json!({"type":"Envelope","request_id":id,"expected_revision":rev,"preview":preview,
            "actions":[{"op":"CreateWall","level":fx.level,"start":[0.0,5000.0],"end":[3000.0,5000.0],
                "thickness_mm":100.0,"height_mm":2700.0}]})
    }

    fn code(v: &Value) -> &str {
        assert_eq!(v["type"], "Err", "{v}");
        v["body"]["code"].as_str().unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ac18_write_without_grant_forbidden() {
        let fx = start("ac18");
        let mut c = hello(&fx).await;
        let r = call(&mut c, wall(&fx, "a", 0, false)).await.unwrap();
        assert_eq!(code(&r), "forbidden");
        let r = call(&mut c, json!({"type":"Undo","expected_revision":0}))
            .await
            .unwrap();
        assert_eq!(code(&r), "forbidden");
        let r = call(&mut c, json!({"type":"Redo","expected_revision":0}))
            .await
            .unwrap();
        assert_eq!(code(&r), "forbidden");
        // la preview e' ammessa senza grant e non crea revisioni
        let r = call(&mut c, wall(&fx, "p", 0, true)).await.unwrap();
        assert_eq!(r["body"]["preview"], true, "{r}");
        let s = call(&mut c, json!({"type":"Summary"})).await.unwrap();
        assert_eq!(s["body"]["revision"], 0);
        assert_eq!(s["body"]["walls"], 4);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn write_after_grant_ok() {
        let fx = start("grant");
        let mut c = hello(&fx).await;
        fx.policy.grant();
        let r = call(&mut c, wall(&fx, "a", 0, false)).await.unwrap();
        assert_eq!(r["type"], "Ok", "{r}");
        assert_eq!(r["body"]["revision"], 1);
        assert_eq!(r["body"]["created"].as_array().unwrap().len(), 1);
        let r = call(&mut c, wall(&fx, "b", 0, false)).await.unwrap();
        assert_eq!(code(&r), "conflict");
        let r = call(&mut c, json!({"type":"Undo","expected_revision":1}))
            .await
            .unwrap();
        assert_eq!(r["body"]["revision"], 2, "{r}");
        fx.policy.revoke();
        let r = call(&mut c, wall(&fx, "c", 2, false)).await.unwrap();
        assert_eq!(code(&r), "forbidden");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bad_token_rejected() {
        let fx = start("badtok");
        let mut c = connect(&fx.name).await;
        let r = call(&mut c, json!({"type":"Hello","token":"00","protocol":1}))
            .await
            .unwrap();
        assert_eq!(code(&r), "bad_token");
        assert!(call(&mut c, json!({"type":"Summary"})).await.is_none());
        // primo frame diverso da Hello
        let mut c = connect(&fx.name).await;
        let r = call(&mut c, json!({"type":"Summary"})).await.unwrap();
        assert_eq!(code(&r), "bad_token");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn oversize_frame_closes_conn_server_alive() {
        let fx = start("oversize");
        let mut c = hello(&fx).await;
        let big = 2 * 1024 * 1024u32;
        c.write_all(&big.to_le_bytes()).await.unwrap();
        let body = read_frame(&mut c).await.unwrap().unwrap();
        let r: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(code(&r), "too_large");
        assert!(
            read_frame(&mut c)
                .await
                .map(|o| o.is_none())
                .unwrap_or(true)
        );
        let mut c2 = hello(&fx).await;
        let s = call(&mut c2, json!({"type":"Summary"})).await.unwrap();
        assert_eq!(s["type"], "Summary");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ac15_retry_over_bridge_no_duplicate() {
        let fx = start("ac15");
        fx.policy.grant();
        let mut c = hello(&fx).await;
        let r1 = call(&mut c, wall(&fx, "same", 0, false)).await.unwrap();
        let r2 = call(&mut c, wall(&fx, "same", 0, false)).await.unwrap();
        assert_eq!(r1, r2);
        let s = call(&mut c, json!({"type":"Summary"})).await.unwrap();
        assert_eq!(s["body"]["walls"], 5);
        assert_eq!(s["body"]["revision"], 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unsupported_protocol_lists_supported() {
        let fx = start("proto");
        let mut c = connect(&fx.name).await;
        let r = call(
            &mut c,
            json!({"type":"Hello","token":fx.token.as_str(),"protocol":99}),
        )
        .await
        .unwrap();
        assert_eq!(code(&r), "unsupported_protocol");
        assert!(
            r["body"]["message"].as_str().unwrap().contains("[1]"),
            "{r}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn silent_client_dropped_after_handshake_timeout() {
        let cfg = ServeConfig {
            handshake_timeout: Duration::from_millis(200),
            ..ServeConfig::default()
        };
        let fx = start_with("silent", cfg);
        let mut c = connect(&fx.name).await;
        let t0 = std::time::Instant::now();
        let r = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut c)).await;
        let closed = r
            .expect("il server deve chiudere")
            .map(|o| o.is_none())
            .unwrap_or(true);
        assert!(closed);
        assert!(t0.elapsed() < Duration::from_secs(3));
        let mut c2 = hello(&fx).await;
        assert_eq!(
            call(&mut c2, json!({"type":"Summary"})).await.unwrap()["type"],
            "Summary"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connection_cap_rejects_extra() {
        let cfg = ServeConfig {
            max_connections: 1,
            ..ServeConfig::default()
        };
        let fx = start_with("cap", cfg);
        let mut c1 = hello(&fx).await;
        let mut c2 = connect(&fx.name).await;
        let body = read_frame(&mut c2).await.unwrap().unwrap();
        let r: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(code(&r), "busy");
        assert_eq!(
            call(&mut c1, json!({"type":"Summary"})).await.unwrap()["type"],
            "Summary"
        );
        drop(c1);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut c3 = hello(&fx).await;
        assert_eq!(
            call(&mut c3, json!({"type":"Summary"})).await.unwrap()["type"],
            "Summary"
        );
    }
}
