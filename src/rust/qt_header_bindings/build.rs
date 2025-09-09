use std::process::Command;

use anyhow;

fn qmake_command(qmake_path: &str, argstring: &str) -> Command {
    let shell_command = format!("{} {}", qmake_path, argstring);
    return if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", format!("{shell_command}").as_str()]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", format!("{shell_command}").as_str()]);
        cmd
    };
}

// For now, Rust "back-end" is just a set of C bindings to the
// C++ back-end.
fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    if cfg!(feature = "prebuild") {
        return Ok(());
    }

    let qmake = option_env!("QMAKE").unwrap_or("qmake");

    let qt_include_dir = qmake_command(qmake, "-query QT_INSTALL_HEADERS")
        .stderr(std::process::Stdio::inherit())
        .output()?;
    let qt_include_dir = String::from_utf8(qt_include_dir.stdout)?.trim().to_string();

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let qnamespace_header = format!("{manifest_dir}/include/qnamespace_wrapper.h");
    let gen_lib_path = "src/codegen/qt_header_bindings.rs";

    // Generate Rust bindings
    let builder = bindgen::Builder::default()
        .header(&qnamespace_header)
        .allowlist_type("Qt::Key")
        .allowlist_type("Qt::KeyboardModifier")
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: false,
        })
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-std=c++17")
        .clang_arg("-I")
        .clang_arg(qt_include_dir);

    let bindings = builder
        .generate()
        .map_err(|e| anyhow::anyhow!("Generation failed: {e}"))?;

    // post-process the file to add enum iteration capabilities.
    let mut file = syn::parse_file(bindings.to_string().as_str())
        .map_err(|e| anyhow::anyhow!("Parsing failed: {e}"))?;
    for item in &mut file.items {
        if let syn::Item::Enum(item_enum) = item {
            item_enum
                .attrs
                .push(syn::parse_quote! { #[derive(enum_iterator::Sequence)] });
        }
    }

    std::fs::write(gen_lib_path, prettyplease::unparse(&file))?;

    println!("cargo:rerun-if-changed={}", qnamespace_header);
    println!("cargo:rerun-if-changed=src/lib.rs");
    Ok(())
}

fn main() {
    match main_impl() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {:?}\nBacktrace: {:?}", e, e.backtrace());
            std::process::exit(1);
        }
    }
}
