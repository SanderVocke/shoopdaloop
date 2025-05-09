use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use regex::Regex;
use walkdir::WalkDir;

/// Converts duplicated library files into a symlinked structure.
pub fn deduplicate_libraries(root: &Path) -> std::io::Result<()> {
    let lib_re = Regex::new(r"^(lib.+\.so)(?:\.(\d+(?:\.\d+)*))?$").unwrap();

    let mut lib_map: HashMap<String, Vec<(PathBuf, Option<String>)>> = HashMap::new();

    // Walk the directory tree and collect all matching .so files
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
            if let Some(caps) = lib_re.captures(file_name) {
                let base = caps.get(1).unwrap().as_str().to_string();
                let version = caps.get(2).map(|m| m.as_str().to_string());
                lib_map.entry(base).or_default().push((path.to_path_buf(), version));
            }
        }
    }

    // Process each set of versions
    for (base, mut entries) in lib_map {
        // Sort by specificity: most versioned last (to be kept)
        entries.sort_by(|a, b| version_cmp(&a.1, &b.1));

        if let Some((real_path, _)) = entries.last() {
            for (path, _) in entries.iter().filter(|(p, _)| p != real_path) {
                if let Some(name) = path.file_name() {
                    let target_name = real_path.file_name().unwrap();
                    let rel_target = PathBuf::from(target_name);
                    println!("Replacing {} with symlink to {}", path.display(), rel_target.display());

                    // Remove old file and replace with symlink
                    fs::remove_file(path)?;
                    std::os::unix::fs::symlink(&rel_target, path)?;
                }
            }
        }
    }

    Ok(())
}

/// Compares version strings by breaking them into numeric components.
fn version_cmp(a: &Option<String>, b: &Option<String>) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(va), Some(vb)) => {
            let va_parts: Vec<_> = va.split('.').filter_map(|s| s.parse::<u32>().ok()).collect();
            let vb_parts: Vec<_> = vb.split('.').filter_map(|s| s.parse::<u32>().ok()).collect();
            va_parts.cmp(&vb_parts)
        }
    }
}