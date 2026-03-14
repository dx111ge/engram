//! Integration test for GLiNER2 in-process NER + RE backend.
//! Requires the model at ~/.engram/models/gliner2/gliner2-multi-v1/

#[cfg(feature = "gliner2")]
mod gliner2_tests {
    use engram_ingest::gliner2_backend::Gliner2Backend;
    use std::path::PathBuf;

    fn model_dir() -> PathBuf {
        let home = if cfg!(windows) {
            std::env::var("USERPROFILE").unwrap_or_default()
        } else {
            std::env::var("HOME").unwrap_or_default()
        };
        PathBuf::from(home)
            .join(".engram")
            .join("models")
            .join("gliner2")
            .join("gliner2-multi-v1")
    }

    fn skip_if_no_model() -> bool {
        !model_dir().join("gliner2_config.json").exists()
    }

    #[test]
    fn test_load_int8() {
        if skip_if_no_model() {
            eprintln!("SKIP: GLiNER2 model not installed");
            return;
        }
        let backend = Gliner2Backend::load(&model_dir(), "int8");
        assert!(backend.is_ok(), "Failed to load: {:?}", backend.err());
    }

    // ---------------------------------------------------------------
    // NER tests
    // ---------------------------------------------------------------

    #[test]
    fn test_ner_english() {
        if skip_if_no_model() {
            return;
        }
        let mut backend = Gliner2Backend::load(&model_dir(), "fp16").unwrap();
        let entities = backend
            .extract_entities(
                "Bill Gates is an American businessman who co-founded Microsoft.",
                &["person", "company"],
                0.3,
            )
            .unwrap();

        println!("NER English:");
        for e in &entities {
            println!("  {:20} | {:10} | {:.1}%", e.text, e.label, e.score * 100.0);
        }

        assert!(entities.iter().any(|e| e.text.contains("Bill") && e.label == "person"));
        assert!(entities.iter().any(|e| e.text.contains("Microsoft") && e.label == "company"));
    }

    #[test]
    fn test_ner_german() {
        if skip_if_no_model() {
            return;
        }
        let mut backend = Gliner2Backend::load(&model_dir(), "fp16").unwrap();
        let entities = backend
            .extract_entities(
                "Tim Cook ist der CEO von Apple. Apple hat seinen Hauptsitz in Cupertino.",
                &["person", "company", "city"],
                0.3,
            )
            .unwrap();

        println!("NER German:");
        for e in &entities {
            println!("  {:20} | {:10} | {:.1}%", e.text, e.label, e.score * 100.0);
        }

        assert!(entities.iter().any(|e| e.text.contains("Tim") && e.label == "person"));
        assert!(entities.iter().any(|e| e.text.contains("Apple") && e.label == "company"));
    }

    // ---------------------------------------------------------------
    // RE tests
    // ---------------------------------------------------------------

    #[test]
    fn test_re_english() {
        if skip_if_no_model() {
            return;
        }
        let mut backend = Gliner2Backend::load(&model_dir(), "fp16").unwrap();
        let relations = backend
            .extract_relations(
                "Bill Gates is an American businessman who co-founded Microsoft.",
                &["founded"],
                0.3,
            )
            .unwrap();

        println!("RE English:");
        for r in &relations {
            println!(
                "  {:20} --[{:15}]--> {:20} | h:{:.1}% t:{:.1}%",
                r.head, r.label, r.tail,
                r.head_score * 100.0,
                r.tail_score * 100.0,
            );
        }

        assert!(!relations.is_empty(), "Should find at least one relation");
        let founded = relations.iter().find(|r| r.label == "founded");
        assert!(founded.is_some(), "Should find 'founded' relation");
        let f = founded.unwrap();
        assert!(
            f.head.contains("Bill") || f.head.contains("Gates"),
            "Head should be Bill Gates, got: {}",
            f.head
        );
        assert!(
            f.tail.contains("Microsoft"),
            "Tail should be Microsoft, got: {}",
            f.tail
        );
    }

    #[test]
    fn test_re_german() {
        if skip_if_no_model() {
            return;
        }
        let mut backend = Gliner2Backend::load(&model_dir(), "fp16").unwrap();
        let relations = backend
            .extract_relations(
                "Tim Cook ist der CEO von Apple. Apple hat seinen Hauptsitz in Cupertino.",
                &["works_at", "headquartered_in"],
                0.3,
            )
            .unwrap();

        println!("RE German:");
        for r in &relations {
            println!(
                "  {:20} --[{:20}]--> {:20} | h:{:.1}% t:{:.1}%",
                r.head, r.label, r.tail,
                r.head_score * 100.0,
                r.tail_score * 100.0,
            );
        }

        // Should find works_at and headquartered_in
        assert!(
            relations.iter().any(|r| r.label == "works_at"),
            "Should find 'works_at' relation"
        );
        assert!(
            relations.iter().any(|r| r.label == "headquartered_in"),
            "Should find 'headquartered_in' relation"
        );
    }

    #[test]
    fn test_re_german_complex() {
        if skip_if_no_model() {
            return;
        }
        let mut backend = Gliner2Backend::load(&model_dir(), "fp16").unwrap();
        let relations = backend
            .extract_relations(
                "Putin und Zelensky verhandeln ueber den Konflikt in der Ukraine. NATO unterstuetzt die Ukraine mit HIMARS.",
                &["supports"],
                0.3,
            )
            .unwrap();

        println!("RE German complex:");
        for r in &relations {
            println!(
                "  {:20} --[{:15}]--> {:20} | h:{:.1}% t:{:.1}%",
                r.head, r.label, r.tail,
                r.head_score * 100.0,
                r.tail_score * 100.0,
            );
        }

        assert!(
            relations.iter().any(|r| r.label == "supports" && r.head.contains("NATO")),
            "Should find NATO supports Ukraine"
        );
    }

    // ---------------------------------------------------------------
    // Multilingual tests
    // ---------------------------------------------------------------

    #[test]
    fn test_ner_re_french() {
        if skip_if_no_model() {
            return;
        }
        let mut backend = Gliner2Backend::load(&model_dir(), "fp16").unwrap();
        let (entities, relations) = backend
            .extract_all(
                "Emmanuel Macron est le president de la France. Il travaille a l'Elysee.",
                &["person", "country", "building"],
                &["leads", "works_at"],
                0.3,
                0.3,
            )
            .unwrap();

        println!("French NER+RE:");
        for e in &entities {
            println!("  NER: {:25} | {:10} | {:.1}%", e.text, e.label, e.score * 100.0);
        }
        for r in &relations {
            println!("  RE:  {:25} --[{:10}]--> {:20} | h:{:.1}% t:{:.1}%",
                r.head, r.label, r.tail, r.head_score * 100.0, r.tail_score * 100.0);
        }

        assert!(entities.iter().any(|e| e.text.contains("Macron") && e.label == "person"));
        assert!(entities.iter().any(|e| e.text.contains("France") && e.label == "country"));
    }

    #[test]
    fn test_ner_re_spanish() {
        if skip_if_no_model() {
            return;
        }
        let mut backend = Gliner2Backend::load(&model_dir(), "fp16").unwrap();
        let (entities, relations) = backend
            .extract_all(
                "Elon Musk es el CEO de Tesla. Tesla tiene su sede en Austin.",
                &["person", "company", "city"],
                &["works_at", "headquartered_in"],
                0.3,
                0.3,
            )
            .unwrap();

        println!("Spanish NER+RE:");
        for e in &entities {
            println!("  NER: {:25} | {:10} | {:.1}%", e.text, e.label, e.score * 100.0);
        }
        for r in &relations {
            println!("  RE:  {:25} --[{:20}]--> {:20}", r.head, r.label, r.tail);
        }

        assert!(entities.iter().any(|e| e.text.contains("Musk") && e.label == "person"));
        assert!(entities.iter().any(|e| e.text.contains("Tesla") && e.label == "company"));
    }

    #[test]
    fn test_ner_re_mixed_lang() {
        if skip_if_no_model() {
            return;
        }
        let mut backend = Gliner2Backend::load(&model_dir(), "fp16").unwrap();
        // Real-world intelligence text mixing German/English
        let (entities, relations) = backend
            .extract_all(
                "Die Bundeswehr bestellt IRIS-T Systeme von Diehl Defence in Ueberlingen.",
                &["organization", "product", "city"],
                &["produces", "located_in"],
                0.3,
                0.3,
            )
            .unwrap();

        println!("Mixed-lang NER+RE:");
        for e in &entities {
            println!("  NER: {:25} | {:12} | {:.1}%", e.text, e.label, e.score * 100.0);
        }
        for r in &relations {
            println!("  RE:  {:25} --[{:12}]--> {:20}", r.head, r.label, r.tail);
        }

        assert!(entities.iter().any(|e| e.text.contains("Bundeswehr")));
    }

    // ---------------------------------------------------------------
    // Combined NER + RE
    // ---------------------------------------------------------------

    #[test]
    fn test_extract_all() {
        if skip_if_no_model() {
            return;
        }
        let mut backend = Gliner2Backend::load(&model_dir(), "fp16").unwrap();
        let (entities, relations) = backend
            .extract_all(
                "Angela Merkel war Bundeskanzlerin von Deutschland.",
                &["person", "country"],
                &["leads"],
                0.3,
                0.3,
            )
            .unwrap();

        println!("Combined NER+RE:");
        println!("  Entities:");
        for e in &entities {
            println!("    {:20} | {:10} | {:.1}%", e.text, e.label, e.score * 100.0);
        }
        println!("  Relations:");
        for r in &relations {
            println!(
                "    {:20} --[{:10}]--> {:20}",
                r.head, r.label, r.tail
            );
        }

        assert!(entities.iter().any(|e| e.text.contains("Merkel")));
        assert!(entities.iter().any(|e| e.text.contains("Deutschland")));
        assert!(!relations.is_empty(), "Should find 'leads' relation");
    }
}
