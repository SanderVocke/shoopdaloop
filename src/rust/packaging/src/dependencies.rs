use anyhow;
use anyhow::Context;
use std::path::{Path, PathBuf};
use std::collections::{HashSet, HashMap};
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
    let ori_env_vars : Vec<(String, String)> = std::env::vars().collect();
    let env_map : HashMap<String, String> = ori_env_vars.iter().cloned().collect();
    let (command, args, warning_patterns, skip_n_levels, dylib_filename_part, new_env_map) = get_os_specifics(executable, include_directory, &env_map)?;
    debug!("Running shell command for determining dependencies: {args:?}");
    let mut list_deps : &mut Command = &mut Command::new(&command);
    list_deps = list_deps.args(&args)
                         .envs(new_env_map.iter())
                         .current_dir(std::env::current_dir().unwrap());
    let list_deps_output = list_deps.output()
            .with_context(|| "Failed to run list_dependencies")?;
    let command_output = std::str::from_utf8(&list_deps_output.stderr)?;
    let deps_output = std::str::from_utf8(&list_deps_output.stdout)?;
    debug!("Command stderr:\n{}", command_output);
    debug!("Command stdout:\n{}", deps_output);
    if !list_deps_output.status.success() {
        error!("Command stderr:\n{}", command_output);
        error!("Command stdout:\n{}", deps_output);
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

fn get_os_specifics<'a>(
    executable: &'a Path,
    include_directory: &'a Path,
    env_map: &'a HashMap<String, String>,
) -> Result<(String, Vec<String>, Vec<String>, usize, &'a str, HashMap<String, String>), anyhow::Error> {
    #[cfg(target_os = "windows")]
    {
        get_windows_specifics(executable, env_map)
    }
    #[cfg(target_os = "linux")]
    {
        get_linux_specifics(executable, include_directory, env_map)
    }
    #[cfg(target_os = "macos")]
    {
        get_macos_specifics(executable, env_map)
    }
}

fn get_windows_specifics<'a>(
    executable: &'a Path,
    env_map: &'a HashMap<String, String>,
) -> Result<(String, Vec<String>, Vec<String>, usize, &'a str, HashMap<String, String>), anyhow::Error> {
    let mut new_env_map: HashMap<String, String> = HashMap::new();
    for (k, v) in env_map.iter() {
        new_env_map.insert(k.to_uppercase(), v.clone());
    }

    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path.ancestors().nth(5).ok_or(anyhow::anyhow!("cannot find src dir"))?;
    let paths_file = src_path.join("distribution/windows/shoop.dllpaths");
    let paths_str = std::fs::read_to_string(&paths_file)
        .with_context(|| format!("Cannot read {paths_file:?}"))?;
    let executable_folder = executable.parent().ok_or(anyhow::anyhow!("Could not get executable directory"))?;
    for relpath in paths_str.lines() {
        let path = new_env_map.get("PATH").expect("No PATH env var found");
        new_env_map.insert(String::from("PATH"), format!("{}/{};{}", executable_folder.to_str().unwrap(), relpath, path));
    }
    let command = String::from("powershell.exe");
    let commandstr = include_str!("scripts/windows_deps.ps1").replace("$args[0]", executable.to_str().unwrap());
    let args = vec![String::from("-Command"), commandstr];
    let warning_patterns = vec![];
    let skip_n_levels = 0;
    let dylib_filename_part = ".dll";

    Ok((command, args, warning_patterns, skip_n_levels, dylib_filename_part, new_env_map))
}

fn get_linux_specifics<'a>(
    executable: &'a Path,
    include_directory: &'a Path,
    env_map: &'a HashMap<String, String>,
) -> Result<(String, Vec<String>, Vec<String>, usize, &'a str, HashMap<String, String>), anyhow::Error> {
    let command = String::from("sh");
    let commandstr = include_str!("scripts/linux_deps.sh");
    let args = vec![
        String::from("-c"),
        String::from(commandstr),
        String::from("dummy"),
        String::from(executable.to_str().unwrap()),
        String::from(include_directory.to_str().unwrap()),
    ];
    let warning_patterns = vec![String::from("not found")];
    let skip_n_levels = 0;
    let dylib_filename_part = ".so";

    Ok((command, args, warning_patterns, skip_n_levels, dylib_filename_part, env_map.clone()))
}

fn get_macos_specifics<'a>(
    executable: &Path,
    env_map: &'a HashMap<String, String>,
) -> Result<(String, Vec<String>, Vec<String>, usize, &'a str, HashMap<String, String>), anyhow::Error> {
    let command = String::from("sh");
    let commandstr = include_str!("scripts/macos_deps.sh");
    let args = vec![
        String::from("-c"),
        String::from(commandstr),
        String::from("dummy"),
        String::from(executable.to_str().unwrap()),
    ];
    let warning_patterns = vec![];
    let skip_n_levels = 0;
    let dylib_filename_part = "";

    Ok((command, args, warning_patterns, skip_n_levels, dylib_filename_part, env_map.clone()))
}
