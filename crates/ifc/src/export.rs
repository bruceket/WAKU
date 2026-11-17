//! Export del modello G0 in IFC4 (ADD2 TC1).
//!
//! Struttura spaziale: IfcProject > IfcSite > IfcBuilding > IfcBuildingStorey
//! (IfcRelAggregates). Muri e porte contenuti nel piano
//! (IfcRelContainedInSpatialStructure); vani aggregati al piano. Unita' SI con
//! lunghezze in millimetri. OwnerHistory omessa (opzionale in IFC4).
//!
//! Gli `#id` sono assegnati in ordine fisso (livelli, muri, aperture, vani per
//! `ElementId` crescente) e i reali sono formattati da [`crate::step::real`]:
//! l'output e' byte-deterministico a parita' di modello e timestamp.

use crate::step::{real, string};
use domain::{ElementId, IfcGuid, Model, Point2};
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use thiserror::Error;

/// Margine (mm) per lato con cui il solido dell'apertura eccede lo spessore.
const OPENING_MARGIN: f64 = 10.0;
/// Precisione del contesto geometrico, in unita' di lunghezza (mm).
const PRECISION: f64 = 0.01;

#[derive(Debug, Error)]
pub enum IfcError {
    #[error("errore di scrittura: {0}")]
    Io(#[from] std::io::Error),
}

/// Esito dell'export: numero di entita' scritte ed elementi saltati con motivo.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportReport {
    pub entities: usize,
    pub skipped: Vec<(ElementId, String)>,
}

/// Opzioni dell'header STEP.
#[derive(Clone, Debug)]
pub struct ExportOptions {
    /// `time_stamp` di FILE_NAME (ISO 8601).
    pub timestamp: String,
    /// `name` di FILE_NAME.
    pub file_name: String,
}

impl Default for ExportOptions {
    /// Timestamp corrente (UTC) e nome file `model.ifc`.
    fn default() -> Self {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        ExportOptions {
            timestamp: iso8601_utc(secs),
            file_name: "model.ifc".to_owned(),
        }
    }
}

/// Data/ora UTC `YYYY-MM-DDThh:mm:ss` da secondi Unix (calendario gregoriano).
fn iso8601_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Conversione giorni -> data civile (algoritmo "days from civil" inverso).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

/// Export IFC4 con timestamp corrente.
pub fn export_ifc4(model: &Model, w: &mut impl Write) -> Result<ExportReport, IfcError> {
    export_ifc4_with(model, w, &ExportOptions::default())
}

/// Export IFC4 con opzioni esplicite (timestamp fisso per output riproducibile).
pub fn export_ifc4_with(
    model: &Model,
    w: &mut impl Write,
    opts: &ExportOptions,
) -> Result<ExportReport, IfcError> {
    let mut b = Builder::new(model);
    let skipped = b.build(model);
    writeln!(w, "ISO-10303-21;")?;
    writeln!(w, "HEADER;")?;
    writeln!(
        w,
        "FILE_DESCRIPTION(('ViewDefinition [NotAssigned]'),'2;1');"
    )?;
    writeln!(
        w,
        "FILE_NAME({},{},(''),(''),'Waku','Waku','');",
        string(&opts.file_name),
        string(&opts.timestamp)
    )?;
    writeln!(w, "FILE_SCHEMA(('IFC4'));")?;
    writeln!(w, "ENDSEC;")?;
    writeln!(w, "DATA;")?;
    for (i, l) in b.lines.iter().enumerate() {
        writeln!(w, "#{}={};", i + 1, l)?;
    }
    writeln!(w, "ENDSEC;")?;
    writeln!(w, "END-ISO-10303-21;")?;
    Ok(ExportReport {
        entities: b.lines.len(),
        skipped,
    })
}

fn r(id: usize) -> String {
    format!("#{id}")
}

fn list(ids: &[usize]) -> String {
    let v: Vec<String> = ids.iter().map(|&i| r(i)).collect();
    format!("({})", v.join(","))
}

/// Entita' comuni riusate.
struct Shared {
    dir_x: usize,
    dir_y: usize,
    dir_z: usize,
    /// IfcAxis2Placement3D identita'.
    wcs: usize,
    /// IfcAxis2Placement2D identita'.
    wcs2d: usize,
    ctx: usize,
    body: usize,
}

/// Muro esportato: entita' e dati per le aperture ospitate.
struct WallOut {
    entity: usize,
    placement: usize,
    length: f64,
    thickness: f64,
}

struct Builder {
    lines: Vec<String>,
    /// GUID gia' usati (modello + derivati), per evitare collisioni.
    used: HashSet<String>,
    seed: u128,
    counter: u64,
}

/// Mescolamento splitmix64 per GUID derivati deterministici.
fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Builder {
    fn new(model: &Model) -> Self {
        let p = &model.project;
        let mut used: HashSet<String> = [&p.guid, &p.site_guid, &p.building_guid]
            .into_iter()
            .map(|g| g.as_str().to_owned())
            .collect();
        used.extend(model.levels.values().map(|x| x.guid.as_str().to_owned()));
        used.extend(model.walls.values().map(|x| x.guid.as_str().to_owned()));
        used.extend(model.openings.values().map(|x| x.guid.as_str().to_owned()));
        used.extend(model.spaces.values().map(|x| x.guid.as_str().to_owned()));
        Builder {
            lines: Vec::new(),
            used,
            seed: p.guid.to_u128(),
            counter: 0,
        }
    }

    fn add(&mut self, s: String) -> usize {
        self.lines.push(s);
        self.lines.len()
    }

    /// GUID deterministico per entita' senza GUID nel dominio (relazioni,
    /// IfcOpeningElement), derivato dal GUID di progetto.
    fn derived_guid(&mut self) -> String {
        loop {
            self.counter += 1;
            let hi = splitmix(self.counter.wrapping_mul(2));
            let lo = splitmix(self.counter.wrapping_mul(2) + 1);
            let v = self.seed ^ ((u128::from(hi) << 64) | u128::from(lo));
            let g = IfcGuid::from_u128(v).as_str().to_owned();
            if self.used.insert(g.clone()) {
                return string(&g);
            }
        }
    }

    fn point3(&mut self, x: f64, y: f64, z: f64) -> usize {
        self.add(format!(
            "IFCCARTESIANPOINT(({},{},{}))",
            real(x),
            real(y),
            real(z)
        ))
    }

    fn dir3(&mut self, x: f64, y: f64, z: f64) -> usize {
        self.add(format!(
            "IFCDIRECTION(({},{},{}))",
            real(x),
            real(y),
            real(z)
        ))
    }

    fn axis3(&mut self, loc: usize, axes: Option<(usize, usize)>) -> usize {
        let a = match axes {
            Some((z, x)) => format!("{},{}", r(z), r(x)),
            None => "$,$".to_owned(),
        };
        self.add(format!("IFCAXIS2PLACEMENT3D({},{a})", r(loc)))
    }

    fn local_placement(&mut self, rel: Option<usize>, axis: usize) -> usize {
        let rel = rel.map_or("$".to_owned(), r);
        self.add(format!("IFCLOCALPLACEMENT({rel},{})", r(axis)))
    }

    /// IfcShapeRepresentation 'Body' 'SweptSolid' + IfcProductDefinitionShape.
    fn body_shape(&mut self, sh: &Shared, solid: usize) -> usize {
        let rep = self.add(format!(
            "IFCSHAPEREPRESENTATION({},'Body','SweptSolid',({}))",
            r(sh.body),
            r(solid)
        ));
        self.add(format!("IFCPRODUCTDEFINITIONSHAPE($,$,({}))", r(rep)))
    }

    fn rel_aggregates(&mut self, whole: usize, parts: &[usize]) {
        let g = self.derived_guid();
        self.add(format!(
            "IFCRELAGGREGATES({g},$,$,$,{},{})",
            r(whole),
            list(parts)
        ));
    }

    fn shared(&mut self) -> Shared {
        let origin = self.point3(0.0, 0.0, 0.0);
        let dir_x = self.dir3(1.0, 0.0, 0.0);
        let dir_y = self.dir3(0.0, 1.0, 0.0);
        let dir_z = self.dir3(0.0, 0.0, 1.0);
        let wcs = self.axis3(origin, None);
        let o2 = self.add("IFCCARTESIANPOINT((0.,0.))".to_owned());
        let wcs2d = self.add(format!("IFCAXIS2PLACEMENT2D({},$)", r(o2)));
        let ctx = self.add(format!(
            "IFCGEOMETRICREPRESENTATIONCONTEXT('Model','Model',3,{},{},$)",
            real(PRECISION),
            r(wcs)
        ));
        let body = self.add(format!(
            "IFCGEOMETRICREPRESENTATIONSUBCONTEXT('Body','Model',*,*,*,*,{},$,.MODEL_VIEW.,$)",
            r(ctx)
        ));
        Shared {
            dir_x,
            dir_y,
            dir_z,
            wcs,
            wcs2d,
            ctx,
            body,
        }
    }

    fn build(&mut self, model: &Model) -> Vec<(ElementId, String)> {
        let mut skipped = Vec::new();
        let sh = self.shared();
        let ctx = sh.ctx;

        // Unita' SI: lunghezza in mm, area m2, volume m3, angolo rad.
        let units = [
            self.add("IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.)".to_owned()),
            self.add("IFCSIUNIT(*,.AREAUNIT.,$,.SQUARE_METRE.)".to_owned()),
            self.add("IFCSIUNIT(*,.VOLUMEUNIT.,$,.CUBIC_METRE.)".to_owned()),
            self.add("IFCSIUNIT(*,.PLANEANGLEUNIT.,$,.RADIAN.)".to_owned()),
        ];
        let ua = self.add(format!("IFCUNITASSIGNMENT({})", list(&units)));
        let p = &model.project;
        let project = self.add(format!(
            "IFCPROJECT({},$,{},$,$,$,$,({}),{})",
            string(p.guid.as_str()),
            string(&p.name),
            r(ctx),
            r(ua)
        ));
        let site_lp = self.local_placement(None, sh.wcs);
        let site = self.add(format!(
            "IFCSITE({},$,'Sito',$,$,{},$,$,.ELEMENT.,$,$,$,$,$)",
            string(p.site_guid.as_str()),
            r(site_lp)
        ));
        let bld_lp = self.local_placement(Some(site_lp), sh.wcs);
        let building = self.add(format!(
            "IFCBUILDING({},$,'Edificio',$,$,{},$,$,.ELEMENT.,$,$,$)",
            string(p.building_guid.as_str()),
            r(bld_lp)
        ));

        // Piani.
        let mut storeys: BTreeMap<ElementId, (usize, usize)> = BTreeMap::new();
        for (&id, l) in &model.levels {
            let z = l.elevation.as_mm();
            if !z.is_finite() {
                skipped.push((id, "quota del livello non finita".to_owned()));
                continue;
            }
            let pt = self.point3(0.0, 0.0, z);
            let ax = self.axis3(pt, None);
            let lp = self.local_placement(Some(bld_lp), ax);
            let e = self.add(format!(
                "IFCBUILDINGSTOREY({},$,{},$,$,{},$,$,.ELEMENT.,{})",
                string(l.guid.as_str()),
                string(&l.name),
                r(lp),
                real(z)
            ));
            storeys.insert(id, (e, lp));
        }
        let mut contained: BTreeMap<ElementId, Vec<usize>> = BTreeMap::new();

        // Muri.
        let mut walls: BTreeMap<ElementId, WallOut> = BTreeMap::new();
        for (&id, w) in &model.walls {
            let Some(&(_, storey_lp)) = storeys.get(&w.level) else {
                skipped.push((id, "livello inesistente".to_owned()));
                continue;
            };
            let len = w.length().as_mm();
            let (t, h) = (w.thickness.as_mm(), w.height.as_mm());
            let ok = [len, t, h].iter().all(|v| v.is_finite() && *v > 0.0)
                && w.start.x.is_finite()
                && w.start.y.is_finite();
            if !ok {
                skipped.push((id, "geometria del muro degenere".to_owned()));
                continue;
            }
            let fp = geometry::wall_footprint(model, id);
            let cx = fp.iter().map(|q| q.x).sum::<f64>() / 4.0;
            let cy = fp.iter().map(|q| q.y).sum::<f64>() / 4.0;
            let d = (w.end - w.start) / len;
            let pt = self.point3(cx, cy, 0.0);
            let rd = self.dir3(d.x, d.y, 0.0);
            let ax = self.axis3(pt, Some((sh.dir_z, rd)));
            let lp = self.local_placement(Some(storey_lp), ax);
            let prof = self.add(format!(
                "IFCRECTANGLEPROFILEDEF(.AREA.,$,{},{},{})",
                r(sh.wcs2d),
                real(len),
                real(t)
            ));
            let solid = self.add(format!(
                "IFCEXTRUDEDAREASOLID({},{},{},{})",
                r(prof),
                r(sh.wcs),
                r(sh.dir_z),
                real(h)
            ));
            let pds = self.body_shape(&sh, solid);
            let e = self.add(format!(
                "IFCWALL({},$,{},$,$,{},{},$,.STANDARD.)",
                string(w.guid.as_str()),
                string(&format!("Muro {}", id.get())),
                r(lp),
                r(pds)
            ));
            contained.entry(w.level).or_default().push(e);
            walls.insert(
                id,
                WallOut {
                    entity: e,
                    placement: lp,
                    length: len,
                    thickness: t,
                },
            );
        }

        // Aperture: IfcOpeningElement (GUID derivato) + IfcDoor (GUID del dominio).
        for (&id, o) in &model.openings {
            let Some(host) = walls.get(&o.host) else {
                skipped.push((id, "muro ospite non esportato".to_owned()));
                continue;
            };
            let (off, wd, ht, sill) = (
                o.offset.as_mm(),
                o.width.as_mm(),
                o.height.as_mm(),
                o.sill.as_mm(),
            );
            let ok = [off, wd, ht, sill].iter().all(|v| v.is_finite()) && wd > 0.0 && ht > 0.0;
            if !ok {
                skipped.push((id, "dimensioni dell'apertura non valide".to_owned()));
                continue;
            }
            let (host_e, host_lp, host_len, host_t) =
                (host.entity, host.placement, host.length, host.thickness);
            let level = model.walls[&o.host].level;
            // Origine: centro della base del foro, in coordinate locali del muro.
            let x = -host_len / 2.0 + off + wd / 2.0;
            let pt = self.point3(x, 0.0, sill);
            let ax = self.axis3(pt, None);
            let lp = self.local_placement(Some(host_lp), ax);
            // Profilo nel piano XZ, estruso lungo +Y attraverso lo spessore.
            let depth = host_t + 2.0 * OPENING_MARGIN;
            let prof = self.add(format!(
                "IFCRECTANGLEPROFILEDEF(.AREA.,$,{},{},{})",
                r(sh.wcs2d),
                real(wd),
                real(ht)
            ));
            let spt = self.point3(0.0, -depth / 2.0, ht / 2.0);
            let sax = self.axis3(spt, Some((sh.dir_y, sh.dir_x)));
            let solid = self.add(format!(
                "IFCEXTRUDEDAREASOLID({},{},{},{})",
                r(prof),
                r(sax),
                r(sh.dir_z),
                real(depth)
            ));
            let pds = self.body_shape(&sh, solid);
            let og = self.derived_guid();
            let opening = self.add(format!(
                "IFCOPENINGELEMENT({og},$,{},$,$,{},{},$,.OPENING.)",
                string(&format!("Foro {}", id.get())),
                r(lp),
                r(pds)
            ));
            let door_lp = self.local_placement(Some(lp), sh.wcs);
            let door = self.add(format!(
                "IFCDOOR({},$,{},$,$,{},$,$,{},{},.DOOR.,$,$)",
                string(o.guid.as_str()),
                string(&format!("Porta {}", id.get())),
                r(door_lp),
                real(ht),
                real(wd)
            ));
            let g = self.derived_guid();
            self.add(format!(
                "IFCRELVOIDSELEMENT({g},$,$,$,{},{})",
                r(host_e),
                r(opening)
            ));
            let g = self.derived_guid();
            self.add(format!(
                "IFCRELFILLSELEMENT({g},$,$,$,{},{})",
                r(opening),
                r(door)
            ));
            contained.entry(level).or_default().push(door);
        }

        // Vani: profilo netto estruso per l'altezza del livello.
        let mut spaces: BTreeMap<ElementId, Vec<usize>> = BTreeMap::new();
        for (&id, s) in &model.spaces {
            let Some(&(_, storey_lp)) = storeys.get(&s.level) else {
                skipped.push((id, "livello inesistente".to_owned()));
                continue;
            };
            if s.boundary.iter().any(|w| !walls.contains_key(w)) {
                skipped.push((id, "muro di contorno non esportato".to_owned()));
                continue;
            }
            let poly = geometry::space_net_polygon(model, &s.boundary)
                .filter(|p| p.iter().all(|q| q.x.is_finite() && q.y.is_finite()));
            let h = model.levels[&s.level].height.as_mm();
            let (Some(poly), true) = (poly, h.is_finite() && h > 0.0) else {
                skipped.push((id, "contorno del vano non valido".to_owned()));
                continue;
            };
            let lp = self.local_placement(Some(storey_lp), sh.wcs);
            let mut pts: Vec<usize> = poly.iter().map(|q| self.point2(*q)).collect();
            pts.push(pts[0]);
            let pl = self.add(format!("IFCPOLYLINE({})", list(&pts)));
            let prof = self.add(format!("IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,{})", r(pl)));
            let solid = self.add(format!(
                "IFCEXTRUDEDAREASOLID({},{},{},{})",
                r(prof),
                r(sh.wcs),
                r(sh.dir_z),
                real(h)
            ));
            let pds = self.body_shape(&sh, solid);
            let e = self.add(format!(
                "IFCSPACE({},$,{},$,$,{},{},$,.ELEMENT.,.INTERNAL.,$)",
                string(s.guid.as_str()),
                string(&s.name),
                r(lp),
                r(pds)
            ));
            spaces.entry(s.level).or_default().push(e);
        }

        // Relazioni spaziali.
        self.rel_aggregates(project, &[site]);
        self.rel_aggregates(site, &[building]);
        let st: Vec<usize> = storeys.values().map(|v| v.0).collect();
        if !st.is_empty() {
            self.rel_aggregates(building, &st);
        }
        for (lvl, &(storey, _)) in &storeys {
            if let Some(sp) = spaces.get(lvl) {
                self.rel_aggregates(storey, sp);
            }
            if let Some(els) = contained.get(lvl) {
                let g = self.derived_guid();
                self.add(format!(
                    "IFCRELCONTAINEDINSPATIALSTRUCTURE({g},$,$,$,{},{})",
                    list(els),
                    r(storey)
                ));
            }
        }
        skipped
    }

    fn point2(&mut self, p: Point2) -> usize {
        self.add(format!("IFCCARTESIANPOINT(({},{}))", real(p.x), real(p.y)))
    }
}
