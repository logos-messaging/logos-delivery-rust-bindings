use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(logosdelivery_static)");
    println!("cargo:rerun-if-env-changed=LOGOS_DELIVERY_LIB_DIR");
    println!("cargo:rerun-if-env-changed=LOGOS_DELIVERY_RELOCATABLE");
    println!("cargo:rerun-if-env-changed=LOGOS_DELIVERY_ALLOW_UNSTAMPED");

    let Some(lib_dir) = locate_lib_dir() else {
        println!(
            "cargo:warning=liblogosdelivery could not be located; `cargo check`/\
             `clippy` will pass, but a binary that starts a node will fail at link. Set \
             LOGOS_DELIVERY_LIB_DIR to the directory containing the library."
        );
        return;
    };

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    match target_os.as_str() {
        "macos" | "linux" | "ios" | "android" => {}
        other => panic!("unsupported OS for logos-delivery transport: {other}"),
    }

    // Two linking modes, because dev builds and *distributable* builds want
    // opposite things out of the library's install name / soname.
    if relocatable() || target_os == "ios" || target_os == "android" {
        // Distribution: link the shipped library in place and leave its
        // relocatable name (@rpath on macOS, $ORIGIN soname on Linux) intact,
        // so the consumer can copy it into its own bundle and resolve it from
        // there. The library's own @loader_path/$ORIGIN rpath then finds
        // librln beside it. This costs the consumer some build-script glue --
        // on macOS it MUST add an rpath, since cargo does not propagate
        // `rustc-link-arg` across crates -- which is exactly what the default
        // mode below exists to avoid. `lib_dir` is published as
        // DEP_LOGOSDELIVERY_LIB_DIR so direct dependents can locate the
        // libraries to bundle.
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
    } else {
        // Default (dev): stamp a private copy with an ABSOLUTE install name.
        // The propagating search + lib directives are then sufficient and
        // consumers need zero build-script glue -- but the resulting binary
        // hardcodes a nix store path and only runs on this machine.
        let stamped = match target_os.as_str() {
            "macos" => stamp_absolute_macos(&lib_dir, &out_dir),
            "linux" => stamp_absolute_linux(&lib_dir, &out_dir),
            _ => unreachable!("target OS validated above"),
        };
        if stamped {
            println!("cargo:rustc-link-search=native={out_dir}");
        } else {
            // A library built without header padding (or without patchelf) cannot be
            // renamed. Linking it in place would build fine but leave every
            // dependant unable to find it at run time, and cargo hides this script's
            // warnings for git and registry dependencies, so it is opt-in.
            assert!(
                std::env::var("LOGOS_DELIVERY_ALLOW_UNSTAMPED").as_deref() == Ok("1"),
                "could not stamp an absolute install name on: {}; install patchelf (Linux), \
                 build the library with -headerpad_max_install_names (macOS), or set \
                 LOGOS_DELIVERY_RELOCATABLE=1 and give your binary an rpath. \
                 LOGOS_DELIVERY_ALLOW_UNSTAMPED=1 links it in place for this crate's own tests only",
                lib_dir.display()
            );
            println!(
                "cargo:warning=could not stamp an absolute install name on the library; \
                 linking it in place"
            );
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
        }
    }

    if target_os == "ios" {
        println!("cargo:rustc-cfg=logosdelivery_static");
        // iOS apps cannot ship loose dylibs the way an APK can, so the delivery
        // node is linked as a static archive. rln has to be named explicitly:
        // with no shared library there is no rpath to resolve it transitively.
        println!("cargo:rustc-link-lib=static=logosdelivery");
        println!("cargo:rustc-link-lib=static=rln");
        println!("cargo:rustc-link-lib=c++");
    } else {
        println!("cargo:rustc-link-lib=dylib=logosdelivery");
    }
    println!("cargo:lib_dir={}", lib_dir.display());
}

/// Opt-in relocatable linking for builds that get shipped to other machines.
/// Off by default so existing consumers (and this repo's own tests) keep the
/// zero-glue absolute-path behaviour.
fn relocatable() -> bool {
    matches!(
        std::env::var("LOGOS_DELIVERY_RELOCATABLE").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// Locate the native library directory as an ABSOLUTE, canonical path. Prefers
/// `LOGOS_DELIVERY_LIB_DIR`. Returns `None` when it is unset (e.g. `cargo check`).
fn locate_lib_dir() -> Option<PathBuf> {
    // An empty value (a stale env file) means unset.
    let dir = std::env::var("LOGOS_DELIVERY_LIB_DIR")
        .ok()
        .filter(|d| !d.is_empty())?;
    if let Some(resolved) = resolve_lib_dir(&dir) {
        return Some(resolved);
    }
    // Without PWD a relative path cannot be anchored, and the warning below would
    // be hidden for git and registry dependencies: fail instead of a bare link error.
    assert!(
        Path::new(&dir).is_absolute() || std::env::var("PWD").is_ok(),
        "LOGOS_DELIVERY_LIB_DIR is relative: {dir}, but PWD is not set to anchor it; use an absolute path"
    );
    // A path that no longer exists (e.g. a garbage-collected nix result) behaves as
    // unset so that cargo check and editor tooling keep working.
    println!("cargo:warning=LOGOS_DELIVERY_LIB_DIR could not be resolved: {dir}");
    None
}

/// Resolve a lib dir to an absolute, canonical path. Cargo runs build scripts
/// with the cwd set to the crate dir, which for a git or registry dependency is
/// inside cargo's cache, so a relative value is anchored at the directory cargo
/// was invoked from (`PWD`), where the user wrote it. Prefer an absolute path.
/// Canonicalizing also follows symlinks (e.g. nix's `result`) to the immutable
/// path, so the stamped install name / soname stays stable.
fn resolve_lib_dir(dir: &str) -> Option<PathBuf> {
    let path = Path::new(dir);
    let anchored = if path.is_absolute() {
        path.to_path_buf()
    } else {
        println!("cargo:rerun-if-env-changed=PWD");
        let invoked_from = std::env::var("PWD").ok()?;
        Path::new(&invoked_from).join(path)
    };
    // Re-run once the lib appears, so build order (nix build vs. cargo) is free.
    println!("cargo:rerun-if-changed={}", anchored.display());
    if let Some(parent) = anchored.parent() {
        println!("cargo:rerun-if-changed={}", parent.display());
    }
    anchored.canonicalize().ok()
}

/// Copy `liblogosdelivery.dylib` into `OUT_DIR` and rewrite its install name to
/// the absolute store path. The consumer records that absolute path, so dyld
/// loads the original file directly — whose own `@loader_path` RPATH resolves
/// `librln.dylib` beside it — with no RPATH needed on the consumer.
fn stamp_absolute_macos(lib_dir: &Path, out_dir: &str) -> bool {
    let src = lib_dir.join("liblogosdelivery.dylib");
    let dst = format!("{out_dir}/liblogosdelivery.dylib");
    require_library(&src);
    copy_writable(&src, Path::new(&dst));
    println!("cargo:rerun-if-changed={}", src.display());
    run("install_name_tool", &["-id", path_str(&src), &dst])
}

/// Linux equivalent: an absolute `DT_SONAME` is recorded verbatim in the
/// consumer's `DT_NEEDED`, so `ld.so` loads it by path with no RPATH. Requires
/// `patchelf` at build time (provided by the nix devshell).
fn stamp_absolute_linux(lib_dir: &Path, out_dir: &str) -> bool {
    let src = lib_dir.join("liblogosdelivery.so");
    let dst = format!("{out_dir}/liblogosdelivery.so");
    require_library(&src);
    copy_writable(&src, Path::new(&dst));
    println!("cargo:rerun-if-changed={}", src.display());
    run("patchelf", &["--set-soname", path_str(&src), &dst])
}

fn require_library(lib: &Path) {
    assert!(
        lib.exists(),
        "{} not found: LOGOS_DELIVERY_LIB_DIR must hold the dynamic library \
         (e.g. `make liblogosdelivery` or the nix package's lib/)",
        lib.display()
    );
}

fn path_str(p: &Path) -> &str {
    p.to_str()
        .unwrap_or_else(|| panic!("non-UTF-8 path: {}", p.display()))
}

fn copy_writable(src: &Path, dst: &Path) {
    use std::os::unix::fs::PermissionsExt;

    fs::copy(src, dst).unwrap_or_else(|e| {
        panic!(
            "could not copy: {} to: {}, error: {e}",
            src.display(),
            dst.display()
        )
    });
    // Store-sourced files are read-only; restore owner write so the install
    // name / soname can be rewritten.
    fs::set_permissions(dst, fs::Permissions::from_mode(0o644)).unwrap();
}

/// Whether `cmd` ran and succeeded.
fn run(cmd: &str, args: &[&str]) -> bool {
    matches!(Command::new(cmd).args(args).status(), Ok(status) if status.success())
}
