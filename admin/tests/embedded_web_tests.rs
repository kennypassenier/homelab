//! Every script and stylesheet under admin/web is embedded in the binary.
//! js/drivepace.js was added on 2026-09-29 without an entry in FILES; the
//! page would have asked for it and got a 404 from the released dashboard.

#[test]
fn every_web_js_and_css_file_is_embedded() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let main = std::fs::read_to_string(root.join("src/main.rs")).unwrap();
    let mut missing = Vec::new();
    for dir in ["js", "js/pages", "css"] {
        let Ok(entries) = std::fs::read_dir(root.join("web").join(dir)) else {
            continue;
        };
        for e in entries {
            let path = e.unwrap().path();
            let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
            if !matches!(ext, "js" | "css" | "json") {
                continue;
            }
            let rel = format!("{dir}/{}", path.file_name().unwrap().to_string_lossy());
            if !main.contains(&format!("\"{rel}\"")) {
                missing.push(rel);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "not embedded in admin/src/main.rs: {missing:?}"
    );
}
