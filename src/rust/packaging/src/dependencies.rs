use anyhow;
use anyhow::Context;
use std::path::{PathBuf, Path};
use std::collections::HashSet;
use std::process::Command;

pub fn get_dependency_libs (exe : &Path,
                            src_dir : &Path,
                            excludelist_path : &Path,
                            includelist_path : &Path,
                            dylib_filename_part: &str,
                            allow_nonexistent: bool) -> Result<Vec<PathBuf>, anyhow::Error> {
    let mut rval : Vec<PathBuf> = Vec::new();
    let mut error_msgs : String = String::from("");

    let excludelist = std::fs::read_to_string(excludelist_path)
            .with_context(|| format!("Cannot read {excludelist_path:?}"))?;
    let includelist = std::fs::read_to_string(includelist_path)
            .with_context(|| format!("Cannot read {excludelist_path:?}"))?;
    let excludes : HashSet<&str> = excludelist.lines().collect();
    let includes : HashSet<&str> = includelist.lines().collect();
    let mut used_includes : HashSet<String> = HashSet::new();

    let command = "bash";
    let list_deps_script = src_dir.join("scripts/list_dependencies.sh");
    let args = [
        list_deps_script.to_str().unwrap(),
        exe.to_str().unwrap(),
        "-b", "dummy",
        "--quit-after", "1"
    ];
    println!("Running command for determining dependencies: {} {}", command, args.join(" "));
    let list_deps_output = Command::new(command)
            .args(&args)
            .output()
            .with_context(|| "Failed to run list_dependencies")?;
    let command_output = std::str::from_utf8(&list_deps_output.stderr)?;
    let deps_output = std::str::from_utf8(&list_deps_output.stdout)?;
    println!("Command stderr:\n{}", command_output);
    println!("Command stdout:\n{}", deps_output);

    for line in deps_output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // Extract the full path to the library
        let path = PathBuf::from(line.trim());
        let path_str = path.to_str().ok_or(anyhow::anyhow!("cannot find dependency"))?;
        let path_filename = path.file_name().unwrap().to_str().unwrap();
        if path_filename.contains(dylib_filename_part) {
            let pattern_match = |s : &str, p : &str| regex::Regex::new(
                    &s.replace(".", "\\.")
                    .replace("*", ".*")
                    .replace("+", "\\+")
                ).unwrap().is_match(p);
            let in_excludes = excludes.iter().any(|e| pattern_match(e, path_str));
            let in_includes = includes.iter().any(|e| pattern_match(e, path_str));
            if !path.exists() {
                if allow_nonexistent {
                    println!("  Nonexistent file {}", &path_str);
                } else {
                    error_msgs.push_str(format!("{}: doesn't exist\n", path_str).as_str());
                    continue;
                }
            }
            if in_excludes && in_includes {
                error_msgs.push_str(format!("{}: is in includes and excludes\n", path_str).as_str());
                continue;
            } else if in_excludes {
                println!("  Note: skipping excluded dependency {}", &path_str);
                continue;
            } else if !in_includes {
                error_msgs.push_str(format!("{}: is not in include list\n", path_str).as_str());
                continue;
            }

            used_includes.insert(path_filename.to_string());
            rval.push(path);
        } else {
            println!("  Note: skipped ldd line (not a .so): {}", line);
        }
    }

    if !error_msgs.is_empty() {
        return Err(anyhow::anyhow!("Dependency errors:\n{}", error_msgs));
    }

    for include in includes {
        if !used_includes.contains(include) {
            println!("  Note: library {} from include list was not required", include);
        }
    }

    Ok(rval)
}