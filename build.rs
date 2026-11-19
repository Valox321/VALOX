use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(valox_nnue_weights)");
    println!("cargo:rerun-if-env-changed=VALOX_NO_NNUE");
    println!("cargo:rerun-if-env-changed=VALOX_NNUE_PATH");

    // Release builds inject the network from outside the repository so that the
    // weights never have to be tracked or included in the source archive.
    let model = std::env::var_os("VALOX_NNUE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("networks/v60-7f587dfb.nnue"));
    println!("cargo:rerun-if-changed={}", model.display());

    if model.is_file() && std::env::var_os("VALOX_NO_NNUE").is_none() {
        let model = std::fs::canonicalize(&model).unwrap_or(model);
        let path = model.to_string_lossy().replace('\\', "/");
        let path = path.replace('"', "\\\"");
        let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
        std::fs::write(
            out_dir.join("nnue_model.rs"),
            format!("include_bytes!(\"{path}\")\n"),
        )
        .expect("write generated NNUE include");
        println!("cargo:rustc-cfg=valox_nnue_weights");
    } else {
        println!("cargo:warning=NNUE weights not found; using the classical evaluator");
    }
}
