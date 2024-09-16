use anyhow;
use anyhow::Context;
use std::path::{Path, PathBuf};
use std::collections::{HashSet, HashMap};
use std::process::Command;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
struct InternalDependency {
    path : PathBuf,
    deps : Vec<Rc<RefCell<InternalDependency>>>,
    children_indent: usize,
    maybe_parent : Option<Rc<RefCell<InternalDependency>>>,
}

pub fn get_dependency_libs (files : &[&Path],
                            env: &HashMap<&str, &str>,
                            excludelist_path : &Path,
                            includelist_path : &Path,
                            dylib_filename_part: &str,
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
    let error_patterns: Vec<String>;
    let skip_n_levels: usize;
    let files_str = files.iter().map(|f| f.to_str().unwrap()).collect::<Vec<_>>().join(" ");
    #[cfg(target_os = "windows")]
    {
        command = String::from("powershell.exe");
        let commandstr : String = format!(
           "$dllNames = @()
            $files = \"{files_str}\" -split ' ' | ForEach-Object {{ \" $_\" -replace \"[|├]\", \" \" -replace \"([^ ]+).* : \", \"$1\" }}
            foreach ($file in $files) {{
                $output = & Dependencies.exe -chain $file;
                $newDllNames = $output | Where-Object {{ -not ($_ -match \"NotFound\") }} |
                Where-Object {{ -not ($_ -match \".exe\") }} |  ForEach-Object {{
                    $_.Trim() -split '\\s+' | Select-Object -Last 1
                }} | ForEach-Object {{ $dllNames = $dllNames + $_ }}
            }}
            # Write-Error \"Raw results:\";
            # Write-Error \"$output\";
            # Write-Error \"Dependencies:\"
            $dllNames | ForEach-Object {{ Write-Output $_ }}");
        args = vec!(String::from("-Command"), commandstr);
        error_patterns = vec!();
        skip_n_levels = 0;
    }
    #[cfg(target_os = "linux")]
    {
        command = String::from("sh");
        let commandstr : String = format!(
            "for f in {files_str}; do lddtree -a $f | grep \"=>\" | grep -E -v \"=>.*=>\" | sed -r 's/([ ]*).*=>[ ]*([^ ]*).*/\\1\\2/g'; done");
        error_patterns = vec!(String::from("not found"));
        args = vec!(String::from("-c"), commandstr);
        skip_n_levels = 1;
    }
    #[cfg(target_os = "macos")]
    {
        command = String::from("sh");
        let commandstr : String = format!(
            "function direct_deps() {{
                otool -L \"$1\" | tail -n +1 | awk 'NR>1 {{print $1}}'
             }}
             function recurse_deps() {{
                direct=$(direct_deps \"$1\")
                for dep in $direct; do
                    if [ ! -z \"$dep\" ]; then
                        echo \"$2$dep\"
                        recurse_deps \"$dep\" \"$2  \"
                    fi
                done
             }}
             for f in {files_str}; do recurse_deps $1 \"\"; done");
        args = vec!(String::from("-c"), commandstr);
        error_patterns = vec!();
        skip_n_levels = 1;
    }
    println!("Running command for determining dependencies: {} {}", &command, args.join(" "));
    let mut list_deps : &mut Command = &mut Command::new(&command);
    list_deps = list_deps.args(&args);
    for (key, val) in env {
        list_deps = list_deps.env(key, val);
    }
    let list_deps_output = list_deps.output()
            .with_context(|| "Failed to run list_dependencies")?;
    let command_output = std::str::from_utf8(&list_deps_output.stderr)?;
    let deps_output = std::str::from_utf8(&list_deps_output.stdout)?;
    println!("Command stderr:\n{}", command_output);
    println!("Command stdout:\n{}", deps_output);
    if !list_deps_output.status.success() {
        return Err(anyhow::anyhow!("list_dependencies returned nonzero exit code"));
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
                current_parent.borrow().deps.last().map(|r| r.clone());
            if indent > children_indent && maybe_prev.is_some() {
                let prev = maybe_prev.unwrap();
                let mut new_parent_mut = prev.borrow_mut();
                new_parent_mut.children_indent = indent;
                new_parent_mut.deps.push(dep.clone());
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
                    return Err(anyhow::anyhow!("Failed to find correct indent level"));
                }
            }
        }
        for pattern in &error_patterns {
            if line.contains(pattern.as_str()) {
                error_msgs.push_str(format!("{}: matched error pattern {}\n", path_str, pattern.as_str()).as_str());
                continue;
            }
        }
        let dylib_filename_pattern = dylib_filename_part.to_lowercase();
        if !path_filename.to_lowercase().contains(dylib_filename_pattern.as_str()) {
            println!("  Note: skipped ldd line (not a dynamic library): {}", line);
            continue;
        }
        if !path.exists() {
            if allow_nonexistent {
                println!("  Nonexistent file {}", &path_str);
            } else {
                error_msgs.push_str(format!("{}: doesn't exist\n", path_str).as_str());
                continue;
            }
        }
        {
            let mut dep_mut = dep.borrow_mut();
            dep_mut.maybe_parent = Some(current_parent.clone());
        }
        println!("adding {path_filename} to {:?}", current_parent.borrow().path);
        current_parent.borrow_mut().deps.push(dep);
        used_includes.insert(path_filename.to_string());
    }

    for include in includes.iter() {
        if !used_includes.contains(*include) {
            println!("  Note: library {} from include list was not required", include);
        }
    }

    fn collect_deps(d : &Rc<RefCell<InternalDependency>>,
                    includes : &HashSet<&str>,
                    excludes : &HashSet<&str>,
                    handled : &mut HashSet<String>,
                    error_msgs : &mut String,
                    paths : &mut Vec<PathBuf>,
                    skip_n_levels : usize)
    {
        let path_str : String;
        let path : PathBuf;
        {
            let db = d.borrow();
            path = db.path.clone();
            path_str = db.path.to_str().unwrap().to_owned();
        }
        let pattern_match = |s : &str, p : &str| {
            let pattern_str = &s
                .replace("\\", "\\\\")
                .replace(".", "\\.")
                .replace("*", ".*")
                .replace("+", "\\+")
                ;
            return regex::Regex::new(pattern_str).unwrap().is_match(p);
        };
        let in_excludes = excludes.iter().any(|e| pattern_match(e, path_str.as_str()));
        let in_includes = includes.iter().any(|e| pattern_match(e, path_str.as_str()));
        if in_excludes && in_includes {
            if !handled.contains(&path_str) {
                println!("  Note: dependency {} is in include and exclude, include takes precedence", &path_str);
            }
        }
        if skip_n_levels == 0 {
            if in_excludes && !in_includes {
                if !handled.contains(&path_str) {
                    println!("  Note: skipping excluded dependency {}", &path_str);
                    handled.insert(path_str.clone());
                }
                return;
            } else if !in_includes {
                if !handled.contains(&path_str) {
                    error_msgs.push_str(format!("{}: is not in include list\n", path_str).as_str());
                    handled.insert(path_str.clone());
                }
                return;
            }

            if !handled.contains(&path_str) {
                println!("  Including dependency {}", &path_str);
                paths.push (path.to_path_buf());
                handled.insert(path_str.clone());
            }
        }

        let db = d.borrow();
        for sub in db.deps.iter() { collect_deps(&sub, includes, excludes, handled, error_msgs, paths, skip_n_levels - (std::cmp::min(skip_n_levels, 1))); }
    }

    let mut paths : Vec<PathBuf> = Vec::new();
    let mut handled : HashSet<String> = HashSet::new();
    for dep in root.borrow().deps.iter() {
        collect_deps(dep, &includes, &excludes, &mut handled, &mut error_msgs, &mut paths, skip_n_levels);
    }
    if !error_msgs.is_empty() {
        return Err(anyhow::anyhow!("Dependency errors:\n{}", error_msgs));
    }
    paths.dedup();

    println!("Dependencies: {paths:?}");
    Ok(paths)
}