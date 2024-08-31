use std::env;
use std::path::{Path, PathBuf};
use glob::glob;

// Include generated source for remembering the OUT_DIR at build time.
const SHOOP_BUILD_OUT_DIR : &str = env!("OUT_DIR");

fn recursive_dir_cpy (src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = path.file_name().unwrap();
        if path.is_dir() {
            std::fs::create_dir(dst.join(file_name))?;
            recursive_dir_cpy(&path, &dst.join(file_name))?;
        } else {
            std::fs::copy(&path, &dst.join(file_name))?;
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let file_path = PathBuf::from(file!());
    let src_path = file_path.ancestors().nth(6).unwrap();

    let usage = || {
        println!("Usage: build_appdir shoopdaloop_executable target_dir");
        println!("  the target directory should not exist, but its parent should.");
        std::process::exit(1);
    };

    let bad_usage = |msg : _| {
        println!("Bad usage: {}", msg);
        usage();
    };

    if args.len() != 3 {
        bad_usage(format!("Wrong number of arguments: {} vs. {}", args.len(), 3));
    }

    let exe_path = Path::new(&args[1]);
    if !exe_path.is_file() {
        bad_usage("First argument is not an executable file".to_string());
    }

    let target_dir = Path::new(&args[2]);
    if target_dir.exists() {
        bad_usage("Target directory already exists".to_string());
    }
    if !target_dir.parent().unwrap().exists() {
        bad_usage("Target directory parent doesn't exist".to_string());
    }

    println!("Assets directory: {}", SHOOP_BUILD_OUT_DIR);

    println!("Creating target directory...");
    std::fs::create_dir(target_dir).unwrap();

    println!("Copying executable...");
    std::fs::copy(exe_path, target_dir.join("AppRun")).unwrap();

    println!("Copying assets...");
    let lib_dir = target_dir.join("shoop_lib");
    std::fs::create_dir(&lib_dir).unwrap();
    recursive_dir_cpy(
        &PathBuf::from(SHOOP_BUILD_OUT_DIR).join("shoop_lib"),
        &lib_dir
    ).unwrap();
    {
        let from_base = PathBuf::from(SHOOP_BUILD_OUT_DIR).join("shoop_pyenv");
        let from : PathBuf;
        {
            let pattern = format!("{}/**/site-packages", from_base.to_str().unwrap());
            let mut sp_glob = glob(&pattern).unwrap();
            from = sp_glob.next()
                    .expect(format!("No site-packages dir found @ {}", pattern).as_str())
                    .unwrap();
        }
        let to = lib_dir.join("py");
        std::fs::create_dir(&to).unwrap();
        recursive_dir_cpy(
            &from,
            &to
        ).unwrap();
    }
    std::fs::copy(
        src_path.join("distribution").join("appimage").join("shoopdaloop.desktop"),
        target_dir.join("shoopdaloop.desktop")
    ).unwrap();
    std::fs::copy(
        src_path.join("distribution").join("appimage").join("shoopdaloop.png"),
        target_dir.join("shoopdaloop.png")
    ).unwrap();

    println!("AppDir produced in {}", target_dir.to_str().unwrap());
    println!("You can use a tool such as appimagetool to create an AppImage from this directory.");
}