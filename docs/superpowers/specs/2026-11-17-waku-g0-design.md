# Waku — Design G0 (prova tecnica)

Data: 2026-11-17 · Riferimento: PDR v2.2 (`Waku_PDR_v2_2_Rust.md`, §13, §14 G0, AC-16)

## Intento

Base reale del prodotto, non demo. G0 prova che lo stack Rust nativo regge il percorso
modifica → undo → export → MCP con qualità da prodotto: test veri, audit dipendenze dal primo
commit, confini dei crate conformi a §13. Esito G0 decide l'avvio di R1.

## Obiettivi

1. Workspace Cargo con i soli crate necessari a G0 (approccio A).
2. Modello minimo: progetto, un livello, muri rettilinei verticali, una apertura ospitata (porta), vano chiuso riconosciuto.
3. Command Service a singolo writer: comandi tipizzati, revisione attesa, conflitto esplicito, undo/redo, journal con origine (GUI/CLI/AI), request_id idempotente.
4. Parser espressioni dimensionali (REQ-B1.01): `332-60`, `550/2`, `2.5 m`, divisione per zero → errore.
5. Geometria: profilo muro con apertura → estrusione → mesh triangolata (cache derivata).
6. Export IFC4 ADD2 TC1 sottoinsieme: IfcProject/Site/Building/BuildingStorey, IfcWall, IfcOpeningElement + IfcRelVoidsElement, IfcDoor + IfcRelFillsElement, IfcSpace; GUID stabili.
7. Persistenza `.waku` (zip + manifest JSON versionato), salvataggio atomico.
8. Desktop eframe + wgpu: vista 3D del modello, vista 2D pianta, strumento muro, undo/redo, export.
9. CLI: crea modello di esempio, esporta IFC, salva/apre `.waku`.
10. MCP (rmcp 3.x, stdio, ponte verso desktop): risorsa `waku://model/summary`, tool `bim_envelope_command`, `bim_undo`, `bim_redo`; profilo default sola lettura, scrittura solo con policy locale (AC-18).
11. Qualità: `cargo fmt`, `clippy -D warnings`, `cargo test`, `cargo-deny` con allow-list esplicita (MIT/Apache-2.0/BSD/ISC/Unicode/Zlib); controllo AC-16 (nessuna dipendenza C/C++ o runtime JS/Python).

## Non obiettivi (G0)

Fasi/comparativa, finiture, controsoffitti, solver arredi, checker/regole, IDS, BCF, PDF/DXF/XLSX,
import IFC, Streamable HTTP, multi-livello, muri curvi. Restano nel perimetro R1+.

## Anti-obiettivi

- Crate stub vuoti "per completezza".
- Mutazioni che bypassano il Command Service (anche dalla GUI).
- Mesh usata come fonte di verità o per l'export.
- Dichiarare conformità IFC/MVD non provata.
- Dipendenze C/C++ nascoste (es. feature di compressione, font shaping nativo).

## Vincoli

Solo Rust (§1.1). `f64` mm, Z verticale; GPU `f32` su origine locale. Apache-2.0. Windows x64 primo target.
Nessuna proprietà intellettuale di terzi copiata (REQ-LIC-04).

## Approcci considerati

- **A. Fetta verticale con soli crate G0 — scelto.** Test reali su ogni pezzo, confini §13 rispettati.
- B. Workspace §13 completo con stub — scartato: codice morto, confini prematuri.
- C. Fork IPedrax/BIMCraft — scartato: licenza/revisione non verificate, PDR richiede originale.

## Struttura

```
apps/desktop   eframe + egui-wgpu
apps/cli       clap
apps/mcp       rmcp stdio
crates/domain       ID (u64 interni + GUID IFC base64 22 char), Length(mm f64), Project/Level/Wall/Opening/Space, invarianti
crates/commands     Command enum, CommandService (singolo writer), Revision, journal, undo/redo, request_id
crates/geometry     espressioni dimensionali, profili 2D, estrusione, tessellazione, rilevamento vani (grafo muri)
crates/render       pipeline wgpu, shader WGSL, camera, picking per ID dominio
crates/ifc          writer STEP IFC4 ADD2 TC1 sottoinsieme
crates/persistence  formato .waku (zip, manifest), salvataggio atomico (tmp + rename)
crates/mcp          risorse e tool sopra CommandService
```

Dipendenze tra crate: domain ← commands ← (ifc, persistence, mcp, apps); geometry ← domain; render ← geometry.
Nessun ciclo. Domain senza GUI.

## Decisioni prese da me (correggibili)

- `nalgebra` per algebra; `serde`/`serde_json`; `zip` con sola feature `deflate` pura Rust (`miniz_oxide`).
- Undo: snapshot del modello per transazione (semplice, corretto; ottimizzabile in R1 con delta).
- Vani in G0: ciclo chiuso di muri sull'asse; diagnostica per contorni aperti.
- Writer IFC scritto a mano (nessuna crate IFC esterna), validazione esterna con IfcOpenShell solo in CI/manuale, non distribuito.
- Concorrenza: CommandService dietro canale (`std::sync::mpsc`) con thread writer; snapshot `Arc<Model>` immutabili.
- Se rmcp 3.x non è su crates.io o tira dipendenze non ammesse: stop e segnalo.

## Architettura processi e MCP (decisione utente: ponte verso GUI)

- `waku-mcp` (stdio, avviato dal client AI) è un ponte: si collega alla sessione desktop attiva via IPC locale. Il modello vive solo nel processo desktop (unico writer, §10).
- IPC: named pipe Windows (`tokio::net::windows::named_pipe`), nome per utente (`\\.\pipe\waku-<user-sid-hash>`), messaggi JSON con lunghezza prefissata. Unix socket rimandato a R5.
- Autenticazione: la GUI genera token casuale per sessione di autorizzazione, scritto in file utente con ACL ristretta; il ponte lo legge e lo presenta. Pipe non raggiungibile da rete.
- Policy (AC-18): default sola lettura e proposta. La GUI mostra pannello "Accesso AI" con concessione scrittura per sessione (revocabile). Scrittura senza concessione → errore esplicito.
- Desktop non attivo: `waku-mcp` risponde con errore chiaro ("nessuna sessione Waku aperta"); nessuna modalità headless in G0.
- CLI: opera headless su file `.waku` con file di lock; se il file è aperto nel desktop, rifiuta la scrittura.

## Decisioni da builder check (mie, correggibili)

- Protocollo MCP: target 2026-07-28; se il client reale parla solo 2025-11-25, compatibilità aggiunta in G0 e documentata.
- Journal e request_id: in memoria nel processo desktop, non persistiti in `.waku` per G0. Retry dello stesso request_id restituisce l'esito già registrato.
- AC-16: vietato codice applicativo C/C++ e runtime JS/Python/Rhai. Ammessi librerie di sistema/driver (windows-sys, DX12/Vulkan). Ogni crate con `build.rs` che compila codice nativo: elencato in `docs/native-deps.md` e approvato esplicitamente; script di audit fallisce su voci non elencate.
- Numero senza unità: usa unità di progetto (default mm). API parser: `parse_length(expr, default_unit)`. Test AC-01 con progetto in cm.

## Criteri di uscita G0

- Test: AC-01 (332-60 cm → 272 cm), AC-02 (muro con porta spostato, undo ripristina), AC-15 (retry MCP senza duplicato), AC-16 (audit), AC-18 (scrittura MCP senza policy rifiutata).
- IFC esportato apribile in un viewer indipendente e valido per IfcOpenShell (verifica manuale documentata).
- Stesse modifiche via GUI, CLI e MCP producono lo stesso modello (test di integrazione).
- MCP provato con un client reale (Claude Code/Desktop via stdio).

## Lasciato al builder

Layout UI interno, nomi di funzioni, organizzazione test, scelta shader, dettagli camera.
