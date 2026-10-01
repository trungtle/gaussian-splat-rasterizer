//! Compiles src/shaders/*.slang to SPIR-V with `slangc`.
//!
//! slangc is located via $SLANGC, then $VULKAN_SDK/Bin, then PATH.

use std::path::{Path, PathBuf};
use std::process::Command;

const SHADER: &str = "src/shaders/splat.slang";
/// (entry point, stage, output file)
const ENTRIES: &[(&str, &str, &str)] = &[("vsMain", "vertex", "splat.vert.spv"), ("fsMain", "fragment", "splat.frag.spv")];

fn slangc() -> PathBuf {
    if let Some(p) = std::env::var_os("SLANGC") {
        return p.into();
    }
    let exe = if cfg!(windows) { "slangc.exe" } else { "slangc" };
    if let Some(sdk) = std::env::var_os("VULKAN_SDK") {
        for bin in ["Bin", "bin"] {
            let p = Path::new(&sdk).join(bin).join(exe);
            if p.exists() {
                return p;
            }
        }
    }
    exe.into()
}

fn main() {
    println!("cargo:rerun-if-changed={SHADER}");
    println!("cargo:rerun-if-env-changed=SLANGC");
    println!("cargo:rerun-if-env-changed=VULKAN_SDK");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let slangc = slangc();
    for (entry, stage, file) in ENTRIES {
        let status = Command::new(&slangc)
            .args([SHADER, "-target", "spirv", "-entry", entry, "-stage", stage])
            .args(["-matrix-layout-column-major", "-O2", "-o"])
            .arg(out.join(file))
            .status()
            .unwrap_or_else(|e| panic!("failed to run {}: {e} (install the Vulkan SDK or set SLANGC)", slangc.display()));
        assert!(status.success(), "slangc failed for {entry}");
    }
}
