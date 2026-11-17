use std::fs;
use std::path::Path;

fn main() {
    let dist = Path::new("frontend/dist");
    let index = dist.join("index.html");
    if !index.exists() {
        let _ = fs::create_dir_all(dist);
        let _ = fs::write(
            &index,
            "<!doctype html><meta charset=\"utf-8\"><title>relaybook-dashboard</title><p>Frontend is not built yet. Run npm run build in frontend/.</p>\n",
        );
    }
    println!("cargo:rerun-if-changed=frontend/dist/index.html");
    println!("cargo:rerun-if-changed=build.rs");
}
