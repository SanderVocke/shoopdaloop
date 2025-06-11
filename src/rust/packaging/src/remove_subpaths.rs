use std::path::{Path, PathBuf};

pub fn remove_subpaths(paths : &Vec<String>) -> Vec<String> {
    let normalize_path = |path: &Path| -> PathBuf {
        PathBuf::from(std::fs::canonicalize(path).unwrap().to_str().unwrap().trim_start_matches(r"\\?\"))
    };
    
    let mut out : Vec<String> = vec![];
    for path in paths.iter().map(|p| PathBuf::from(p)) {
        let mut include = true;
        for other_path in paths.iter().map(|p| PathBuf::from(p)) {
            let on = normalize_path(&other_path);
            let n = normalize_path(&path);
            // TODO: this is a bit of a hack, but it's the only way to make sure we don't copy 
            if on == n { continue; }
            if n.starts_with(on) { include = false; break; }
        }
        if include { out.push(path.to_string_lossy().into_owned()); }
    }
    return out;
}