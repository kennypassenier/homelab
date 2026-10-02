//! Every script and stylesheet under admin/web is embedded in the binary.
//! A hand-kept list in main.rs missed new modules three times (js/drivepace.js
//! on 2026-09-29, fix-177's six pages, fix-179's js/perstack.js), each a 404
//! in the released dashboard. Since then build.rs generates the list from
//! the files on disk; this test keeps it that way.

#[test]
fn the_embedded_web_files_are_generated_not_listed_by_hand() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let main = std::fs::read_to_string(root.join("src/main.rs")).unwrap();
    assert!(
        main.contains(r#"include!(concat!(env!("OUT_DIR"), "/web_files.rs"));"#),
        "admin/src/main.rs must take FILES from build.rs's generated web_files.rs"
    );
    assert!(
        !main.contains("include_bytes!(\"../web/"),
        "admin/src/main.rs embeds a web file by hand again; build.rs owns that list"
    );
    let build = std::fs::read_to_string(root.join("build.rs")).unwrap();
    for walked in ["\"js\"", "\"css\"", "index.html"] {
        assert!(build.contains(walked), "build.rs no longer embeds {walked}");
    }
}
