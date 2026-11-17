# Dipendenze native ammesse

Audit limitato ai target distribuiti: `cargo xtask audit` esamina solo il grafo risolto per
`x86_64-pc-windows-msvc` (`cargo metadata --filter-platform`, costante `SHIPPED_TARGETS` in
`xtask/src/audit.rs`). Linux e macOS si aggiungono in R5. Crate nativi presenti solo per altri
target (es. `iana-time-zone-haiku`) non sono considerati.

Elenco dei crate con `build.rs` che dipendono da `cc`/`cmake`/`bindgen`, approvati esplicitamente.
Formato: `- <crate>: <motivo>`.

Oltre ai build nativi, l'audit segnala ogni crate con campo `links` o con nome che termina in
`-sys` non presente in questo elenco.

- windows-sys: binding Rust puri (generati da metadata Win32) alle DLL di sistema Windows; nessun `build.rs` e nessuna compilazione C/C++, linka solo librerie fornite dal sistema operativo tramite `windows-targets`.
- renderdoc-sys: definizioni FFI Rust pure dell'API RenderDoc (dipendenza di `wgpu-hal`); nessun `build.rs`, nessun `links`, nessuna dipendenza; la libreria RenderDoc non viene linkata ma eventualmente caricata a runtime solo se presente.
