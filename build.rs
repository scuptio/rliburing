use std::env;
use std::path::PathBuf;
use std::process::Command;
// Handle static inline functions using wrap_static_fns
// Example: https://github.com/rust-lang/rust-bindgen/discussions/2405#discussioncomment-5109826

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    let c_header_file = "wrapper.h";
    let link_lib = "wrapper_extern";
    let lib_file = "libwrapper_extern.a";

    // This is the path to the object file.
    let obj_path = PathBuf::from(&out_dir).join("wrapper_extern.o");
    // This is the path to the static library file.
    let lib_path = PathBuf::from(&out_dir).join(lib_file);
    // This is the extern wrapper c file
    let extern_path = PathBuf::from(&out_dir).join("bindgen").join("wrapper_extern.c");

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header(c_header_file)
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        // wrap for static function.
        .wrap_static_fns(true)
        .wrap_static_fns_path(&extern_path)
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    let project_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Compile the generated wrappers into an object file.
    let clang_output = Command::new("clang")
        .arg("-O")
        .arg("-c")
        .arg("-o")
        .arg(&obj_path)
        .arg(&extern_path)
        .arg(format!("-I{}", project_root.to_str().unwrap()))
        .arg("-include")
        .arg(c_header_file)
        .output()
        .unwrap();
    if !clang_output.status.success() {
        panic!(
            "Could not compile object file:\n{}",
            String::from_utf8_lossy(&clang_output.stderr)
        );
    }

    let lib_output = Command::new("ar")
        .arg("rcs")
        .arg(lib_path)
        .arg(obj_path)
        .output()
        .unwrap();
    if !lib_output.status.success() {
        panic!(
            "Could not emit library file:\n{}",
            String::from_utf8_lossy(&lib_output.stderr)
        );
    }

    // Tell cargo to look for shared libraries in the specified directory
    println!("cargo:rustc-link-search=/usr/lib/x86_64-linux-gnu/");
    println!("cargo:rustc-link-search=/usr/lib/");
    println!("cargo:rustc-link-search=native={}", out_dir);

    // Tell cargo to tell rustc to link the system extern wrapper and uring shared library.
    println!("cargo:rustc-link-lib=uring");
    println!("cargo:rustc-link-lib={}", link_lib);
    bindings
        .write_to_file(PathBuf::from(&out_dir).join("bindings.rs"))
        .expect("Couldn't write bindings!");

    pkg_config::probe_library("liburing").unwrap();
}
