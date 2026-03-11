use engram_core::graph::{Graph, Provenance};
use engram_core::Embedder;
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
        Some("serve") => cmd_serve(&args),
        Some("mcp") => cmd_mcp(&args),
        Some("reindex") => cmd_reindex(&args),
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
        let display_label = g.label_for_id(node.id).unwrap_or_else(|_| node.label().to_string());
        println!("Node: {}", display_label);
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

fn cmd_serve(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = default_path(args, 2);
    let addr = args.get(3).map(|s| s.as_str()).unwrap_or("0.0.0.0:3030");

    // Detect hardware and print compute capabilities
    let hw = engram_compute::planner::HardwareInfo::detect();
    println!("Compute backends:");
    print!("  CPU: {} cores", hw.cpu_cores);
    if hw.has_avx2 { print!(", AVX2+FMA"); }
    if hw.has_neon { print!(", NEON"); }
    println!();
    if hw.has_gpu {
        println!("  GPU: {} ({})", hw.gpu_name, hw.gpu_backend);
    } else {
        println!("  GPU: none detected");
    }
    if hw.has_npu {
        println!("  NPU: {}", hw.npu_name);
    } else {
        println!("  NPU: none detected");
    }
    for npu in &hw.dedicated_npu {
        println!("  NPU hw: {npu}");
    }

    let mut g = if path.exists() {
        Graph::open(&path)?
    } else {
        Graph::create(&path)?
    };

    // Wire the compute planner into the graph for GPU/NPU-accelerated search
    let planner = engram_compute::planner::ComputePlanner::new();
    g.set_compute_planner(planner);

    // Auto-configure embedder: config file takes precedence over env vars
    let mut state = engram_api::state::AppState::new(g);

    // Load config sidecar if it exists
    let config_path = {
        let mut p = path.as_os_str().to_owned();
        p.push(".config");
        PathBuf::from(p)
    };
    state.load_config(config_path);

    // Determine embedder settings: config file > env vars
    let (embed_endpoint, embed_model) = {
        let cfg = state.config.read().unwrap();
        let ep = cfg.embed_endpoint.clone()
            .or_else(|| std::env::var("ENGRAM_EMBED_ENDPOINT").ok());
        let model = cfg.embed_model.clone()
            .or_else(|| std::env::var("ENGRAM_EMBED_MODEL").ok());
        (ep, model)
    };

    if let Some(ref endpoint) = embed_endpoint {
        let model_name = embed_model.clone()
            .unwrap_or_else(|| "multilingual-e5-small".into());
        // Create embedder with dimension probing
        let probe = engram_core::ApiEmbedder::new(
            endpoint.clone(), model_name.clone(), 0, None,
        );
        match probe.probe_dimension() {
            Ok(dim) => {
                let embedder = engram_core::ApiEmbedder::new(
                    endpoint.clone(), model_name.clone(), dim, None,
                );
                println!("Embedder: {} ({}D) via {}", model_name, dim, endpoint);
                state.set_embedder_info(model_name, dim, endpoint.clone());
                state.graph.write().unwrap().set_embedder(Box::new(embedder));
            }
            Err(e) => {
                // Fall back to from_env() which has its own error handling
                println!("Embedder probe failed ({}), trying env fallback...", e);
                if let Some(embedder) = engram_core::ApiEmbedder::from_env() {
                    let model = embedder.model_id().to_string();
                    let dim = embedder.dim();
                    let ep = std::env::var("ENGRAM_EMBED_ENDPOINT").unwrap_or_default();
                    println!("Embedder: {} ({}D, env fallback) via {}", model, dim, ep);
                    state.set_embedder_info(model, dim, ep);
                    state.graph.write().unwrap().set_embedder(Box::new(embedder));
                } else {
                    println!("Embedder: none (probe failed, no env fallback)");
                }
            }
        }
    } else if let Some(embedder) = engram_core::ApiEmbedder::from_env() {
        let model = embedder.model_id().to_string();
        let dim = embedder.dim();
        let endpoint = std::env::var("ENGRAM_EMBED_ENDPOINT").unwrap_or_default();
        println!("Embedder: {} ({}D, auto-detected) via {}", model, dim, endpoint);
        state.set_embedder_info(model, dim, endpoint);
        state.graph.write().unwrap().set_embedder(Box::new(embedder));
    } else {
        // Try to auto-load ONNX model from sidecar files
        let mut onnx_loaded = false;
        #[cfg(feature = "onnx")]
        {
            let model_path = format!("{}.model.onnx", path.display());
            let tokenizer_path = format!("{}.tokenizer.json", path.display());
            if std::path::Path::new(&model_path).exists() && std::path::Path::new(&tokenizer_path).exists() {
                match engram_core::OnnxEmbedder::load(
                    std::path::Path::new(&model_path),
                    std::path::Path::new(&tokenizer_path),
                ) {
                    Ok(embedder) => {
                        let dim = embedder.dim();
                        println!("Embedder: ONNX local ({}D)", dim);
                        state.set_embedder_info("ONNX Local".into(), dim, "local".into());
                        state.graph.write().unwrap().set_embedder(Box::new(embedder));
                        onnx_loaded = true;
                    }
                    Err(e) => {
                        println!("Embedder: ONNX files found but failed to load: {e}");
                    }
                }
            }
        }
        if !onnx_loaded {
            println!("Embedder: none (set ENGRAM_EMBED_ENDPOINT or use POST /config)");
        }
    }

    // Load assessment sidecar (if assess feature enabled)
    #[cfg(feature = "assess")]
    {
        let assess_path = {
            let mut p = path.as_os_str().to_owned();
            p.push(".assessments");
            PathBuf::from(p)
        };
        state.load_assessments(assess_path);
        let count = state.assessments.read().map(|s| s.len()).unwrap_or(0);
        if count > 0 {
            println!("Assessments: {} loaded", count);
        }
    }

    // Load action rules sidecar (if actions feature enabled)
    #[cfg(feature = "actions")]
    {
        let rules_path = {
            let mut p = path.as_os_str().to_owned();
            p.push(".rules");
            PathBuf::from(p)
        };
        state.action_rules_path = Some(rules_path);
        state.load_action_rules_from_file();
    }

    // Load scheduler sidecar (if ingest feature enabled)
    #[cfg(feature = "ingest")]
    {
        let schedules_path = {
            let mut p = path.as_os_str().to_owned();
            p.push(".schedules");
            PathBuf::from(p)
        };
        state.load_schedules(schedules_path);
    }

    // Auto-enable mesh if configured
    #[cfg(feature = "mesh")]
    {
        let mesh_enabled = state.config.read().map(|c| c.mesh_enabled.unwrap_or(false)).unwrap_or(false);
        if mesh_enabled {
            let identity_path = {
                let mut p = path.as_os_str().to_owned();
                p.push(".identity");
                PathBuf::from(p)
            };
            let keypair = engram_mesh::identity::Keypair::load_or_generate(&identity_path)
                .unwrap_or_else(|e| {
                    eprintln!("Warning: failed to load/generate mesh identity: {e}, generating ephemeral");
                    engram_mesh::identity::Keypair::generate()
                });

            let peers_path = {
                let mut p = path.as_os_str().to_owned();
                p.push(".peers");
                Some(PathBuf::from(p))
            };
            let audit_path = {
                let mut p = path.as_os_str().to_owned();
                p.push(".audit");
                Some(PathBuf::from(p))
            };

            state.enable_mesh(keypair, peers_path, audit_path);
            let peer_count = state.mesh.as_ref().map(|m| m.peers.read().map(|p| p.peers.len()).unwrap_or(0)).unwrap_or(0);
            println!("Mesh: enabled ({} peers)", peer_count);
        }
    }

    // Restore quantization mode from config (defaults to enabled)
    {
        let quant_enabled = state.config.read()
            .map(|c| c.quantization_enabled.unwrap_or(true))
            .unwrap_or(true);
        if quant_enabled {
            let mut g = state.graph.write().unwrap();
            g.set_vector_quantization(engram_core::QuantizationMode::Int8);
            println!("Quantization: int8 (4x memory reduction)");
        } else {
            println!("Quantization: off");
        }
    }

    // Set secrets path for deferred unlock (admin login decrypts secrets)
    let secrets_path = {
        let mut p = path.as_os_str().to_owned();
        p.push(".secrets");
        PathBuf::from(p)
    };
    state.secrets_path = Some(secrets_path.clone());
    if secrets_path.exists() {
        println!("Secrets: encrypted (will unlock on admin login)");
    } else {
        println!("Secrets: none (will create on admin setup)");
    }

    // Load user store from sidecar
    let users_path = {
        let mut p = path.as_os_str().to_owned();
        p.push(".users");
        PathBuf::from(p)
    };
    {
        let store = engram_api::auth::UserStore::load(&users_path);
        let count = store.len();
        *state.user_store.write().unwrap() = store;
        if count > 0 {
            println!("Users: {count} loaded");
        } else {
            println!("Users: none (setup required via frontend)");
        }
    }

    let grpc_addr = args.get(4).map(|s| s.as_str()).unwrap_or("0.0.0.0:50051");
    println!("HTTP: {addr}");
    println!("gRPC: {grpc_addr}");

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let grpc_state = state.clone();
        let grpc_addr_owned = grpc_addr.to_string();
        tokio::spawn(async move {
            if let Err(e) = engram_api::grpc::serve_grpc(grpc_state, &grpc_addr_owned).await {
                eprintln!("gRPC server error: {e}");
            }
        });

        // Detect frontend directory next to the executable
        let frontend_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("frontend")))
            .filter(|p| p.join("index.html").exists())
            .or_else(|| {
                // Fallback: check relative to working directory
                let p = std::path::PathBuf::from("frontend");
                if p.join("index.html").exists() { Some(p) } else { None }
            });
        if let Some(ref dir) = frontend_dir {
            println!("Frontend: {}", dir.display());
        } else {
            println!("Frontend: not found (place frontend/ next to the binary)");
        }
        let frontend_str = frontend_dir.as_ref().map(|p| p.to_string_lossy().to_string());
        engram_api::server::serve_with_frontend(state, addr, frontend_str.as_deref()).await
    })?;

    Ok(())
}

fn cmd_mcp(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = default_path(args, 2);

    let g = if path.exists() {
        Graph::open(&path)?
    } else {
        Graph::create(&path)?
    };

    let state = engram_api::state::AppState::new(g);
    engram_api::mcp::run_stdio(state);
    Ok(())
}

fn cmd_reindex(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = default_path(args, 2);
    let mut g = Graph::open(&path)?;

    if let Some(embedder) = engram_core::ApiEmbedder::from_env() {
        println!("Using embedder: {} ({}D)", embedder.model_id(), embedder.dim());
        g.set_embedder(Box::new(embedder));
    } else {
        return Err("ENGRAM_EMBED_ENDPOINT must be set for reindex".into());
    }

    let count = g.reindex()?;
    g.checkpoint()?;
    println!("Re-embedded {count} nodes");
    Ok(())
}

fn print_usage() {
    println!("engram v1.1.0 -- AI Memory Engine");
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
    println!("  engram serve [path] [addr] [grpc_addr]         Start HTTP+gRPC server (default: 0.0.0.0:3030, gRPC: 50051)");
    println!("  engram mcp [path]                             Start MCP server (JSON-RPC over stdio)");
    println!("  engram reindex [path]                         Re-embed all nodes (after model change)");
    println!();
    println!("Search syntax:");
    println!("  engram search \"postgresql\"                    Full-text search");
    println!("  engram search \"confidence>0.8\"                Filter by confidence");
    println!("  engram search \"prop:role=database\"            Filter by property");
    println!("  engram search \"tier:active\"                   Filter by memory tier");
    println!("  engram search \"type:server AND confidence>0.5\" Boolean queries");
}
