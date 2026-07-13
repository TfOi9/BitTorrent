use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from("src/generated");
    tonic_build::configure()
        .out_dir(&out_dir)
        .compile_protos(&["proto/dht.proto"], &["proto"])?;
    Ok(())
}