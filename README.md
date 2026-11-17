# WAKU

<p align="center">
  <img src="assets/logo-light.svg#gh-light-mode-only" alt="WAKU Logo" width="160" />
  <img src="assets/logo-dark.svg#gh-dark-mode-only" alt="WAKU Logo" width="160" />
</p>

<p align="center">
  <strong>Native, deterministic, AI-first BIM engine & modeler written in 100% pure safe Rust.</strong>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License: Apache 2.0" /></a>
  <a href="rust-toolchain.toml"><img src="https://img.shields.io/badge/rust-2024%20edition-orange.svg" alt="Rust Edition 2024" /></a>
  <a href="docs/g0-report.md"><img src="https://img.shields.io/badge/status-G0%20passed-brightgreen.svg" alt="Status G0" /></a>
  <a href="docs/native-deps.md"><img src="https://img.shields.io/badge/C%2FC%2B%2B%20deps-0%20(pure%20Rust)-blueviolet.svg" alt="Zero C/C++ Dependencies" /></a>
  <a href="docs/mcp.md"><img src="https://img.shields.io/badge/protocol-MCP%20stdio-informational.svg" alt="MCP Stdio" /></a>
</p>

---

## 🏛️ Cos'è WAKU

**WAKU** (dal concetto giapponese di *telaio*, *cornice*, *spazio delimitato*) è un motore CAD/BIM di nuova generazione concepito da zero per essere:

1. **Puramente nativo e sicuro**: 100% Rust (edizione 2024), privo di runtime di scripting esterni (no Python, no Node/JS, no Rhai) e privo di codice applicativo C/C++ nascosto.
2. **Deterministico e auditabile**: Architettura a transazioni con singolo writer, journal immutabile degli eventi con tracciamento dell'origine (`GUI`, `CLI`, `AI`), revisioni monotoniche e operazioni idempotenti basate su `request_id`.
3. **AI-First con controllo utente (Human-in-the-Loop)**: Integrazione diretta del protocollo **Model Context Protocol (MCP)** via stdio e named pipe locale, con rigorosa sandbox dei permessi (nessuna scrittura AI consentita senza autorizzazione esplicita dell'utente nella GUI - AC-18).
4. **Interoperabile per standard aperti**: Esportatore nativo **IFC4 (ADD2 TC1)** scritto a mano in STEP ISO 10303-21, con generazione deterministica e stabile dei GUID globali.

---

## ✨ Funzionalità Chiave

### 🏗️ Modellazione e Dominio
- **Elementi di involucro architettonico**: Muri rettilinei verticali con spessore e altezza parametrica, aperture ospitate (porte) con vincoli al muro ospite.
- **Riconoscimento automatico dei vani**: Analisi topologica del grafo degli assi murari con individuazione automatica dei cicli chiusi (`DetectSpaces`) e calcolo del poligono netto (`IfcSpace`).
- **Valutatore di espressioni dimensionali (REQ-B1.01)**: Input flessibile per quote e distanze con supporto ad aritmetica mista e conversioni di unità (`332-60 cm`, `550/2`, `2.5 m`), validazione rigorosa e prevenzione delle divisioni per zero.

### 🔄 Command Service & Undo/Redo
- Tutte le mutazioni del modello transitano obbligatoriamente da un unico servizio centralizzato (`CommandService`).
- **Undo/Redo completo** a snapshot con conservazione della cronologia.
- Rilevamento automatico dei conflitti di concorrenza (`expected_revision`).
- Deduplicazione e idempotenza delle richieste tramite `request_id` (AC-15).

### 🖥️ Interfaccia Desktop Reattiva & Grafica 3D
- Realizzata con **egui / eframe** e rendering accelerato via GPU con **wgpu** e shader custom WGSL.
- **Doppia vista sincronizzata**: Vista 2D interattiva a pianta e Viewport 3D con camera orbit/pan/zoom e picking per ID elemento.
- **Design system accessibile**: Palette colori conforme agli standard di contrasto **WCAG 2.1 AA** (tema chiaro e scuro automatico).

### 📦 Esportazione IFC4 e Persistenza Atomica
- **Export IFC4 STEP**: Scrittura diretta delle entità spaziali (`IfcProject`, `IfcSite`, `IfcBuilding`, `IfcBuildingStorey`), costruttive (`IfcWall`, `IfcDoor`, `IfcOpeningElement`) e spaziali interne (`IfcSpace`), con relazioni di vuoto (`IfcRelVoidsElement`) e riempimento (`IfcRelFillsElement`).
- **Formato file `.waku`**: Archivio compresso Zip con manifest JSON versionato.
- **Salvataggio atomico a prova di crash**: Scrittura su file temporaneo nella medesima directory, flush dei buffer, `sync_all` su disco e rename atomico. Lock esclusivo di sessione (`.waku.lock`) per evitare corruzioni da accessi concorrenti.

### 🤖 Bridge MCP (Model Context Protocol)
- Eseguibile bridge `waku-mcp` per client AI (es. Claude Code, Claude Desktop, Cursor).
- Espone risorse modello (`waku://model/summary`) e tool di manipolazione (`bim_envelope_command`, `bim_undo`, `bim_redo`).
- Collegamento IPC locale protetto (Windows Named Pipe) con token ad ACL ristretta generato per sessione.
- Policy di sicurezza nativa: blocco automatico dei comandi di mutazione in assenza di concessione esplicita rilasciata dall'utente dal pannello "Accesso AI" del desktop.

---

## 🏛️ Architettura del Workspace

Il repository è organizzato come un workspace Cargo modulare a responsabilità singola:

```text
WAKU/
├── apps/
│   ├── desktop/          # Applicazione grafica (eframe, egui-wgpu, viewport 2D/3D)
│   ├── cli/              # Tool a riga di comando headless (waku-cli) per automazioni e CI
│   └── mcp/              # Server stdio Model Context Protocol (waku-mcp)
│
├── crates/
│   ├── domain/           # Entità pure (Project, Wall, Opening, Space, Length, GUID) e parser espressioni
│   ├── commands/         # CommandService, Command enum, Journal, Revision, Undo/Redo
│   ├── geometry/         # Estrusione profili 2D, triangolazione mesh e rilevamento topologico vani
│   ├── render/           # Pipeline wgpu, shader WGSL, gestione camera e picking
│   ├── ifc/              # Scrittore IFC4 STEP deterministico conforme a ISO 10303-21
│   ├── persistence/      # Formato .waku, salvataggio atomico e locking di progetto
│   └── mcp/              # Logica server MCP e client/server IPC bridge (Named Pipes)
│
├── tests/
│   └── equivalence/      # Test di equivalenza modello tra GUI, CLI e bridge MCP
├── xtask/                # Task runner interno per audit di licenze e dipendenze native
├── docs/                 # Specifiche tecniche, report e documentazione di design
└── assets/               # Loghi ufficiali SVG, palette e icone
```

---

## 🚀 Prerequisiti e Avvio Rapido

### Prerequisiti
- **Rust Toolchain**: Versione stabile `1.100+` (Edition 2024).
- Piattaforma di riferimento corrente per il bridge IPC: **Windows 11 / 10 x64** (supporto IPC Unix domain socket previsto in milestone successiva).

### Compilazione dell'intero workspace
```bash
git clone https://github.com/bruceket/WAKU.git
cd WAKU
cargo build --release
```

I binari compilati saranno posizionati in `target/release/`:
- `waku.exe`: Applicazione Desktop GUI
- `waku-cli.exe`: Strumento a riga di comando
- `waku-mcp.exe`: Server MCP per agenti AI

---

## 📖 Utilizzo

### 1. Applicazione Desktop
Avvia l'interfaccia interattiva:
```bash
cargo run --release -p desktop
# oppure esegui direttamente target/release/waku.exe
```
Dall'interfaccia puoi:
- Disegnare muri rettilinei ed inserire porte.
- Rilevare automaticamente i vani chiusi tramite il pulsante **Rileva Vani**.
- Esportare il modello in formato standard `.ifc` o salvare il file di progetto `.waku`.
- Gestire le autorizzazioni AI dal pannello dedicato in alto a destra.

---

### 2. CLI Headless (`waku-cli`)
Pensata per pipeline di automazione, script di integrazione e server CI:

```bash
# Crea un nuovo progetto vuoto impostando i centimetri come unità base
waku-cli new edificio.waku --unit cm

# Crea il progetto di test standard (stanza 4x3 m con porta e vano)
waku-cli sample test_room.waku --force

# Visualizza il riassunto informativo del modello in formato JSON
waku-cli info test_room.waku

# Applica una transazione di azioni da un file JSON (Vec<Action>)
waku-cli apply test_room.waku actions.json

# Esporta il file .waku in uno standard IFC4 STEP
waku-cli export-ifc test_room.waku out_model.ifc
```

---

### 3. Configurazione MCP con Claude / Client AI
Configura `waku-mcp` per consentire agli agenti AI (es. Claude Code) di ispezionare e modificare il progetto attivo nella GUI:

#### Con Claude CLI:
```bash
claude mcp add waku -- C:\percorso\verso\WAKU\target\release\waku-mcp.exe
```

#### Tramite file di configurazione (`claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "waku": {
      "command": "C:\\percorso\\verso\\WAKU\\target\\release\\waku-mcp.exe",
      "args": []
    }
  }
}
```

> **Nota di Sicurezza**: Quando l'AI invia un comando di modifica (`bim_envelope_command`), l'operazione verrà bloccata con codice `forbidden` a meno che l'utente non abbia attivato esplicitamente la concessione nel pannello "Accesso AI" dell'applicazione desktop attiva.

---

## 🧪 Qualità del Codice e Verifica

Il progetto rispetta rigidi standard di ingegneria del software e continuous integration:

```bash
# Verifica formattazione del codice
cargo fmt --all --check

# Linter con divieto assoluto di warning
cargo clippy --workspace --all-targets -- -D warnings

# Suite completa di test unitari, di integrazione ed equivalenza
cargo test --workspace

# Verifica assenza di runtime esterni o codice nativo non approvato (AC-16)
cargo xtask audit
```

### Test di Equivalenza Canali
Il test di integrazione `tests/equivalence/tests/same_actions.rs` garantisce che le stesse sequenze di azioni applicate via **GUI**, via **CLI** (`waku-cli apply`) o tramite **Bridge MCP** producano modelli di dominio e file IFC **identici ed equivalenti**.

---

## 🗺️ Roadmap & Fasi di Sviluppo

- [x] **Milestone G0 (Gate Zero - Completata)**:
  - Fetta verticale completa: GUI, CLI, MCP Bridge.
  - Dominio geometrico di base (Muri, Porte, Vani).
  - Parser dimensionale conforme (`332-60 cm`).
  - Esportatore conforme IFC4 ADD2 TC1 (STEP ISO 10303-21).
  - Persistenza atomica `.waku`.
  - Zero dipendenze C/C++ applicative o interpreti terzi.
- [ ] **Milestone R1 (In Sviluppo)**:
  - Gestione multi-piano e scale.
  - Pulizia automatica delle giunzioni d'angolo dei muri (L, T, X).
  - Tipologie avanzate di finestre e aperture.
  - Importatore IFC4.
  - Socket di dominio Unix per supporto nativo Linux / macOS del bridge MCP.
- [ ] **Milestone R2+**:
  - Vincoli parametrici avanzati, computo metrico e finiture.
  - Esportazione documentale (PDF/DXF) e integrazione con standard BCF/IDS.

---

## 📄 Licenza

Distribuito sotto licenza **Apache License, Version 2.0**.  
Consulta il file [LICENSE](LICENSE) e il file [NOTICE](NOTICE) per i dettagli completi.
