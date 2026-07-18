//! Native content-directory reading.
//!
//! The loader itself is pure (it takes sources); this module supplies them
//! from disk on native builds. The web build embeds or fetches content
//! instead — that backend lands with the delivery milestone.

#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::host::ContentSource;

/// Recursively reads every `.rhai` file under `root`, returning sources
/// with content-relative forward-slash paths in sorted order.
pub fn read_content_dir(root: &Path) -> io::Result<Vec<ContentSource>> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rhai") {
                files.push(path);
            }
        }
    }

    let mut sources = Vec::with_capacity(files.len());
    for path in files {
        let relative = path
            .strip_prefix(root)
            .expect("walked paths are under root")
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        sources.push(ContentSource {
            path: relative,
            source: fs::read_to_string(&path)?,
        });
    }
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(sources)
}
