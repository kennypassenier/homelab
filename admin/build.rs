//! fix-179 follow-up: the dashboard's embedded web files are the files on
//! disk, not a hand-kept list. Twice a new module was written and never
//! added to the old `FILES` list in main.rs (fix-177's six pages, then
//! fix-179's perstack helper), and the page broke only once deployed.
//! Everything under web/js and web/css plus web/index.html is embedded;
//! tests, type stubs, node_modules and the refusal page (served by the
//! guard before the locks) stay out by not being under those roots.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn main() {
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
    let mut files = vec![web.join("index.html")];
    for root in ["js", "css"] {
        walk(&web.join(root), &mut files);
        println!("cargo:rerun-if-changed=web/{root}");
    }
    println!("cargo:rerun-if-changed=web/index.html");
    files.sort();
    let mut src = String::from("pub const FILES: &[(&str, &[u8])] = &[\n");
    for f in &files {
        let rel = f
            .strip_prefix(&web)
            .expect("under web/")
            .to_string_lossy()
            .replace('\\', "/");
        writeln!(
            src,
            "    ({rel:?}, include_bytes!({:?})),",
            f.display().to_string()
        )
        .expect("write to a String");
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    std::fs::write(out.join("web_files.rs"), src).expect("write web_files.rs");
}
