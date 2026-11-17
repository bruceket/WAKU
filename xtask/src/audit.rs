use anyhow::{Context, bail};
use cargo_metadata::{DependencyKind, Metadata, MetadataCommand};

const NATIVE_BUILD_DEPS: [&str; 3] = ["cc", "cmake", "bindgen"];
const DENY_PREFIXES: [&str; 8] = [
    "opencascade",
    "clipper",
    "manifold",
    "ifcopenshell",
    "tauri",
    "webview2",
    "wry",
    "pyo3",
];
const DENY_EXACT: [&str; 3] = ["v8", "deno_core", "rhai"];

fn denylisted(name: &str) -> bool {
    DENY_EXACT.contains(&name) || DENY_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Violazioni della policy "Rust-only": runtime/kernel vietati e build nativi non approvati.
pub fn violations(meta: &Metadata, allowed: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for pkg in &meta.packages {
        let name = pkg.name.as_str();
        if denylisted(name) {
            out.push(format!("{name}: denylisted runtime"));
            continue;
        }
        let has_build_rs = pkg
            .targets
            .iter()
            .any(|t| t.kind.iter().any(|k| k.to_string() == "custom-build"));
        if !has_build_rs || allowed.contains(&name) {
            continue;
        }
        let native = pkg.dependencies.iter().find(|d| {
            d.kind == DependencyKind::Build && NATIVE_BUILD_DEPS.contains(&d.name.as_str())
        });
        if let Some(d) = native {
            out.push(format!("{name}: native build ({})", d.name));
        }
    }
    out
}

fn parse_allowed(doc: &str) -> Vec<&str> {
    doc.lines()
        .filter_map(|l| l.trim().strip_prefix("- "))
        .filter_map(|l| l.split(':').next())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect()
}

pub fn run() -> anyhow::Result<()> {
    let meta = MetadataCommand::new().exec().context("cargo metadata")?;
    let doc_path = meta.workspace_root.join("docs/native-deps.md");
    let doc = std::fs::read_to_string(&doc_path).with_context(|| format!("lettura {doc_path}"))?;
    let v = violations(&meta, &parse_allowed(&doc));
    if v.is_empty() {
        println!("audit: ok");
        return Ok(());
    }
    for line in &v {
        eprintln!("audit: {line}");
    }
    bail!("audit: {} violazioni", v.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(name: &str) -> Metadata {
        let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    #[test]
    fn flags_cc_dependent_not_allowed() {
        assert_eq!(
            violations(&load("meta_cc.json"), &[]),
            vec!["foo: native build (cc)"]
        );
    }

    #[test]
    fn flags_denylisted_rhai() {
        assert_eq!(
            violations(&load("meta_rhai.json"), &[]),
            vec!["rhai: denylisted runtime"]
        );
    }

    #[test]
    fn allows_listed() {
        assert!(violations(&load("meta_cc.json"), &["foo"]).is_empty());
    }
}
