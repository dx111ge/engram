use engram_core::graph::{Graph, Provenance};
use std::path::PathBuf;

fn default_path(args: &[String], idx: usize) -> PathBuf {
    args.get(idx)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("knowledge.brain"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str());

    let result = match cmd {
        Some("create") => cmd_create(&args),
        Some("stats") => cmd_stats(&args),
        Some("store") => cmd_store(&args),
        Some("set") => cmd_set_property(&args),
        Some("relate") => cmd_relate(&args),
        Some("query") => cmd_query(&args),
        Some("search") => cmd_search(&args),
        Some("delete") => cmd_delete(&args),
        _ => {
            print_usage();
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn cmd_create(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = default_path(args, 2);
    Graph::create(&path)?;
    println!("Created: {}", path.display());
    Ok(())
}

fn cmd_stats(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = default_path(args, 2);
    let g = Graph::open(&path)?;
    let (nodes, edges) = g.stats();
    println!("Nodes: {nodes}");
    println!("Edges: {edges}");
    Ok(())
}

fn cmd_store(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let label = args.get(2).ok_or("Usage: engram store <label> [path]")?;
    let path = default_path(args, 3);
    let prov = Provenance::user("cli");

    let mut g = if path.exists() {
        Graph::open(&path)?
    } else {
        Graph::create(&path)?
    };

    let id = g.store(label, &prov)?;
    g.checkpoint()?;
    println!("Stored node '{label}' (id: {id})");
    Ok(())
}

fn cmd_relate(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // engram relate <from> <relationship> <to> [path]
    let from = args.get(2).ok_or("Usage: engram relate <from> <relationship> <to> [path]")?;
    let rel = args.get(3).ok_or("Usage: engram relate <from> <relationship> <to> [path]")?;
    let to = args.get(4).ok_or("Usage: engram relate <from> <relationship> <to> [path]")?;
    let path = default_path(args, 5);
    let prov = Provenance::user("cli");

    let mut g = Graph::open(&path)?;
    let id = g.relate(from, to, rel, &prov)?;
    g.checkpoint()?;
    println!("{from} -[{rel}]-> {to} (edge id: {id})");
    Ok(())
}

fn cmd_query(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // engram query <label> [depth] [path]
    let label = args.get(2).ok_or("Usage: engram query <label> [depth] [path]")?;
    let depth: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let path = default_path(args, 4);

    let g = Graph::open(&path)?;

    // Show node details
    if let Some(node) = g.get_node(label)? {
        println!("Node: {}", node.label());
        println!("  id: {}", node.id);
        println!("  confidence: {:.2}", node.confidence);
        println!("  memory_tier: {}", node.memory_tier);
    } else {
        println!("Node '{label}' not found");
        return Ok(());
    }

    // Show properties
    if let Some(props) = g.get_properties(label)? {
        if !props.is_empty() {
            println!("Properties:");
            let mut keys: Vec<_> = props.keys().collect();
            keys.sort();
            for key in keys {
                println!("  {key}: {}", props[key]);
            }
        }
    }

    // Show outgoing edges
    let edges = g.edges_from(label)?;
    if !edges.is_empty() {
        println!("Edges out:");
        for e in &edges {
            println!("  {e}");
        }
    }

    // Show incoming edges
    let edges_in = g.edges_to(label)?;
    if !edges_in.is_empty() {
        println!("Edges in:");
        for e in &edges_in {
            println!("  {e}");
        }
    }

    // Traversal
    if depth > 0 {
        let result = g.traverse(label, depth, 0.0)?;
        if result.nodes.len() > 1 {
            println!("Reachable ({depth}-hop): {} nodes", result.nodes.len());
        }
    }

    Ok(())
}

fn cmd_set_property(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // engram set <label> <key> <value> [path]
    let label = args.get(2).ok_or("Usage: engram set <label> <key> <value> [path]")?;
    let key = args.get(3).ok_or("Usage: engram set <label> <key> <value> [path]")?;
    let value = args.get(4).ok_or("Usage: engram set <label> <key> <value> [path]")?;
    let path = default_path(args, 5);
    let mut g = Graph::open(&path)?;

    if g.set_property(label, key, value)? {
        g.checkpoint()?;
        println!("{label}.{key} = {value}");
    } else {
        println!("Node '{label}' not found");
    }
    Ok(())
}

fn cmd_search(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // engram search <query> [path]
    let query = args.get(2).ok_or("Usage: engram search <query> [path]")?;
    let path = default_path(args, 3);

    let g = Graph::open(&path)?;
    let results = g.search(query, 20).map_err(|e| e)?;

    if results.is_empty() {
        println!("No results");
    } else {
        println!("Results ({}):", results.len());
        for r in &results {
            println!("  {r}");
        }
    }
    Ok(())
}

fn cmd_delete(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let label = args.get(2).ok_or("Usage: engram delete <label> [path]")?;
    let path = default_path(args, 3);
    let prov = Provenance::user("cli");

    let mut g = Graph::open(&path)?;
    if g.delete(label, &prov)? {
        g.checkpoint()?;
        println!("Deleted: {label}");
    } else {
        println!("Node '{label}' not found");
    }
    Ok(())
}

fn print_usage() {
    println!("engram v0.1.0 — AI Memory Engine");
    println!();
    println!("Usage:");
    println!("  engram create [path]                          Create a new .brain file");
    println!("  engram stats [path]                           Show statistics");
    println!("  engram store <label> [path]                   Store a node");
    println!("  engram set <label> <key> <value> [path]       Set a property");
    println!("  engram relate <from> <rel> <to> [path]        Create a relationship");
    println!("  engram query <label> [depth] [path]           Query a node and its edges");
    println!("  engram search <query> [path]                  Search (BM25, filters, boolean)");
    println!("  engram delete <label> [path]                  Soft-delete a node");
    println!();
    println!("Search syntax:");
    println!("  engram search \"postgresql\"                    Full-text search");
    println!("  engram search \"confidence>0.8\"                Filter by confidence");
    println!("  engram search \"prop:role=database\"            Filter by property");
    println!("  engram search \"tier:active\"                   Filter by memory tier");
    println!("  engram search \"type:server AND confidence>0.5\" Boolean queries");
}
