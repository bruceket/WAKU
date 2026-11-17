# Supporto IFC (G0)

Export in **IFC4 (ADD2 TC1)**, formato STEP ISO 10303-21, scritto a mano nel
crate `crates/ifc` a partire dallo schema ufficiale buildingSMART. Nessun import.

API: `ifc::export_ifc4(&Model, &mut impl Write)` e
`ifc::export_ifc4_with(.., &ExportOptions { timestamp, file_name })`.

## Entita' scritte

| Area | Entita' |
| --- | --- |
| Header | `FILE_DESCRIPTION(('ViewDefinition [NotAssigned]'),'2;1')`, `FILE_NAME`, `FILE_SCHEMA(('IFC4'))` |
| Contesti | `IfcGeometricRepresentationContext` 'Model' 3D (precisione 0.01 mm), `IfcGeometricRepresentationSubContext` 'Body' `MODEL_VIEW` |
| Unita' | `IfcUnitAssignment`: `IfcSIUnit` LENGTHUNIT MILLI METRE, AREAUNIT SQUARE_METRE, VOLUMEUNIT CUBIC_METRE, PLANEANGLEUNIT RADIAN |
| Struttura | `IfcProject`, `IfcSite`, `IfcBuilding`, `IfcBuildingStorey` (uno per livello), `IfcRelAggregates` |
| Muri | `IfcWall` `.STANDARD.`; 'Body' 'SweptSolid': `IfcExtrudedAreaSolid` di `IfcRectangleProfileDef` (lunghezza x spessore) posizionato al centro dell'impronta, orientato lungo l'asse |
| Aperture | `IfcOpeningElement` `.OPENING.` relativo al muro, solido estruso attraverso lo spessore (+10 mm per lato); `IfcRelVoidsElement` |
| Porte | `IfcDoor` `.DOOR.` relativa all'apertura, `OverallWidth`/`OverallHeight`, senza rappresentazione; `IfcRelFillsElement` |
| Vani | `IfcSpace` `.ELEMENT.` `.INTERNAL.`, 'Body' 'SweptSolid' di `IfcArbitraryClosedProfileDef` (`IfcPolyline` del poligono netto) estruso all'altezza minima dei muri di contorno; aggregato al piano |
| Contenimento | `IfcRelContainedInSpatialStructure` (muri e porte) per piano |

GUID: quelli del dominio per progetto, sito, edificio, livelli, muri, porte
(`Opening.guid` -> `IfcDoor`) e vani. `IfcOpeningElement` e relazioni hanno GUID
derivati in modo stabile da un hash a 128 bit implementato nel crate (doppio
FNV-1a 64) di una chiave fissa: tipo + GUID della porta per il foro; tipo +
GUID relating + GUID related per `IfcRelVoidsElement`/`IfcRelFillsElement`;
tipo + GUID relating per `IfcRelAggregates` e
`IfcRelContainedInSpatialStructure` (una per contenitore). Aggiungere o
rimuovere altri elementi non cambia questi GUID; in caso di collisione si
aggiunge un sale. `OwnerHistory` omessa (opzionale in IFC4).

Nel golden i GUID del modello sono valori di test deterministici
(`IfcGuid::from_u128` progressivo), non GUID reali.

Determinismo: `#id` in ordine fisso (contesti, unita', struttura, livelli,
muri, aperture, vani per `ElementId` crescente, relazioni); reali con 6
decimali senza zeri finali (`4000.`, `0.5`); stringhe con apostrofo raddoppiato
e non-ASCII come `\X2\..\X0\` / `\X4\..\X0\`. Golden:
`tests/corpus/g0/sample_room.ifc` (rigenerazione: `WAKU_BLESS=1 cargo test -p ifc`).

## Limiti

- Nessuna pulizia delle giunzioni: i solidi dei muri si sovrappongono negli angoli.
- Nessun materiale, layer set, Pset, quantita' o stile di presentazione.
- Nessuna rappresentazione 'Axis' dei muri.
- `IfcDoor` senza Representation propria in G0: in un viewer e' visibile solo
  il vano (void) nel muro.
- Vani: poligono netto calcolato sempre dall'asse dei muri (ignora `WallLocation`).
- Elementi non esportabili (livello mancante, geometria degenere, ospite saltato)
  sono omessi e riportati in `ExportReport.skipped`.
- Nessun import IFC.
- **Nessuna dichiarazione di conformita' a un MVD** (Reference View, Design
  Transfer View o altri): il file e' IFC4 valido per schema nelle intenzioni, ma
  non certificato.

## Validazione esterna

2026-11-17 - `ifcopenshell.validate` sul golden: **non eseguita: IfcOpenShell non
installato** (Python non presente sulla macchina di sviluppo; nessuna
installazione globale effettuata). Da eseguire in un job CI separato e non
bloccante (`ifc-validate`, ubuntu, `pip install ifcopenshell`).
