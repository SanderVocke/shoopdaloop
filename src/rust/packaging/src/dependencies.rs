use anyhow::anyhow;
use anyhow::Context;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

// Only the Linux scanner still shells out, and only it threads an environment
// map through to the child process.
#[cfg(target_os = "linux")]
use std::collections::HashMap;

use crate::deps_walker::InternalDependency;
use crate::list_matcher::ListMatcher;

use common::logging::macros::*;
shoop_log_unit!("packaging");

/// Determine which libraries have to be bundled alongside `executable`.
///
/// `include_directory` is the output folder being populated; anything resolving
/// inside it is traversed but not copied again.
///
/// Windows and macOS use an in-process import-table walker seeded with every
/// binary already staged in the output folder. Linux keeps its `lddtree`-based
/// scanner, which already seeds itself the same way via `patchelf --add-needed`.
/// See [`crate::deps_walker`] for why seeding from the whole folder rather than
/// from the executable alone is the entire point.
#[tracing::instrument(name = "tool.packaging.dependencies", skip_all)]
pub fn get_dependency_libs(
    executable: &Path,
    include_directory: &Path,
    excludelist_path: &Path,
    includelist_path: &Path,
    allow_nonexistent: bool,
) -> Result<HashSet<PathBuf>, anyhow::Error> {
    let mut error_msgs: String = String::from("");
    let matcher = ListMatcher::from_files(includelist_path, excludelist_path)?;
    let mut used_includes: HashSet<String> = HashSet::new();

    #[cfg(windows)]
    let (root, skip_n_levels) = {
        let _ = allow_nonexistent;
        let walk = crate::scan::prepare_windows_walk(
            include_directory,
            executable,
            &[],
            /* use_cmake_prefix_path */ true,
            /* no_system_dirs */ false,
            /* legacy_root_only */ false,
        )?;
        let staged_under_lib = walk
            .folder_index
            .paths()
            .iter()
            .filter(|p| p.starts_with(include_directory.join("lib")))
            .count();
        if staged_under_lib > 0 {
            // Expected to be zero: `lib/` is created empty and filled only after
            // this scan. A non-zero count means something pre-populated it,
            // which silently reclassifies those libraries from "to copy" to
            // "already in folder".
            warn!(
                "  {} binaries are already staged under lib/ before the scan; \
                 they will be treated as already-bundled",
                staged_under_lib
            );
        }
        let request = crate::deps_walker::ScanRequest {
            roots: walk.roots.clone(),
            output_folder: include_directory.to_path_buf(),
            report_only: false,
            max_depth: None,
        };
        let (tree, report) = crate::deps_walker::build_dependency_tree(
            &walk.scanner,
            &request,
            &walk.env,
            &matcher,
            &walk.folder_index,
            &mut error_msgs,
        )?;
        crate::scan::log_report_summary(&report);
        (tree, 0usize)
    };

    #[cfg(target_os = "macos")]
    let (root, skip_n_levels) = {
        let _ = allow_nonexistent;
        // The system-library prefixes live next to the include/exclude lists, so
        // no extra parameter has to be threaded through the OS modules.
        let system_prefixes_path = includelist_path
            .parent()
            .map(|dir| dir.join("system_lib_prefixes"));
        let walk = crate::scan::prepare_macos_walk(
            include_directory,
            executable,
            system_prefixes_path.as_deref(),
        )?;
        let request = crate::deps_walker::ScanRequest {
            roots: walk.roots.clone(),
            output_folder: include_directory.to_path_buf(),
            report_only: std::env::var_os("SHOOP_PACKAGING_DEPS_REPORT_ONLY").is_some(),
            max_depth: None,
        };
        let (tree, report) = crate::deps_walker::build_dependency_tree(
            &walk.scanner,
            &request,
            &walk.env,
            &matcher,
            &walk.folder_index,
            &mut error_msgs,
        )?;
        crate::scan::log_report_summary(&report);
        crate::scan::report_unlisted_candidates(&report);
        (tree, 0usize)
    };

    #[cfg(target_os = "linux")]
    let (root, skip_n_levels) = subprocess_dependency_tree(
        executable,
        include_directory,
        allow_nonexistent,
        &mut error_msgs,
    )?;

    let mut paths: Vec<PathBuf> = Vec::new();
    let mut handled: HashSet<String> = HashSet::new();
    for dep in root.borrow().deps.values() {
        collect_deps(
            dep,
            &matcher,
            &mut handled,
            &mut error_msgs,
            &mut used_includes,
            &mut paths,
            include_directory,
            skip_n_levels,
        )?;
    }
    for include in matcher.include_patterns() {
        if !used_includes.contains(include) {
            info!(
                "  Note: library {} from include list was not required",
                include
            );
        }
    }
    if !error_msgs.is_empty() {
        return Err(anyhow!("Dependency errors:\n{}", error_msgs));
    }
    let paths: HashSet<PathBuf> = HashSet::from_iter(paths.into_iter());

    // A library inside a framework has to be bundled as the whole framework
    // directory, so the layout the loader expects survives. Done here, after
    // selection, rather than during traversal: the walk resolves and parses the
    // inner binary, which is how it descends into frameworks in the first place,
    // and the include/exclude patterns are written against library file names.
    #[cfg(target_os = "macos")]
    let paths: HashSet<PathBuf> = crate::macho_resolve::reduce_framework_paths(paths);

    fn collect_deps(
        d: &Rc<RefCell<InternalDependency>>,
        matcher: &ListMatcher,
        handled: &mut HashSet<String>,
        error_msgs: &mut String,
        used_includes: &mut HashSet<String>,
        paths: &mut Vec<PathBuf>,
        ignore_dir: &Path,
        skip_n_levels: usize,
    ) -> Result<(), anyhow::Error> {
        let path_str: String;
        let path: PathBuf;
        {
            let db = d.borrow();
            path = db.path.clone();
            path_str = db.path.to_string_lossy().into_owned();
        }

        let matched_exclude = matcher.matched_exclude(&path_str);
        let matched_include = matcher.matched_include(&path_str);
        let in_excludes = matched_exclude.is_some();
        let in_includes = matched_include.is_some();
        let already_in_folder = path.exists()
            && ignore_dir.exists()
            && path
                .canonicalize()
                .with_context(|| format!("Couldn't canonicalize {path:?}"))?
                .starts_with(
                    ignore_dir
                        .canonicalize()
                        .with_context(|| format!("Couldn't canonicalize {ignore_dir:?}"))?,
                );
        if !already_in_folder && in_excludes && in_includes {
            if !handled.contains(&path_str) {
                warn!(
                    "  Dependency {} is in include and exclude, include takes precedence",
                    &path_str
                );
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
                    paths.push(path.to_path_buf());
                    // Record the *pattern*, not the file name: the "was not
                    // required" report below compares against patterns, so
                    // inserting a file name here meant every include was always
                    // reported as unused.
                    if let Some(pattern) = matched_include {
                        used_includes.insert(pattern.to_string());
                    }
                }
                handled.insert(path_str.clone());
            }
        }

        let db = d.borrow();
        for sub in db.deps.values() {
            collect_deps(
                &sub,
                matcher,
                handled,
                error_msgs,
                used_includes,
                paths,
                ignore_dir,
                skip_n_levels - (std::cmp::min(skip_n_levels, 1)),
            )?;
        }
        Ok(())
    }

    Ok(paths)
}

/// Determine dependencies by running a per-platform helper and parsing its
/// indented output.
///
/// Used by Linux (`lddtree`) and macOS (`otool`). Windows has an in-process
/// walker instead; see [`get_dependency_libs`].
#[cfg(target_os = "linux")]
fn subprocess_dependency_tree(
    executable: &Path,
    include_directory: &Path,
    allow_nonexistent: bool,
    error_msgs: &mut String,
) -> Result<(Rc<RefCell<InternalDependency>>, usize), anyhow::Error> {
    use std::process::Command;

    let ori_env_vars: Vec<(String, String)> = std::env::vars().collect();
    let env_map: HashMap<String, String> = ori_env_vars.iter().cloned().collect();
    let (command, args, warning_patterns, skip_n_levels, dylib_filename_part, new_env_map) =
        get_linux_specifics(executable, include_directory, &env_map)?;
    let argstr = args.join(" ");
    debug!("Running shell command for determining dependencies: {argstr}");
    let mut list_deps: &mut Command = &mut Command::new(&command);
    list_deps = list_deps
        .args(&args)
        .envs(new_env_map.iter())
        .current_dir(std::env::current_dir().context("Failed to get current dir")?);
    let list_deps_output = list_deps
        .output()
        .with_context(|| "Failed to run list_dependencies")?;
    let command_output = std::str::from_utf8(&list_deps_output.stderr)?;
    let deps_output = std::str::from_utf8(&list_deps_output.stdout)?;
    debug!("Command stderr:\n{}", command_output);
    debug!("Command stdout:\n{}", deps_output);
    if !list_deps_output.status.success() {
        error!("Command stderr:\n{}", command_output);
        error!("Command stdout:\n{}", deps_output);
        return Err(anyhow!("list_dependencies returned nonzero exit code"));
    }
    for line in command_output.lines() {
        for pattern in &warning_patterns {
            if line.contains(pattern.as_str()) {
                warn!(
                    "{}: stderr line matched warning pattern {}",
                    line,
                    pattern.as_str()
                );
            }
        }
    }

    let root = parse_dependency_tree(
        deps_output,
        &warning_patterns,
        dylib_filename_part,
        allow_nonexistent,
        error_msgs,
    );
    Ok((root, skip_n_levels))
}

/// How to invoke `lddtree`, and how to interpret its output.
///
/// The script prepends every `*.so*` in the output folder to the executable's
/// needed list with `patchelf`, so the Linux scan is already seeded from the
/// whole folder -- the property Windows and macOS previously lacked and now get
/// from [`crate::deps_walker`].
#[cfg(target_os = "linux")]
fn get_linux_specifics<'a>(
    executable: &'a Path,
    include_directory: &'a Path,
    env_map: &'a HashMap<String, String>,
) -> Result<
    (
        String,
        Vec<String>,
        Vec<String>,
        usize,
        &'a str,
        HashMap<String, String>,
    ),
    anyhow::Error,
> {
    let command = String::from("sh");
    let commandstr = include_str!("scripts/linux_deps.sh");
    let args = vec![
        String::from("-c"),
        String::from(commandstr),
        String::from("dummy"),
        String::from(executable.to_str().ok_or(anyhow!("Invalid unicode"))?),
        String::from(
            include_directory
                .to_str()
                .ok_or(anyhow!("Invalid unicode"))?,
        ),
    ];
    let warning_patterns = vec![String::from("not found")];
    let skip_n_levels = 0;
    let dylib_filename_part = ".so";

    Ok((
        command,
        args,
        warning_patterns,
        skip_n_levels,
        dylib_filename_part,
        env_map.clone(),
    ))
}

/// Reconstruct a dependency tree from a helper's indented stdout.
///
/// Only the subprocess path uses this, but it stays compiled everywhere so its
/// unit tests keep running on any host -- they are the evidence that changes to
/// the Windows path left the Linux one alone.
#[allow(dead_code)]
fn parse_dependency_tree(
    deps_output: &str,
    warning_patterns: &[String],
    dylib_filename_part: &str,
    allow_nonexistent: bool,
    error_msgs: &mut String,
) -> Rc<RefCell<InternalDependency>> {
    let root: Rc<RefCell<InternalDependency>> =
        Rc::new(RefCell::new(InternalDependency::default()));
    let mut current_parent: Rc<RefCell<InternalDependency>> = root.clone();
    for line in deps_output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let path = PathBuf::from(line.trim());
        let path_str = path.to_string_lossy();
        let path_filename = match path.file_name() {
            Some(f) => f.to_string_lossy(),
            None => {
                warn!("Missing filename in path: {}", path_str);
                continue;
            }
        };

        if let Some(pattern) = warning_patterns
            .iter()
            .find(|pattern| line.contains(pattern.as_str()))
        {
            warn!(
                "{}: stdout line matched pattern {}",
                path_str,
                pattern.as_str()
            );
            continue;
        }

        let dylib_filename_pattern = dylib_filename_part.to_lowercase();
        if !path_filename
            .to_lowercase()
            .contains(dylib_filename_pattern.as_str())
        {
            warn!(
                "  Note: skipped dependency line (not a dynamic library): {}",
                line
            );
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

        // Validate the line before changing the tree. Tools such as lddtree can
        // emit unresolved pseudo-paths like `None`; allowing one of those to
        // participate in indentation handling can insert it as a real node.
        let indent = line.chars().take_while(|&c| c == ' ').count();

        if Rc::ptr_eq(&current_parent, &root) && root.borrow().deps.len() == 0 {
            // First line, this is our base indentation
            let mut root_mut = root.borrow_mut();
            root_mut.children_indent = indent;
        }

        let dep: Rc<RefCell<InternalDependency>> =
            Rc::new(RefCell::new(InternalDependency::default()));
        {
            let mut dep_mut = dep.borrow_mut();
            dep_mut.path = path.clone();
        }
        {
            let mut children_indent = current_parent.borrow().children_indent;
            let maybe_prev: Option<Rc<RefCell<InternalDependency>>> =
                current_parent.borrow().deps.last().map(|r| r.1.clone());
            if indent > children_indent && maybe_prev.is_some() {
                // TODO: Avoid panic call
                let prev = maybe_prev.expect("Guarded by is_some check");
                let mut new_parent_mut = prev.borrow_mut();
                new_parent_mut.children_indent = indent;
                new_parent_mut.deps.insert(path.clone(), dep.clone());
                current_parent = prev.clone();
            } else if indent < children_indent {
                while indent < children_indent {
                    let parent = current_parent.borrow().maybe_parent.clone();

                    if let Some(parent) = parent {
                        current_parent = parent;
                        children_indent = current_parent.borrow().children_indent;
                    } else {
                        // We reached the root, but the indent is still smaller than the root's children indent?
                        // This means the indentation is inconsistent with the tree structure we built so far.
                        // We should stop traversing up.
                        break;
                    }
                }

                // If we are here, we either found the correct level, or we reached the top and the indent is still small.
                // In either case, we treat this node as a child of the current parent, establishing a new indentation level for this parent if needed.
                current_parent.borrow_mut().children_indent = indent;
            } else if children_indent != indent {
                // cannot recover
                // This error path is now unreachable due to the change above.
                // If children_indent == indent, we continue.
                // If children_indent < indent, we set new children_indent.
                // If children_indent > indent, we traverse up until children_indent <= indent.
                // If after traversing up, children_indent < indent, we set new children_indent.
                // If after traversing up, children_indent == indent, we continue.
                // So, children_indent != indent should not happen here.
            }
        }
        {
            let mut dep_mut = dep.borrow_mut();
            dep_mut.maybe_parent = Some(current_parent.clone());
        }
        if !current_parent.borrow_mut().deps.contains_key(&path) {
            debug!(
                "adding {} to {:?} at indent {}",
                path_filename,
                current_parent.borrow().path,
                indent
            );
            current_parent.borrow_mut().deps.insert(path.clone(), dep);
        } else {
            debug!("{} already handled, skipping", path_filename);
        }
    }

    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipped_non_library_lines_do_not_enter_the_tree() {
        let input = r#"
    /tmp/libroot.so
        None
        /tmp/libchild.so
"#;

        let mut error_msgs = String::new();
        let root = parse_dependency_tree(input, &[], ".so", true, &mut error_msgs);

        assert!(error_msgs.is_empty());
        let root_borrow = root.borrow();
        assert_eq!(root_borrow.deps.len(), 1);
        assert!(!root_borrow.deps.contains_key(Path::new("None")));
        let root_library = root_borrow.deps.values().next().unwrap().borrow();
        assert_eq!(root_library.deps.len(), 1);
        assert!(root_library
            .deps
            .contains_key(Path::new("/tmp/libchild.so")));
        assert!(!root_library.deps.contains_key(Path::new("None")));
    }

    #[test]
    fn test_crash_repro() {
        let input = r#"
   D:/a/shoopdaloop/shoopdaloop/shoopdaloop.65442fab.release-windows-msvc-x64.portable\shoopdaloop_exe.exe 
      C:\Windows\system32\bcryptPrimitives.dll 
         C:\Windows\system32\ntdll.dll 
         C:\Windows\system32\kernelbase.dll 
            C:\Windows\system32\advapi32.dll 
               C:\Windows\system32\MSVCRT.dll 
               C:\Windows\system32\sechost.dll 
               C:\Windows\system32\kernel32.dll 
                      
                     C:\Windows\system32\rpcrt4.dll 
               C:\Windows\system32\CRYPTSP.dll"#;

        let mut error_msgs = String::new();
        // We use allow_nonexistent=true because these paths likely don't exist on the test runner machine
        let root = parse_dependency_tree(input, &[], ".dll", true, &mut error_msgs);

        // Basic validaton that we parsed something
        let root_deps = &root.borrow().deps;
        assert_eq!(root_deps.len(), 1);

        // Check structure
        let exe = root_deps.values().next().unwrap();
        let exe_deps = &exe.borrow().deps;
        assert!(exe_deps.len() > 0);
    }
}
