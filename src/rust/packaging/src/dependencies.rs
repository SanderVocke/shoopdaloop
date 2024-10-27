use anyhow;
use anyhow::Context;
use std::path::{Path, PathBuf};
use std::collections::HashSet;
use indexmap::IndexMap;
use std::process::Command;
use std::cell::RefCell;
use std::rc::Rc;

use common::logging::macros::*;
shoop_log_unit!("packaging");

#[derive(Default)]
struct InternalDependency {
    path : PathBuf,
    deps : IndexMap<PathBuf, Rc<RefCell<InternalDependency>>>,
    children_indent: usize,
    maybe_parent : Option<Rc<RefCell<InternalDependency>>>,
}

pub fn get_dependency_libs (executable : &Path,
                            include_directory : &Path,
                            excludelist_path : &Path,
                            includelist_path : &Path,
                            allow_nonexistent: bool) -> Result<Vec<PathBuf>, anyhow::Error> {
    let mut error_msgs : String = String::from("");

    let excludelist = std::fs::read_to_string(excludelist_path)
            .with_context(|| format!("Cannot read {excludelist_path:?}"))?;
    let includelist = std::fs::read_to_string(includelist_path)
            .with_context(|| format!("Cannot read {excludelist_path:?}"))?;
    let excludes : HashSet<&str> = excludelist.lines().collect();
    let includes : HashSet<&str> = includelist.lines().collect();
    let mut used_includes : HashSet<String> = HashSet::new();


    let command : String;
    let args: Vec<String>;
    let warning_patterns: Vec<String>;
    let skip_n_levels: usize;
    let dylib_filename_part: &str;
    // let files_str = files.iter().map(|f| f.to_str().unwrap()).collect::<Vec<_>>().join(" ");
    #[cfg(target_os = "windows")]
    {
        command = String::from("powershell.exe");
        let commandstr : String = format!(
           "
            $files = \"{0}\" -split ' '
            foreach ($file in $files) {{
                $output = & Dependencies.exe -chain -depth 4 $file.Trim();
                $output | Where-Object {{ -not ($_ -match \"NotFound\") }} |
                    Get-Unique |
                    ForEach-Object {{ \" $_\" -replace \"[|├]\", \" \" -replace \"([^ ]+).* : \", \"$1\" }} |
                    ForEach-Object {{ Write-Output $_ }}
            }}", executable.to_str().unwrap());
        args = vec!(String::from("-Command"), commandstr);
        warning_patterns = vec!();
        skip_n_levels = 0;
        dylib_filename_part = ".dll";
    }
    #[cfg(target_os = "linux")]
    {
        // lddtree is nice but unaware of libraries loaded at run-time. It also becomes really slow
        // if you give it multiple files.
        // So we use patchelf to inject dependencies on all .so files that are already in the bundle.
        // That way lddtree is forced to include all libraries that the main executable could potentially load.
        command = String::from("sh");
        let commandstr : String = format!(
            "
             EXE=$(mktemp);
             cp {0} $EXE;
             for f in $(find {1} -type f -name \"*.so*\"); do patchelf --add-needed $(basename $f) $EXE; done;
             RAW=$(LD_LIBRARY_PATH=$(find {1} -type d | xargs printf \"%s:\" | sed 's/,$/\\n/'):$LD_LIBRARY_PATH lddtree $EXE)
             printf \"$RAW\\n\" | grep -v \"not found\" | grep \"=>\" | grep -E -v \"=>.*=>\" | sed -r 's/([ ]*).*=>[ ]*([^ ]*).*/\\1\\2/g';
             printf \"$RAW\\n\" | grep \"not found\" >&2;
             rm $EXE;", executable.to_str().unwrap(), include_directory.to_str().unwrap());
        warning_patterns = vec!(String::from("not found"));
        args = vec!(String::from("-c"), commandstr);
        skip_n_levels = 0;
        dylib_filename_part = ".so";
    }
    #[cfg(target_os = "macos")]
    {
        command = String::from("sh");
        let commandstr : String = format!(
            "
            set -x
            exe=\"{0}\"
            handled=\"\"
            exe_dir=$(dirname \"$exe\")
            exe_dir_escaped=$(dirname \"$exe\" | sed 's/\\//\\\\\\//g')
            search_dirs=$(otool -l \"$exe\" | grep -A2 LC_RPATH | grep -E \" path [/@]\" | awk '
            NR>1 {{print $2}}' | sed \"s/@loader_path/$exe_dir_escaped/g\")
            
            function resolve_rpath() {{
                if [ -f \"$1\" ]; then echo $1; return
                elif [[ $1 == *\"@rpath\"* ]]; then
                    for dir in $search_dirs; do
                        trypath=\"$dir/$(echo $1 | sed 's/@rpath//g')\"
                        if [ -f \"$trypath\" ]; then
                          echo \"$trypath\"
                          return
                        fi
                    done
                fi
            }}
            
            function recurse_deps() {{
                for f in $(otool -L \"$1\" | tail -n +1 | awk 'NR>1 {{print $1}}' || true); do
                    resolved=$(resolve_rpath \"$f\" \"$2\")
                    resolved_filename=$(basename \"$resolved\")
                    filtered=$(echo $handled | grep \"$resolved_filename\")
                    if [ ! -z \"$resolved\" ] && [ -z \"$filtered\" ]; then
                        echo \"${{2}}$resolved\"
                        handled=$(printf \"$handled\n$resolved_filename\")
                        recurse_deps \"$resolved\" \"$2  \"
                    fi 
                done
            }}
            recurse_deps \"$exe\" \"\"",
            executable.to_str().unwrap());
        args = vec!(String::from("-c"), commandstr);
        warning_patterns = vec!();
        skip_n_levels = 0;
        dylib_filename_part = ".dylib";
    }
    debug!("Running shell command for determining dependencies: {}", args.last().ok_or(anyhow::anyhow!("Empty args list"))?);
    let mut list_deps : &mut Command = &mut Command::new(&command);
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    list_deps = list_deps.args(&args)
                         .envs(env_vars)
                         .current_dir(std::env::current_dir().unwrap());
    let list_deps_output = list_deps.output()
            .with_context(|| "Failed to run list_dependencies")?;
    let command_output = std::str::from_utf8(&list_deps_output.stderr)?;
    let deps_output = std::str::from_utf8(&list_deps_output.stdout)?;
    debug!("Command stderr:\n{}", command_output);
    debug!("Command stdout:\n{}", deps_output);
    if !list_deps_output.status.success() {
        return Err(anyhow::anyhow!("list_dependencies returned nonzero exit code"));
    }
    for line in command_output.lines() {
        for pattern in &warning_patterns {
            if line.contains(pattern.as_str()) {
                warn!("{}: stderr line matched warning pattern {}", line, pattern.as_str());
            }
        }
    }

    let root : Rc<RefCell<InternalDependency>> = Rc::new(RefCell::new(InternalDependency::default()));
    let mut current_parent : Rc<RefCell<InternalDependency>> = root.clone();
    for line in deps_output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let path = PathBuf::from(line.trim());
        let path_str = path.to_str().ok_or(anyhow::anyhow!("cannot find dependency"))?;
        let path_filename = path.file_name().unwrap().to_str().unwrap();
        let indent = line.chars()
                            .take_while(|&c| c == ' ')
                            .count();

        if Rc::ptr_eq(&current_parent, &root) && root.borrow().deps.len() == 0 {
            // First line, this is our base indentation
            let mut root_mut = root.borrow_mut();
            root_mut.children_indent = indent;
        }

        let dep : Rc<RefCell<InternalDependency>> = Rc::new(RefCell::new(InternalDependency::default()));
        {
            let mut dep_mut = dep.borrow_mut();
            dep_mut.path = path.clone();
        }
        {
            let mut children_indent = current_parent.borrow().children_indent;
            let maybe_prev : Option<Rc<RefCell<InternalDependency>>> =
                current_parent.borrow().deps.last().map(|r| r.1.clone());
            if indent > children_indent && maybe_prev.is_some() {
                let prev = maybe_prev.unwrap();
                let mut new_parent_mut = prev.borrow_mut();
                new_parent_mut.children_indent = indent;
                new_parent_mut.deps.insert(path.clone(), dep.clone());
                current_parent = prev.clone();
            } else if indent < children_indent {
                while indent < children_indent {
                    let parent = current_parent.borrow().maybe_parent.clone()
                        .ok_or(anyhow::anyhow!("Failed to traverse outward"))?
                        .clone();
                    current_parent = parent;
                    children_indent = current_parent.borrow().children_indent;
                }
                if children_indent != indent {
                    return Err(anyhow::anyhow!("Failed to find correct indent level: {children_indent} vs. {indent}"));
                }
            }
        }
        for pattern in &warning_patterns {
            if line.contains(pattern.as_str()) {
                warn!("{}: stdout line matched pattern {}", path_str, pattern.as_str());
                continue;
            }
        }
        let dylib_filename_pattern = dylib_filename_part.to_lowercase();
        if !path_filename.to_lowercase().contains(dylib_filename_pattern.as_str()) {
            warn!("  Note: skipped dependency line (not a dynamic library): {}", line);
            continue;
        }
        if !path.exists() {
            if allow_nonexistent {
                warn!("  Nonexistent file {}", &path_str);
            } else {
                error_msgs.push_str(format!("{}: doesn't exist\n", path_str).as_str());
                continue;
            }
        }
        {
            let mut dep_mut = dep.borrow_mut();
            dep_mut.maybe_parent = Some(current_parent.clone());
        }
        if !current_parent.borrow_mut().deps.contains_key(&path) {
            debug!("adding {path_filename} to {:?} at indent {}", current_parent.borrow().path, indent);
            current_parent.borrow_mut().deps.insert(path.clone(), dep);
        } else {
            debug!("{path_filename} already handled, skipping");
        }
    }

    fn collect_deps(d : &Rc<RefCell<InternalDependency>>,
                    includes : &HashSet<&str>,
                    excludes : &HashSet<&str>,
                    handled : &mut HashSet<String>,
                    error_msgs : &mut String,
                    used_includes: &mut HashSet<String>,
                    paths : &mut Vec<PathBuf>,
                    ignore_dir : &Path,
                    skip_n_levels : usize) -> Result<(), anyhow::Error>
    {
        let path_str : String;
        let path : PathBuf;
        {
            let db = d.borrow();
            path = db.path.clone();
            path_str = db.path.to_str().unwrap().to_owned();
        }
        let pattern_match = |s : &str, p : &str| {
            let path_str = &p
                .replace("\\", "/")
                .to_lowercase();
            let pattern_str = &s
                .replace("\\", "\\\\")
                .replace(".", "\\.")
                .replace("*", ".*")
                .replace("+", "\\+")
                .to_lowercase();
            return regex::Regex::new(pattern_str).unwrap().is_match(path_str);
        };
        let in_excludes = excludes.iter().any(|e| pattern_match(e, path_str.as_str()));
        let in_includes = includes.iter().any(|e| pattern_match(e, path_str.as_str()));
        let already_in_folder = path.exists() && ignore_dir.exists() &&
                                path.canonicalize()
                                    .with_context(|| format!("Couldn't canonicalize {path:?}"))?
                                    .starts_with(ignore_dir.canonicalize()
                                                           .with_context(|| format!("Couldn't canonicalize {ignore_dir:?}"))?);
        if !already_in_folder && in_excludes && in_includes {
            if !handled.contains(&path_str) {
                warn!("  Dependency {} is in include and exclude, include takes precedence", &path_str);
            }
        }
        if skip_n_levels == 0 {
            if !already_in_folder && in_excludes && !in_includes {
                if !handled.contains(&path_str) {
                    info!("  Skipping excluded dependency {}", &path_str);
                    handled.insert(path_str.clone());
                }
                return Ok(());
            } else if !already_in_folder && !in_includes {
                if !handled.contains(&path_str) {
                    error_msgs.push_str(format!("{}: is not in include list\n", path_str).as_str());
                    handled.insert(path_str.clone());
                }
                return Ok(());
            }

            if !handled.contains(&path_str) {
                if already_in_folder {
                    debug!("  Traversing dependency (already in folder): {}", &path_str);
                } else {
                    info!("  Including dependency {}", &path_str);
                    paths.push (path.to_path_buf());
                    let path_filename = path.file_name().unwrap().to_str().unwrap();
                    used_includes.insert(path_filename.to_string());
                }
                handled.insert(path_str.clone());
            }
        }

        let db = d.borrow();
        for sub in db.deps.values() { collect_deps(&sub, includes, excludes, handled, error_msgs, used_includes,
                                                   paths, ignore_dir, skip_n_levels - (std::cmp::min(skip_n_levels, 1)))?; }
        Ok(())
    }

    let mut paths : Vec<PathBuf> = Vec::new();
    let mut handled : HashSet<String> = HashSet::new();
    for dep in root.borrow().deps.values() {
        collect_deps(dep, &includes, &excludes, &mut handled, &mut error_msgs, &mut used_includes, &mut paths, &include_directory, skip_n_levels)?;
    }
    for include in includes.iter() {
        if !used_includes.contains(*include) {
            info!("  Note: library {} from include list was not required", include);
        }
    }
    if !error_msgs.is_empty() {
        return Err(anyhow::anyhow!("Dependency errors:\n{}", error_msgs));
    }
    paths.dedup();

    info!("Dependencies: {paths:?}");
    Ok(paths)
}