use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
// Handle static inline functions using wrap_static_fns
// Example: https://github.com/rust-lang/rust-bindgen/discussions/2405#discussioncomment-5109826

fn bindings_builder(extern_path: &Path) -> bindgen::Builder {
    bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        // wrap for static function.
        .wrap_static_fns(true)
        .wrap_static_fns_path(extern_path)
}

/// `clang -print-resource-dir`, e.g. `/usr/lib/llvm-18/lib/clang/18`.
fn clang_resource_dir() -> Option<String> {
    let output = Command::new("clang")
        .arg("-print-resource-dir")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let dir = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if dir.is_empty() {
        None
    } else {
        Some(dir)
    }
}

/// `gcc -print-multiarch`, e.g. `x86_64-linux-gnu`.
fn gcc_multiarch() -> Option<String> {
    let output = Command::new("gcc")
        .arg("-print-multiarch")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let dir = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if dir.is_empty() {
        None
    } else {
        Some(dir)
    }
}

/// Generate the bindings, retrying once with a fully pinned C include set
/// when the ambient toolchain image is broken.
///
/// Some toolchain images leak libstdc++'s C++-only header directory
/// (`/usr/include/c++/<ver>`) into the C include search path. liburing's
/// `barrier.h` includes `<stdatomic.h>`; clang's resource-dir copy of that
/// header then delegates with `#include_next` to the C++-only variant, which
/// declares nothing in C mode, and the parse fails with `use of undeclared
/// identifier 'memory_order_*'`. Neither `-isystem` ordering nor
/// `-nostdinc++` can help, because the delegation still lands on the empty
/// header. The retry therefore strips every ambient include directory with
/// `-nostdinc`, clears any leaked `*_INCLUDE_PATH` variables (those survive
/// `-nostdinc`), and re-adds exactly the C paths the parse needs.
fn generate_bindings(extern_path: &Path) -> bindgen::Bindings {
    match bindings_builder(extern_path).generate() {
        Ok(bindings) => bindings,
        Err(first_error) => {
            println!(
                "cargo:warning=bindgen failed ({first_error}); \
                 retrying with a pinned C include set"
            );
            for var in [
                "CPATH",
                "C_INCLUDE_PATH",
                "CPLUS_INCLUDE_PATH",
                "OBJC_INCLUDE_PATH",
                "OBJCPLUS_INCLUDE_PATH",
            ] {
                env::remove_var(var);
            }
            let mut pinned_args: Vec<String> = vec!["-nostdinc".to_owned()];
            if let Some(resource_dir) = clang_resource_dir() {
                pinned_args.push("-isystem".to_owned());
                pinned_args.push(format!("{resource_dir}/include"));
            }
            let mut dirs = vec!["/usr/local/include".to_owned()];
            if let Some(multiarch) = gcc_multiarch() {
                dirs.push(format!("/usr/include/{multiarch}"));
            }
            dirs.push("/usr/include".to_owned());
            for dir in dirs {
                pinned_args.push("-isystem".to_owned());
                pinned_args.push(dir);
            }
            bindings_builder(extern_path)
                .clang_args(pinned_args)
                .generate()
                .expect("Unable to generate bindings")
        }
    }
}

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

    let bindings = generate_bindings(&extern_path);

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
