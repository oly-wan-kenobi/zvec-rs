use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor_header = manifest_dir.join("vendor").join("c_api.h");

    println!("cargo:rerun-if-changed=vendor/c_api.h");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=ZVEC_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=ZVEC_LIB_DIR");
    println!("cargo:rerun-if-env-changed=ZVEC_ROOT");
    println!("cargo:rerun-if-env-changed=ZVEC_STATIC");
    println!("cargo:rerun-if-env-changed=DOCS_RS");

    // Let downstream crates discover the vendored headers.
    println!(
        "cargo:include={}",
        manifest_dir.join("vendor").display()
    );

    let header_path = resolve_header(&manifest_dir, &vendor_header);

    generate_bindings(&header_path);

    if env::var_os("DOCS_RS").is_some() {
        // docs.rs sandbox: skip linking, we only build for rustdoc.
        return;
    }

    configure_link();
}

fn resolve_header(manifest_dir: &PathBuf, vendor_header: &PathBuf) -> PathBuf {
    // Prefer a user-specified include dir so bindings match their installed lib.
    if let Some(dir) = env::var_os("ZVEC_INCLUDE_DIR") {
        let candidate = PathBuf::from(dir).join("zvec").join("c_api.h");
        if candidate.exists() {
            return candidate;
        }
    }
    if let Some(root) = env::var_os("ZVEC_ROOT") {
        let candidate = PathBuf::from(root)
            .join("include")
            .join("zvec")
            .join("c_api.h");
        if candidate.exists() {
            return candidate;
        }
    }

    assert!(
        vendor_header.exists(),
        "vendored header {} not found and no ZVEC_INCLUDE_DIR/ZVEC_ROOT override",
        vendor_header.display()
    );
    let _ = manifest_dir;
    vendor_header.clone()
}

fn generate_bindings(header: &PathBuf) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let bindings = bindgen::Builder::default()
        .header(header.to_string_lossy())
        .allowlist_function("zvec_.*")
        .allowlist_type("zvec_.*")
        .allowlist_var("ZVEC_.*")
        .default_enum_style(bindgen::EnumVariation::NewType {
            is_bitfield: false,
            is_global: false,
        })
        .constified_enum_module("zvec_error_code_t")
        .prepend_enum_name(false)
        .derive_debug(true)
        .derive_default(true)
        .generate_comments(true)
        .size_t_is_usize(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("failed to generate zvec C API bindings");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write bindings.rs");
}

fn configure_link() {
    // 1. Explicit ZVEC_LIB_DIR / ZVEC_ROOT takes precedence over everything.
    if let Some(dir) = env::var_os("ZVEC_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", PathBuf::from(dir).display());
        emit_link();
        return;
    }
    if let Some(root) = env::var_os("ZVEC_ROOT") {
        let lib = PathBuf::from(&root).join("lib");
        println!("cargo:rustc-link-search=native={}", lib.display());
        let lib64 = PathBuf::from(&root).join("lib64");
        if lib64.exists() {
            println!("cargo:rustc-link-search=native={}", lib64.display());
        }
        emit_link();
        return;
    }

    // 2. Optional pkg-config probe.
    #[cfg(feature = "pkg-config")]
    {
        if pkg_config::Config::new()
            .atleast_version("0.3.0")
            .probe("zvec_c_api")
            .is_ok()
        {
            return;
        }
    }

    // 3. Fall back to the system linker's default search paths.
    emit_link();
}

fn emit_link() {
    let kind = if env::var_os("ZVEC_STATIC").is_some() {
        "static"
    } else {
        "dylib"
    };
    println!("cargo:rustc-link-lib={}=zvec_c_api", kind);
}
