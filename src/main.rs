use engram_core::BrainFile;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("create") => {
            let path = args.get(2).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("knowledge.brain"));
            match BrainFile::create(&path) {
                Ok(_) => println!("Created: {}", path.display()),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        Some("stats") => {
            let path = args.get(2).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("knowledge.brain"));
            match BrainFile::open(&path) {
                Ok(brain) => {
                    let (nodes, edges) = brain.stats();
                    println!("Nodes: {nodes}");
                    println!("Edges: {edges}");
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        _ => {
            println!("engram v0.1.0 — AI Memory Engine");
            println!();
            println!("Usage:");
            println!("  engram create [path]   Create a new .brain file");
            println!("  engram stats [path]    Show .brain file statistics");
        }
    }
}
