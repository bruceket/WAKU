# Asset di terze parti incorporati (REQ-LIC-02)

Il binario desktop `waku` incorpora i font predefiniti di egui tramite il crate
`epaint_default_fonts` 0.37.0 (feature eframe `default_fonts`). Licenza SPDX
dichiarata dal crate: `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`.

| Font | File | Licenza | Note |
|---|---|---|---|
| Ubuntu Light | `Ubuntu-Light.ttf` | Ubuntu Font Licence 1.0 (`Ubuntu-font-1.0`) | testo proporzionale |
| Hack Regular | `Hack-Regular.ttf` | MIT (Hack); derivato da DejaVu (pubblico dominio) e Bitstream Vera (Bitstream Vera License) | testo monospazio |
| Noto Emoji Regular | `NotoEmoji-Regular.ttf` | SIL Open Font License 1.1 (`OFL-1.1`) | emoji |
| emoji-icon-font | `emoji-icon-font.ttf` | MIT | icone |
| egui-icons | `egui-icons.ttf` | MIT (sottoinsieme di emoji-icon-font) | `egui::special_emojis` |

Eccezioni in `deny.toml` (`[[licenses.exceptions]]`, crate `epaint_default_fonts`):
`OFL-1.1`, `Ubuntu-font-1.0`. Motivo: asset font, non codice; entrambe le licenze
consentono la redistribuzione incorporata in software. I testi integrali delle
licenze sono distribuiti nel crate (`fonts/OFL.txt`, `fonts/UFL.txt`,
`fonts/Hack-Regular.txt`, `fonts/emoji-icon-font-mit-license.txt`) e vanno inclusi
nel pacchetto di distribuzione.

Logo Waku (`assets/logo-light.svg`, `assets/logo-dark.svg`): originale, Apache-2.0,
vedi `assets/PROVENANCE.md`.
