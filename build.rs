fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_client(false) // provider is a server only
        .compile_protos(&["./proto/provider.proto"], &["./proto"])?;
    Ok(())
}
