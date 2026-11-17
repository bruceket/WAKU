# waku-mcp — server MCP stdio

`waku-mcp` (crate `apps/mcp`, logica in `crates/mcp/src/mcp_server.rs`) e' un server MCP su
stdio costruito con **rmcp `=3.10.0`** (feature `server`, `transport-io`). Fa da ponte verso il
desktop Waku in esecuzione tramite la named pipe del bridge (Task 10).

## Versioni di protocollo

| Versione     | Modalita'                    | Note                                              |
|--------------|------------------------------|---------------------------------------------------|
| `2026-07-28` | stateless (target G0)        | `server/discover` + `_meta["io.modelcontextprotocol/protocolVersion"]` per richiesta |
| `2025-11-25` | handshake `initialize`       | negoziata da rmcp per i client basati su `initialize` |
| `2025-06-18`, `2025-03-26`, `2024-11-05` | `initialize` | accettate (default rmcp `KNOWN_VERSIONS`), non testate |

- `server/discover` pubblicizza `supportedVersions: ["2024-11-05","2025-03-26","2025-06-18","2025-11-25","2026-07-28"]`.
- `initialize` con `2026-07-28` (versione senza handshake) ricade su `2025-11-25`
  (`ProtocolVersion::LATEST_WITH_INITIALIZE`); con `2025-11-25` risponde `2025-11-25`.
- Versione non supportata (AC-20):
  - richiesta stateless con `_meta` `1999-01-01` → errore JSON-RPC `-32022 "Unsupported protocol version"` con
    `data: {requested, supported: [tutte le versioni sopra]}` (comportamento nativo rmcp);
  - `initialize` con `1999-01-01` → negoziazione a `2025-11-25` (ammesso dalla spec per `initialize`).
- Entrambe le modalita' sono coperte da `apps/mcp/tests/stdio_smoke.rs`.

## Superficie

Risorsa `waku://model/summary` (`application/json`): `{revision, project, levels, walls, openings, spaces}`.

Tool (schema JSON scritti a mano, `additionalProperties: false`):

- `bim_envelope_command` `{request_id: string, expected_revision: integer, actions: Action[], preview?: bool}`
  → `structuredContent` `{revision, created, preview}`. `actions` usa il JSON di `commands::Action`
  (discriminante `"op"`, es. `CreateWall`).
- `bim_undo` / `bim_redo` `{expected_revision: integer}` → `{revision, created, preview}`.

Le scritture passano dal bridge, che le registra nel journal con origine `Ai` e le ammette solo se
l'utente ha concesso l'accesso AI nel desktop. La concessione vale per la *generazione* del bridge
(una per writer, cioe' per progetto aperto nel desktop; aprire o creare un progetto ne avvia una
nuova), non per la singola connessione: tutti i client
connessi a quella generazione la ereditano, e la revoca vale per tutti. Un `request_id` gia'
committato e riusato con azioni diverse e' rifiutato con codice `invalid`.

## Errori

- Errore del bridge → risultato tool con `isError: true` e testo `"{code}: {message}"`
  (`forbidden`, `conflict`, `invalid`, `bad_token`, `busy`, ...). Argomenti non validi o tool
  sconosciuto → `invalid: ...`.
- Desktop assente (pipe inesistente o file token mancante) → `no_session: Nessuna sessione Waku
  aperta. Avvia Waku e concedi l'accesso AI.` Per `resources/read` e' un errore JSON-RPC con lo
  stesso messaggio e `data.code = "no_session"`.

Ogni chiamata apre una connessione nuova al bridge (Hello con token riletto da
`%LOCALAPPDATA%\Waku\session.token`), invia una richiesta e chiude: il server sopravvive ai
riavvii del desktop.

## Piattaforme

In G0 il bridge esiste solo su Windows: su Linux/macOS `BridgeClient::connect` restituisce sempre
`no_session` fino a R5.

## Uso con Claude Code

Build: `cargo build --release -p mcp-app`. Registrazione (da eseguire a cura dell'utente):

```
claude mcp add waku -- C:\Users\<utente>\Documents\Progetti\WAKU\target\release\waku-mcp.exe
```

Verifica manuale eseguita (Task 11, Step 5): smoke test stdio automatico sul binario
(`tools/list`, `resources/read`, `tools/call` senza desktop → `no_session`) in entrambe le
modalita'. La prova con Claude Code reale non e' stata eseguita (configurazione utente non
modificata); la versione negoziata attesa per un client `initialize` e' `2025-11-25`.
