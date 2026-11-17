# Brand Waku: palette e accessibilità (REQ-BRAND-02)

Asset ufficiali in `assets/` (vedi `assets/PROVENANCE.md`).

| Ruolo | Chiaro | Scuro |
|---|---|---|
| Primario (simbolo, strumento corrente, evidenziazione 3D) | `#BE5532` | `#E0754D` |
| Primario per superfici con testo (pulsante attivo, selezione, link) | `#B95331` | `#E0754D` (testo `#1C1714`) |
| Sfondo | `#FAF7F2` | `#1C1714` |
| Testo | `#302720` | `#F3EBE3` |

Colori di stato (errore `#DC2626` ecc.) restano distinti dal primario.

## Contrasti WCAG 2.1 (`theme::contrast_ratio`, verificati dai test)

| Coppia | Rapporto | Soglia |
|---|---|---|
| `#302720` su `#FAF7F2` | 13,68 | 4,5 |
| `#F3EBE3` su `#1C1714` | 15,06 | 4,5 |
| `#FAF7F2` su `#BE5532` | **4,33 (non AA)** | 4,5 |
| `#FAF7F2` su `#B95331` (correzione adottata) | 4,52 | 4,5 |
| `#B95331` su `#FAF7F2` (link) | 4,52 | 4,5 |
| `#E0754D` su `#1C1714` | 5,77 | 3,0 |
| `#1C1714` su `#E0754D` (selezione scura) | 5,77 | 4,5 |

Il primario ufficiale `#BE5532` con etichetta chiara non raggiunge AA: per i
pulsanti attivi e la selezione si usa `#B95331`, la tonalità più vicina
conforme. Scelta da confermare dall'owner.

## Divergenza dal PDR

Il PDR §1.3 indica una palette a 4 toni per le facce
(`#CC633D #AD492B #BE5532 #C65A34` / scuro `#E0754D #C25B3A #D2663F #D96C45`);
il logo ufficiale usa invece un unico quadrato `#BE5532` con facce `#FAF7F2`
a opacità diverse. L'aggiornamento del PDR è decisione dell'owner.
