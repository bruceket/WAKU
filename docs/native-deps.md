# Dipendenze native ammesse

Audit limitato ai target distribuiti: `cargo xtask audit` esamina solo il grafo risolto per
`x86_64-pc-windows-msvc` (`cargo metadata --filter-platform`, costante `SHIPPED_TARGETS` in
`xtask/src/audit.rs`). Linux e macOS si aggiungono in R5. Crate nativi presenti solo per altri
target (es. `iana-time-zone-haiku`) non sono considerati.

Elenco dei crate con `build.rs` che dipendono da `cc`/`cmake`/`bindgen`, approvati esplicitamente.
Formato: `- <crate>: <motivo>`.

