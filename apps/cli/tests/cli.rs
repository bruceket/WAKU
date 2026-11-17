use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

struct Tmp(PathBuf);
impl Tmp {
    fn new(tag: &str) -> Tmp {
        let p = std::env::temp_dir().join(format!("waku-cli-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
    fn f(&self, n: &str) -> PathBuf {
        self.0.join(n)
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cli() -> Command {
    Command::cargo_bin("waku-cli").unwrap()
}

fn info(p: &PathBuf) -> serde_json::Value {
    let out = cli().arg("info").arg(p).output().unwrap();
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn sample_then_export_structure() {
    let d = Tmp::new("exp");
    let (w, o) = (d.f("a.waku"), d.f("a.ifc"));
    cli().arg("sample").arg(&w).assert().success();
    let v = info(&w);
    assert_eq!(v["walls"], 4);
    assert_eq!(v["spaces"].as_array().unwrap().len(), 1);
    cli()
        .arg("export-ifc")
        .arg(&w)
        .arg(&o)
        .env("WAKU_FIXED_TIMESTAMP", "2026-01-01T00:00:00")
        .assert()
        .success();
    let ifc = std::fs::read_to_string(&o).unwrap();
    assert!(ifc.contains("2026-01-01T00:00:00"));
    let count = |k: &str| ifc.matches(&format!("={k}(")).count();
    assert_eq!(count("IFCWALL"), 4);
    assert_eq!(count("IFCOPENINGELEMENT"), 1);
    assert_eq!(count("IFCDOOR"), 1);
    assert_eq!(count("IFCSPACE"), 1);
    assert_eq!(count("IFCRELVOIDSELEMENT"), 1);
    assert_eq!(count("IFCRELFILLSELEMENT"), 1);
    // ogni GUID di dominio salvato compare nell'IFC
    let model = persistence::load(&w).unwrap();
    let mut guids = vec![model.project.guid.as_str().to_owned()];
    guids.extend(model.walls.values().map(|x| x.guid.as_str().to_owned()));
    guids.extend(model.openings.values().map(|x| x.guid.as_str().to_owned()));
    guids.extend(model.spaces.values().map(|x| x.guid.as_str().to_owned()));
    for g in guids {
        assert!(ifc.contains(&format!("'{g}'")), "GUID mancante {g}");
    }
}

#[test]
fn apply_adds_wall_and_bumps_revision() {
    let d = Tmp::new("ap");
    let w = d.f("a.waku");
    cli()
        .arg("new")
        .arg(&w)
        .args(["--unit", "cm"])
        .assert()
        .success();
    let before = info(&w);
    let level = before["levels"][0]["id"].as_u64().unwrap();
    let j = d.f("act.json");
    std::fs::write(
        &j,
        format!(
            r#"[{{"op":"CreateWall","level":{level},"start":[0,0],"end":[1000,0],"thickness_mm":100,"height_mm":2700}}]"#
        ),
    )
    .unwrap();
    cli().arg("apply").arg(&w).arg(&j).assert().success();
    let after = info(&w);
    assert_eq!(after["walls"], 1);
    assert_eq!(after["revision"], 0); // la revisione non e' persistita
    assert!(!d.f("a.waku.lock").exists());
}

#[test]
fn apply_rejects_when_locked() {
    let d = Tmp::new("lk");
    let w = d.f("a.waku");
    cli().arg("new").arg(&w).assert().success();
    std::fs::write(d.f("a.waku.lock"), "x").unwrap();
    let j = d.f("act.json");
    std::fs::write(&j, "[]").unwrap();
    cli()
        .arg("apply")
        .arg(&w)
        .arg(&j)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("progetto aperto altrove"));
}

#[test]
fn apply_invalid_json_exit_1_file_untouched() {
    let d = Tmp::new("ij");
    let w = d.f("a.waku");
    cli().arg("new").arg(&w).assert().success();
    let before = std::fs::read(&w).unwrap();
    let j = d.f("act.json");
    std::fs::write(&j, "{non json").unwrap();
    cli().arg("apply").arg(&w).arg(&j).assert().code(1);
    assert_eq!(std::fs::read(&w).unwrap(), before);
    assert!(!d.f("a.waku.lock").exists());
}

#[test]
fn apply_domain_error_exit_1_file_untouched() {
    let d = Tmp::new("de");
    let w = d.f("a.waku");
    cli().arg("new").arg(&w).assert().success();
    let before = std::fs::read(&w).unwrap();
    let j = d.f("act.json");
    std::fs::write(&j, r#"[{"op":"DeleteWall","id":999}]"#).unwrap();
    cli().arg("apply").arg(&w).arg(&j).assert().code(1);
    assert_eq!(std::fs::read(&w).unwrap(), before);
}

#[test]
fn new_and_sample_refuse_overwrite_without_force() {
    let d = Tmp::new("force");
    let w = d.f("a.waku");
    cli().arg("sample").arg(&w).assert().success();
    let before = std::fs::read(&w).unwrap();
    for sub in ["new", "sample"] {
        cli()
            .arg(sub)
            .arg(&w)
            .assert()
            .code(1)
            .stderr(predicate::str::contains("esiste gia'"));
        assert_eq!(std::fs::read(&w).unwrap(), before);
    }
    cli().arg("new").arg(&w).arg("--force").assert().success();
    assert_eq!(info(&w)["walls"], 0);
    cli()
        .arg("sample")
        .arg(&w)
        .arg("--force")
        .assert()
        .success();
    assert_eq!(info(&w)["walls"], 4);
}
