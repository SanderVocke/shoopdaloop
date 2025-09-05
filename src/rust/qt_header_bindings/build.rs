use anyhow;

// For now, Rust "back-end" is just a set of C bindings to the
// C++ back-end.
fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    if cfg!(feature = "prebuild") {
        return Ok(());
    }

    let qnamespace_header = option_env!("SHOOP_QNAMESPACE_HEADER").unwrap_or_default();
    let qt_include_dir = option_env!("SHOOP_QT_INCLUDE_DIR").unwrap_or_default();
    let gen_lib_path = "src/codegen/qt_header_bindings.rs";

    // Generate Rust bindings
    let bindings = bindgen::Builder::default()
        .header(qnamespace_header)
        .allowlist_type("Qt::Key")
        .allowlist_type("Qt::KeyboardModifier")
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: false,
        })
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-std=c++17")
        .clang_arg("-I")
        .clang_arg(qt_include_dir)
        .generate()
        .expect("Unable to generate Qt header bindings.");

    // post-process the file to add enum iteration capabilities.
    let mut file = syn::parse_file(bindings.to_string().as_str())
        .expect("Failed to parse generated bindings.");
    for item in &mut file.items {
        if let syn::Item::Enum(item_enum) = item {
            item_enum
                .attrs
                .push(syn::parse_quote! { #[derive(enum_iterator::Sequence)] });
        }
    }

    std::fs::write(gen_lib_path, prettyplease::unparse(&file))
        .expect("Failed to write generated, modified bindings");

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
