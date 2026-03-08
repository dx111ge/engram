fn main() {
    #[cfg(feature = "grpc")]
    {
        tonic_build::configure()
            .build_server(true)
            .build_client(false)
            .compile_protos(&["proto/engram.proto"], &["proto"])
            .expect("Failed to compile engram.proto");
    }
}
