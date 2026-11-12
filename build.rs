use std::path::Path;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(valox_nnue_weights)");
    let model = Path::new("networks/v60-7f587dfb.nnue");
    println!("cargo:rerun-if-changed={}", model.display());
    println!("cargo:rerun-if-env-changed=VALOX_NO_NNUE");
    if model.is_file() && std::env::var_os("VALOX_NO_NNUE").is_none() {
        println!("cargo:rustc-cfg=valox_nnue_weights");
    } else {
        println!("cargo:warning=NNUE weights not found; using the classical evaluator");
    }
}
