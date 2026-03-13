#[cfg(feature = "onnx")]
mod onnx_tests {
    use engram_core::index::embedding::Embedder;
    use engram_core::index::embed_onnx::OnnxEmbedder;
    use std::path::{Path, PathBuf};

    /// Resolve ~/.engram/models/embed/ directory.
    fn embed_model_dir() -> Option<PathBuf> {
        let home = std::env::var_os("ENGRAM_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .map(|h| PathBuf::from(h).join(".engram"))
            })?;
        Some(home.join("models").join("embed"))
    }

    /// Find the first installed ONNX embedding model.
    fn find_model() -> Option<(PathBuf, PathBuf)> {
        let dir = embed_model_dir()?;
        for entry in std::fs::read_dir(&dir).ok()?.flatten() {
            let p = entry.path();
            let mp = p.join("model.onnx");
            let tp = p.join("tokenizer.json");
            if mp.exists() && tp.exists() {
                return Some((mp, tp));
            }
        }
        None
    }

    #[test]
    fn test_onnx_embedder_load_and_embed() {
        let (model_path, tokenizer_path) = match find_model() {
            Some(paths) => paths,
            None => {
                eprintln!("Skipping ONNX test: no model in ~/.engram/models/embed/");
                return;
            }
        };

        let embedder = OnnxEmbedder::load(&model_path, &tokenizer_path)
            .expect("Failed to load ONNX embedder");

        println!("Model: {}", embedder.model_id());
        println!("Dimension: {}", embedder.dim());

        // Test basic embedding
        let vec1 = embedder.embed("Rust is a systems programming language").unwrap();
        assert_eq!(vec1.len(), embedder.dim());
        println!("Embedding length: {}", vec1.len());

        // Verify L2 normalization
        let norm: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01, "Expected unit norm, got {norm}");

        // Test semantic similarity
        let vec2 = embedder.embed("Rust programming language for systems").unwrap();
        let vec3 = embedder.embed("The weather is sunny today").unwrap();

        let sim_12: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
        let sim_13: f32 = vec1.iter().zip(vec3.iter()).map(|(a, b)| a * b).sum();

        println!("Similarity (Rust/Rust): {sim_12:.4}");
        println!("Similarity (Rust/Weather): {sim_13:.4}");

        assert!(sim_12 > sim_13, "Similar sentences should have higher cosine similarity");
        println!("ONNX embedder test PASSED");
    }

    #[test]
    fn test_onnx_from_brain_path() {
        // from_brain_path now ignores the path argument and checks ~/.engram/models/embed/
        let brain_path = Path::new("unused.brain");
        if let Some(embedder) = OnnxEmbedder::from_brain_path(brain_path) {
            let vec = embedder.embed("hello world").unwrap();
            assert!(!vec.is_empty());
            println!("from_brain_path: {}D embeddings", vec.len());
        } else {
            eprintln!("Skipping from_brain_path test: no model in ~/.engram/models/embed/");
        }
    }
}
