# Provenienza asset

| File | Origine | Licenza |
|---|---|---|
| `logo-light.svg` | fornito dall'utente (owner del progetto), 2026-11-17; copia byte per byte, contiene un manifest C2PA (`<metadata><c2pa:manifest>`) | da confermare dall'owner |
| `app-icon.svg` | fornito dall'utente (owner del progetto), 2026-11-17; copia byte per byte, contiene un manifest C2PA | da confermare dall'owner |
| `logo-dark.svg` | derivato dal logo chiaro, proposta da confermare: stesso simbolo, wordmark `#E0754D`, sfondo previsto `#1C1714`; senza blocco metadata | da confermare dall'owner |

I file forniti non vanno modificati (il manifest C2PA ne verifica l'integrità);
`.gitattributes` li marca `-text` per evitare conversioni di fine riga.

Geometria: quadrato arrotondato `#BE5532` con volume a 4 facce `#FAF7F2`
(opacità 1 / 0,55 / 0,8 / 0,92) e wordmark "waku" a tratti arrotondati.
Sotto 24 px si usa il solo simbolo (AC-24). L'app ridisegna logo e icona
finestra dalla stessa geometria (`apps/desktop/src/theme.rs`), senza
rasterizzatore SVG. Palette e contrasti: `docs/brand.md`.
