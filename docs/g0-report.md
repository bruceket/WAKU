# Report G0

Data: 2026-11-17. Branch: `feat/g0`. Esiti verificati eseguendo i comandi indicati su una
singola macchina Windows 11 (vedi "Macchina e tempi"). Dove una verifica non e' stata eseguita
e' dichiarato esplicitamente nelle sezioni "Punti aperti".

## Esiti dei criteri di accettazione

| AC | Criterio (sintesi) | Esito | Test o comando che lo dimostra |
|---|---|---|---|
| AC-01 | Espressioni dimensionali con unita' di progetto (332-60 cm = 272 cm) | Superato | `expr::tests::ac01_subtraction_cm` (crate `domain`); `app::tests::length_field_ac01` (desktop, campo lunghezza) |
| AC-02 | Muro con porta spostato; undo ripristina | Superato | `service::tests::ac02_move_wall_with_door_and_undo` (crate `commands`) |
| AC-15 | Retry con stesso `request_id` senza duplicati | Superato | `service::tests::ac15_retry_same_request_id_no_duplicate` (`commands`); `bridge_server::tests::ac15_retry_over_bridge_no_duplicate` (`mcp`) |
| AC-16 | Nessun codice C/C++ applicativo ne' runtime JS/Python/Rhai; licenze ammesse | Superato | `cargo xtask audit` (`audit: ok`), test `audit::tests::*` in `xtask`; `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`) |
| AC-18 | Scrittura AI rifiutata senza concessione dell'utente | Superato | `bridge_server::tests::ac18_write_without_grant_forbidden` (`mcp`); `policy::tests::default_denied_grant_revoke` |
| AC-20 | Versione di protocollo non supportata gestita in modo esplicito | Superato | `stdio_stateless_unsupported_version_lists_supported`, `stdio_initialize_unsupported_version_negotiates_2025_11_25` (`apps/mcp/tests/stdio_smoke.rs`); `bridge_server::tests::unsupported_protocol_lists_supported` |
| AC-24 | Logo: solo simbolo sotto 24 px; variante per tema | Superato a livello di logica; resa grafica non verificata a vista (vedi punti aperti) | `app::tests::logo_variant_cases` (desktop) |

Equivalenza tra canali (richiesta dal piano di chiusura): `same_actions_same_model`
(`tests/equivalence/tests/same_actions.rs`) applica le stesse azioni (a) direttamente al
`CommandService` con origine Gui, (b) tramite `waku-cli apply`, (c) tramite bridge IPC e
`BridgeClient`, e verifica modelli e IFC equivalenti. Superato. Il confronto e' normalizzato:

- i GUID di dominio sono casuali per canale, quindi il modello viene confrontato senza guid e
  senza id (muri: estremi, spessore, altezza, posizione; aperture: indice del muro ospite,
  offset, larghezza, altezza, davanzale; vani: contorno come indici di muro, nome; unita' di
  progetto). Il confronto **esclude** livelli e nome del progetto;
- nell'IFC ogni GUID di 22 caratteri e' sostituito da un segnaposto numerato per prima
  comparsa; timestamp fisso (`WAKU_FIXED_TIMESTAMP` per la CLI, `ExportOptions` negli altri
  canali); il numero di entita' deve coincidere.

Note sul test:

- Il confronto avviene dopo ogni passo: dopo T1 (esattamente 1 vano con 4 muri di contorno, in
  tutti e tre i canali; CLI: modello ricaricato dopo il primo `apply`, bridge: snapshot del writer
  tra T1 e T2), dopo T2 e, nei canali (a) e (c), dopo Undo (deve coincidere con lo stato dopo T1,
  cioe' posizione originale del muro) e dopo Redo (deve coincidere con lo stato dopo T2).
- La CLI non ha un comando di undo/redo: per il canale CLI si applicano le due transazioni
  (file JSON separati) senza Undo/Redo; l'equivalenza dell'undo e' verificata solo tra (a) e (c).
- La transazione T2 e' un `MoveWall` di 500 mm in y del primo muro **senza** un successivo
  `DetectSpaces`: lo spostamento porta gli estremi sulla parte interna dei muri adiacenti
  (giunzione a T) e G0 rifiuta quella geometria con un errore esplicito
  (`CommandError::Geometry`). Il vano rilevato in T1 viene rimosso dal `MoveWall`, quindi il
  modello dopo T2 ha 4 muri, 1 porta e 0 vani (dopo T1: 1 vano). Questo e' un effetto del limite noto "giunzioni
  senza pulizia" (sotto), non un difetto del test.
- Il canale (c) gira solo su Windows (`cfg(windows)`), perche' il bridge e' a named pipe.

## Esecuzione del gate

Tutti i comandi eseguiti sulla macchina di sviluppo, branch `feat/g0`:

| Comando | Esito |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -- -D warnings` | ok, nessun warning |
| `cargo test --workspace` | ok: 137 test superati, 0 falliti, 1 ignorato (`render` `tests/gpu.rs`, richiede GPU) |
| `cargo deny check` | `advisories ok, bans ok, licenses ok, sources ok` |
| `cargo xtask audit` | `audit: ok` |
| `cargo build --release` | ok, `Finished release profile [optimized] target(s) in 3m 15s` |

Nota: `cargo` segnala un avviso non bloccante sul campo `workspace.package.rust-version`
(`unused_workspace_package_fields`); non e' stato modificato.

## Validazione IFC esterna

**Non eseguita.** IfcOpenShell non e' stato eseguito perche' sulla macchina non e' presente
Python e non e' stata fatta alcuna installazione globale. L'export IFC4 e' verificato solo da
test interni (struttura STEP, conteggio entita', golden `tests/corpus/g0/sample_room.ifc`,
GUID presenti). Non si dichiara quindi conformita' dello schema ne' a un MVD. Punto aperto:
eseguire `ifcopenshell.validate` sul golden, ad esempio in un job CI separato e non
bloccante (`ifc-validate`), e annotare l'esito in `docs/ifc-support.md`.

## Client MCP reale e versione di protocollo

**Prova con un client MCP reale non eseguita.** Sono eseguiti solo gli smoke test stdio
automatici sul binario (`apps/mcp/tests/stdio_smoke.rs`: `tools/list`, `resources/read`,
`tools/call` senza desktop, versioni non supportate) nelle modalita' stateless
(`2026-07-28`) e `initialize` (`2025-11-25`). La configurazione dell'utente non e' stata
modificata. Punto aperto per l'utente: registrare il server ed eseguire una sessione reale
con il desktop acceso e l'accesso AI concesso, con il comando indicato in `docs/mcp.md`:

```
claude mcp add waku -- C:\Users\<utente>\Documents\Progetti\WAKU\target\release\waku-mcp.exe
```

Le versioni di protocollo supportate sono elencate in `docs/mcp.md`; per un client basato su
`initialize` la versione negoziata attesa e' `2025-11-25`, ma questo non e' stato osservato
con un client reale.

## Verifica interattiva della GUI

**Non eseguita.** L'app desktop (`waku.exe`, eframe/wgpu) e' coperta da test unitari sulla
logica (strumento muro, campo lunghezza, logo, errori di comando) e compila in release, ma
nessuno ha aperto la finestra per provare disegno, vista 3D, Ctrl+Z / Ctrl+Y, menu File,
pannello "Accesso AI" e resa del logo. Punto aperto: sessione manuale su una macchina con GPU
e verifica anche dell'accessibilita' (WCAG 2.1 AA, contrasto e navigazione da tastiera), non
valutata in G0.

## Macchina e tempi

| Voce | Valore |
|---|---|
| CPU | 11th Gen Intel Core i7-1185G7 @ 3.00 GHz, 4 core / 8 thread |
| RAM | 15,7 GB |
| GPU | Intel Iris Xe Graphics (driver 32.0.101.7088) |
| Sistema operativo | Windows 11 Pro |
| Rust | rustc 1.100.0 (0ef1bc386 2026-11-09), cargo 1.100.0 |
| `cargo build --release` | 196 s (3 m 15 s) in una singola misura; la cache `target/release` poteva contenere dipendenze gia' compilate, quindi non e' una misura di build a freddo |
| Dimensione binari release | `waku.exe` 15,4 MB, `waku-mcp.exe` 3,2 MB, `waku-cli.exe` 1,6 MB |

## Limiti noti

- Nessuna pulizia delle giunzioni dei muri: i solidi si sovrappongono negli angoli e le
  giunzioni a T o le sovrapposizioni sono rifiutate dal rilevamento vani con errore esplicito.
- Undo/redo a snapshot (`Arc<Model>` per revisione): semplice e corretto, ma il costo di memoria
  cresce con il numero di revisioni e con la dimensione del modello.
- La revisione non e' persistita nel file `.waku`: a ogni apertura riparte da 0.
- Il file del token del bridge (`%LOCALAPPDATA%\Waku\session.token`) non riceve una DACL
  esplicita: si affida all'ACL per-utente ereditata dalla cartella.
- Nessun picking 3D: la selezione e il disegno avvengono sulla vista in pianta 2D.
- I testi delle licenze dei font incorporati (OFL, Ubuntu Font Licence, Hack, emoji-icon-font)
  vanno inclusi nel pacchetto di distribuzione (`docs/third-party-assets.md`).
- Il bridge esiste solo su Windows (named pipe); Linux e macOS sono rimandati a R5 e l'audit
  AC-16 copre solo il target `x86_64-pc-windows-msvc`.
- IFC: nessun materiale, Pset, quantita' o import; `IfcDoor` senza rappresentazione propria;
  nessuna dichiarazione di conformita' a un MVD (dettagli in `docs/ifc-support.md`).

## Eccezioni in `deny.toml`

- `[[licenses.exceptions]]` per il crate `epaint_default_fonts`: `OFL-1.1` e
  `Ubuntu-font-1.0`. Sono asset font incorporati nel binario, non codice; le due licenze
  consentono la redistribuzione incorporata.
- `[advisories] ignore` per tre avvisi "unmaintained" (non vulnerabilita') su crate del solo
  backend Linux di accesskit, non compilati per il target distribuito Windows:
  `RUSTSEC-2026-0370` (zvariant), `RUSTSEC-2026-0371` (zbus_names), `RUSTSEC-2026-0372`
  (zvariant_derive). Da rivalutare quando si aggiunge il supporto Linux.
- `multiple-versions = "warn"` e `wildcards = "allow"` sono impostazioni permissive, non
  eccezioni puntuali.

## Proposta per R1

Quanto segue e' una proposta per discussione, non un impegno su contenuti, date o livelli di
servizio; ogni voce richiede conferma prima di essere pianificata.

1. Chiudere i punti aperti di verifica prima di estendere il perimetro: validazione IFC con
   IfcOpenShell, sessione reale con un client MCP, prova manuale della GUI.
2. Valutare la pulizia delle giunzioni dei muri, perche' e' la causa dei rifiuti nel
   rilevamento vani dopo uno spostamento e della sovrapposizione dei solidi nell'IFC.
3. Valutare la persistenza della revisione e un modello di undo piu' leggero dello snapshot
   completo, se i modelli reali risultano abbastanza grandi da renderlo un problema (da
   misurare, non assunto).
4. Aggiungere un comando di undo/redo alla CLI, o dichiarare esplicitamente che non e'
   previsto, per allineare i canali.
5. Prima di qualunque distribuzione a clienti, in particolare in contesti PA, Healthcare o
   Banking: inclusione dei testi di licenza, rafforzamento della protezione del file token
   (DACL esplicita), verifica di accessibilita' WCAG 2.1 AA e revisione ISO 27001 del
   trattamento dei dati di progetto.
6. Estendere il bridge e l'audit AC-16 a Linux e macOS secondo il piano R5.
