//! Salvaguarda de arquitectura: el acceso a la cuenta real solo puede construirse en el
//! composition root (`apps/trader-tui/src`) y en el archivo del adaptador que lo define.
//!
//! Recorre todo el código Rust del workspace y falla si la autorización del modo real o el
//! constructor de la configuración real aparecen en cualquier otro lugar: pruebas de
//! integración, otros crates, ejemplos o benchmarks. Así ninguna prueba automática puede
//! conectarse a una cuenta con dinero.

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

/// Se arman por partes para que este archivo no se detecte a sí mismo.
const FORBIDDEN: [&str; 3] = [
    concat!("LiveTrading", "Authorization"),
    concat!("AlpacaConfig", "::live"),
    concat!("Alpaca", "Environment::", "Live"),
];

/// Únicos lugares donde pueden aparecer (rutas relativas a la raíz, con `/`).
const ALLOWED_PREFIXES: [&str; 4] = [
    "apps/trader-tui/src/",
    "crates/adapters/broker-alpaca/src/config.rs",
    "crates/adapters/broker-alpaca/src/live_authorization.rs",
    "crates/adapters/broker-alpaca/src/lib.rs",
];

/// Carpetas que no son código fuente del workspace.
const SKIPPED_DIRS: [&str; 5] = ["target", ".git", "ml", "node_modules", "logs"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if path.is_dir() {
            if !SKIPPED_DIRS.contains(&name) {
                rust_files(&path, found);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn live_access_only_exists_in_the_composition_root() {
    let root = workspace_root();
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    assert!(
        files.len() > 50,
        "se esperaban muchos archivos .rs en {}; ¿cambió la estructura?",
        root.display()
    );

    let mut violations = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if ALLOWED_PREFIXES
            .iter()
            .any(|allowed| relative.starts_with(allowed))
        {
            continue;
        }
        let content = fs::read_to_string(file).unwrap();
        for needle in FORBIDDEN {
            if content.contains(needle) {
                violations.push(format!("{relative}: usa `{needle}`"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "el acceso a la cuenta real solo puede construirse en apps/trader-tui/src:\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_composition_root_still_requires_the_confirmation() {
    // Si alguien quitara la frase del composition root, esta prueba lo nota: `main` debe
    // construir la configuración real solo después de pedir la confirmación
    let main = fs::read_to_string(workspace_root().join("apps/trader-tui/src/main.rs")).unwrap();
    let asks = main
        .find("live_confirmation::ask")
        .expect("main.rs debe pedir la frase de confirmación");
    let builds = main
        .find(FORBIDDEN[1])
        .expect("main.rs construye la configuración real");
    assert!(
        asks < builds,
        "la frase debe pedirse antes de construir la configuración real"
    );
}
