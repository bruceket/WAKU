# Brand Waku: palette e accessibilità (REQ-BRAND-02)

Asset ufficiali in `assets/` (vedi `assets/PROVENANCE.md`). Implementazione:
`apps/desktop/src/theme.rs` (`visuals(dark)`, `status_colors(dark)`, `active_fill(dark)`).

| Ruolo | Chiaro | Scuro |
|---|---|---|
| Primario del marchio (simbolo, evidenziazione 3D) | `#BE5532` | `#E0754D` |
| Token attivo `active_fill`: pulsante attivo, selezione, strumento corrente (toolbar e anteprima in pianta), link | `#B95331` | `#E0754D` |
| Etichetta su token attivo | `#FAF7F2` | `#1C1714` |
| Sfondo | `#FAF7F2` | `#1C1714` |
| Testo | `#302720` | `#F3EBE3` |
| Errore | `#BE123C` | `#FB7185` |
| Avviso | `#7C5E00` | `#FBBF24` |
| OK | `#15803D` | `#4ADE80` |

Nessun `override_text_color`: in egui 0.37 prevarrebbe sui colori degli stati
attivi e della selezione. I colori del testo sono impostati per stato.

## Contrasti effettivi (coppie renderizzate, verificate dai test)

| Coppia | Chiaro | Scuro | Soglia |
|---|---|---|---|
| Testo su pannello | 13,68 | 15,06 | 4,5 |
| Testo su widget inattivo | 11,71 | 10,87 | 4,5 |
| Testo su widget hover | 10,66 | 8,79 | 4,5 |
| Etichetta su pulsante attivo | 4,52 | 5,77 | 4,5 |
| Testo selezionato su selezione | 4,52 | 5,77 | 4,5 |
| Link su pannello | 4,52 | 5,77 | 4,5 |
| Errore su sfondo | 5,88 | 6,60 | 4,5 |
| Avviso su sfondo | 5,68 | 10,64 | 4,5 |
| OK su sfondo | 4,69 | 10,20 | 4,5 |
| `#FAF7F2` su `#BE5532` (primario puro) | **4,33, non AA** | — | 4,5 |

Il primario ufficiale `#BE5532` con un'etichetta chiara non raggiunge il livello AA: dove c'è
testo uso `#B95331`, la tonalità conforme più vicina. Scelta da confermare
dall'owner. I colori di stato distano almeno 25° di tinta dal token attivo.

## Divergenza dal PDR

Il PDR §1.3 indica una palette a 4 toni per le facce
(`#CC633D #AD492B #BE5532 #C65A34` / scuro `#E0754D #C25B3A #D2663F #D96C45`);
il logo ufficiale usa invece un unico quadrato `#BE5532` con facce `#FAF7F2`
a opacità diverse. Decidere se aggiornare il PDR spetta all'owner.
