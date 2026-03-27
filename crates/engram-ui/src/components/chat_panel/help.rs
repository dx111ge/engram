//! /help command handler -- local help text generation, no LLM needed.

/// All slash commands available for autocomplete.
/// Each entry: (command, icon class, short description).
pub const SLASH_COMMANDS: &[(&str, &str, &str)] = &[
    ("/help", "fa-solid fa-circle-question", "Show available commands"),
    ("/help knowledge", "fa-solid fa-database", "Knowledge tools"),
    ("/help analysis", "fa-solid fa-chart-line", "Analysis tools"),
    ("/help temporal", "fa-solid fa-clock", "Temporal tools"),
    ("/help investigation", "fa-solid fa-magnifying-glass", "Investigation tools"),
    ("/help assessment", "fa-solid fa-scale-balanced", "Assessment tools"),
    ("/help reasoning", "fa-solid fa-code-branch", "Reasoning tools"),
    ("/help actions", "fa-solid fa-gears", "Action tools"),
    ("/help reporting", "fa-solid fa-file-lines", "Reporting tools"),
    ("/path", "fa-solid fa-route", "Find paths between entities"),
];

/// Generate a plain-text help response (kept for LLM message history).
pub fn generate_help_response(input: &str) -> String {
    let arg = input.trim().strip_prefix("/help").unwrap_or("").trim().to_lowercase();

    match arg.as_str() {
        "" => help_overview(),
        "knowledge" => help_knowledge(),
        "analysis" => help_analysis(),
        "temporal" => help_temporal(),
        "investigation" => help_investigation(),
        "assessment" => help_assessment(),
        "reasoning" => help_reasoning(),
        "actions" => help_actions(),
        "reporting" => help_reporting(),
        other => format!(
            "Unknown help category: \"{}\"\n\n\
             Available categories: knowledge, analysis, temporal, investigation,\n\
             assessment, reasoning, actions, reporting\n\n\
             Type /help for the full overview.",
            other
        ),
    }
}

/// Generate styled HTML help response for rich display in chat.
pub fn generate_help_html(input: &str) -> String {
    let arg = input.trim().strip_prefix("/help").unwrap_or("").trim().to_lowercase();

    match arg.as_str() {
        "" => help_overview_html(),
        "knowledge" => help_category_html("Knowledge Tools", "fa-solid fa-database", &KNOWLEDGE_TOOLS),
        "analysis" => help_category_html("Analysis Tools", "fa-solid fa-chart-line", &ANALYSIS_TOOLS),
        "temporal" => help_category_html("Temporal Tools", "fa-solid fa-clock", &TEMPORAL_TOOLS),
        "investigation" => help_category_html("Investigation Tools", "fa-solid fa-magnifying-glass", &INVESTIGATION_TOOLS),
        "assessment" => help_category_html("Assessment Tools", "fa-solid fa-scale-balanced", &ASSESSMENT_TOOLS),
        "reasoning" => help_category_html("Reasoning Tools", "fa-solid fa-code-branch", &REASONING_TOOLS),
        "actions" => help_category_html("Action Tools", "fa-solid fa-gears", &ACTION_TOOLS),
        "reporting" => help_category_html("Reporting Tools", "fa-solid fa-file-lines", &REPORTING_TOOLS),
        _ => format!(
            "<div class=\"chat-card\"><div class=\"chat-card-header\">\
             <i class=\"fa-solid fa-circle-exclamation\"></i> Unknown category</div>\
             <p style=\"padding:0.5rem;\">Available: knowledge, analysis, temporal, investigation, \
             assessment, reasoning, actions, reporting</p></div>"
        ),
    }
}

// ── Tool definitions for HTML rendering ──

struct HelpTool {
    name: &'static str,
    description: &'static str,
    example: &'static str,
    is_write: bool,
}

const KNOWLEDGE_TOOLS: [HelpTool; 10] = [
    HelpTool { name: "query", description: "Traverse graph from entity", example: "Show me connections of Putin", is_write: false },
    HelpTool { name: "search", description: "Full-text keyword search", example: "Search for Ukraine", is_write: false },
    HelpTool { name: "explain", description: "Provenance, confidence, edges", example: "Explain Lavrov", is_write: false },
    HelpTool { name: "similar", description: "Semantic similarity search", example: "Find entities similar to NATO", is_write: false },
    HelpTool { name: "store", description: "Add new entity", example: "Store that Berlin is a city", is_write: true },
    HelpTool { name: "relate", description: "Create relationship", example: "Connect Putin to Russia as president", is_write: true },
    HelpTool { name: "reinforce", description: "Boost confidence", example: "Reinforce the claim about sanctions", is_write: true },
    HelpTool { name: "correct", description: "Mark as wrong", example: "Correct the outdated entry for...", is_write: true },
    HelpTool { name: "delete", description: "Soft-delete entity", example: "Delete the duplicate entry for...", is_write: true },
    HelpTool { name: "prove", description: "Find evidence for a claim", example: "Prove that X is connected to Y", is_write: false },
];

const ANALYSIS_TOOLS: [HelpTool; 4] = [
    HelpTool { name: "compare", description: "Compare two entities side-by-side", example: "Compare NATO and CSTO", is_write: false },
    HelpTool { name: "shortest_path", description: "Find shortest path between entities", example: "Shortest path from Putin to Biden", is_write: false },
    HelpTool { name: "most_connected", description: "Find most connected entities", example: "What are the most connected nodes?", is_write: false },
    HelpTool { name: "isolated", description: "Find entities with no/few connections", example: "Show me isolated entities", is_write: false },
];

const TEMPORAL_TOOLS: [HelpTool; 3] = [
    HelpTool { name: "timeline", description: "Show entity changes over time", example: "Timeline of Ukraine conflict", is_write: false },
    HelpTool { name: "date_query", description: "Query knowledge at a specific date", example: "What did I know about sanctions in January?", is_write: false },
    HelpTool { name: "current", description: "Show current state vs historical", example: "Current confidence for all entities about Iran", is_write: false },
];

const INVESTIGATION_TOOLS: [HelpTool; 5] = [
    HelpTool { name: "ingest", description: "Extract entities from text", example: "Ingest this article about...", is_write: true },
    HelpTool { name: "analyze", description: "Deep analysis of entity neighborhood", example: "Analyze the network around Lavrov", is_write: false },
    HelpTool { name: "investigate", description: "Full investigation with web search", example: "Investigate Wagner Group deeply", is_write: true },
    HelpTool { name: "changes", description: "Show recent graph changes", example: "What changed in the last 24 hours?", is_write: false },
    HelpTool { name: "watch", description: "Mark entity for monitoring", example: "Watch Prigozhin for changes", is_write: true },
];

const ASSESSMENT_TOOLS: [HelpTool; 6] = [
    HelpTool { name: "assess_create", description: "Create new hypothesis", example: "Create assessment: Will sanctions hold?", is_write: true },
    HelpTool { name: "assess_evidence", description: "Add evidence to assessment", example: "Add supporting evidence from...", is_write: true },
    HelpTool { name: "assess_evaluate", description: "Evaluate hypothesis confidence", example: "Evaluate my assessment on...", is_write: false },
    HelpTool { name: "assess_list", description: "List all active assessments", example: "Show my assessments", is_write: false },
    HelpTool { name: "assess_detail", description: "Show assessment with all evidence", example: "Detail on assessment about sanctions", is_write: false },
    HelpTool { name: "assess_compare", description: "Compare competing hypotheses", example: "Compare assessments about leadership change", is_write: false },
];

const REASONING_TOOLS: [HelpTool; 2] = [
    HelpTool { name: "what_if", description: "Simulate confidence changes and cascading effects", example: "What if Putin's health confidence drops to 20%?", is_write: false },
    HelpTool { name: "influence", description: "Find influence paths between entities", example: "How does China influence the Iran deal?", is_write: false },
];

const ACTION_TOOLS: [HelpTool; 4] = [
    HelpTool { name: "rule_create", description: "Create inference rule", example: "Create rule: if sanctions then economic impact", is_write: true },
    HelpTool { name: "rule_list", description: "List all active rules", example: "Show my rules", is_write: false },
    HelpTool { name: "rule_fire", description: "Manually trigger rule evaluation", example: "Fire rules on recent changes", is_write: false },
    HelpTool { name: "schedule", description: "View or create scheduled tasks", example: "What's scheduled for today?", is_write: false },
];

const REPORTING_TOOLS: [HelpTool; 3] = [
    HelpTool { name: "briefing", description: "Generate intelligence briefing", example: "Brief me on the current state of...", is_write: false },
    HelpTool { name: "export", description: "Export entities or subgraph", example: "Export all entities about Ukraine", is_write: false },
    HelpTool { name: "entity_timeline", description: "Show full history of an entity", example: "Show the complete timeline for Wagner Group", is_write: false },
];

// ── HTML generators ──

fn help_overview_html() -> String {
    let categories = [
        ("Knowledge", "fa-solid fa-database", "10", "Query, search, explain, store, relate...", "/help knowledge"),
        ("Analysis", "fa-solid fa-chart-line", "4", "Compare, shortest path, most connected, isolated", "/help analysis"),
        ("Temporal", "fa-solid fa-clock", "3", "Timeline, date queries, current state", "/help temporal"),
        ("Investigation", "fa-solid fa-magnifying-glass", "5", "Ingest, analyze, investigate, changes, watch", "/help investigation"),
        ("Assessment", "fa-solid fa-scale-balanced", "6", "Hypotheses, evidence, evaluate", "/help assessment"),
        ("Reasoning", "fa-solid fa-code-branch", "2", "What-if, influence paths", "/help reasoning"),
        ("Actions", "fa-solid fa-gears", "4", "Rules, inference, scheduling", "/help actions"),
        ("Reporting", "fa-solid fa-file-lines", "3", "Briefings, export, entity timeline", "/help reporting"),
    ];

    let mut html = "<div class=\"chat-help-grid\">".to_string();
    for (name, icon, count, desc, cmd) in &categories {
        html.push_str(&format!(
            "<div class=\"chat-help-category\" onclick=\"\
                window.dispatchEvent(new CustomEvent('engram-chat-send',{{detail:'{cmd}'}}));\">\
                <div class=\"chat-help-category-icon\"><i class=\"{icon}\"></i></div>\
                <div class=\"chat-help-category-name\">{name} <span class=\"chat-help-count\">({count})</span></div>\
                <div class=\"chat-help-category-desc\">{desc}</div>\
             </div>",
        ));
    }
    html.push_str("</div>");
    html.push_str("<p style=\"text-align:center;font-size:0.75rem;color:var(--text-muted);margin-top:0.5rem;\">Click a category for details. Just ask naturally -- no commands needed!</p>");
    html
}

fn help_category_html(title: &str, icon: &str, tools: &[HelpTool]) -> String {
    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"{icon}\"></i> {title}</div>\
            <div class=\"chat-card-body\">"
    );

    for tool in tools {
        let write_badge = if tool.is_write {
            " <span class=\"chat-help-write-badge\"><i class=\"fa-solid fa-pen\"></i> write</span>"
        } else {
            ""
        };
        html.push_str(&format!(
            "<div class=\"chat-help-tool\" onclick=\"\
                window.dispatchEvent(new CustomEvent('engram-tool-card',{{detail:'{name}'}}));\">\
                <div class=\"chat-help-tool-name\">{}{}</div>\
                <div class=\"chat-help-tool-desc\">{}</div>\
                <div class=\"chat-help-tool-example\">\"{}\"</div>\
             </div>",
            tool.name, write_badge, tool.description, tool.example,
            name = tool.name,
        ));
    }

    html.push_str("</div></div>");
    html
}

// ── Plain text help functions (kept for LLM message history) ──

fn help_overview() -> String {
    [
        "Knowledge Chat -- Available Commands",
        "",
        "  Knowledge (10)     query, search, explain, store, relate...",
        "  Analysis (4)       compare, shortest path, most connected, isolated",
        "  Temporal (3)       timeline, date queries, current state",
        "  Investigation (5)  ingest, analyze, investigate, changes, watch",
        "  Assessment (6)     hypotheses, evidence, evaluate",
        "  Reasoning (2)      what-if, influence paths",
        "  Actions (4)        rules, inference, scheduling",
        "  Reporting (3)      briefings, export, entity timeline",
        "",
        "Type /help <category> for details.",
        "Just ask naturally -- no commands needed!",
    ].join("\n")
}

fn help_knowledge() -> String {
    "Knowledge Tools:\n\n  query, search, explain, similar, store [write], relate [write], reinforce [write], correct [write], delete [write], prove".to_string()
}

fn help_analysis() -> String {
    "Analysis Tools:\n\n  compare, shortest_path, most_connected, isolated".to_string()
}

fn help_temporal() -> String {
    "Temporal Tools:\n\n  timeline, date_query, current".to_string()
}

fn help_investigation() -> String {
    "Investigation Tools:\n\n  ingest [write], analyze, investigate [write], changes, watch [write]".to_string()
}

fn help_assessment() -> String {
    "Assessment Tools:\n\n  assess_create [write], assess_evidence [write], assess_evaluate, assess_list, assess_detail, assess_compare".to_string()
}

fn help_reasoning() -> String {
    "Reasoning Tools:\n\n  what_if, influence".to_string()
}

fn help_actions() -> String {
    "Action Tools:\n\n  rule_create [write], rule_list, rule_fire, schedule".to_string()
}

fn help_reporting() -> String {
    "Reporting Tools:\n\n  briefing, export, entity_timeline".to_string()
}

/// Inject shared JS helper functions for card XHR and autocomplete.
/// Only injected once (checks for window.__engram_card_helpers).
fn ensure_card_helpers() {
    let _ = js_sys::eval(
        "if(!window.__engram_card_helpers){window.__engram_card_helpers=true;\
         window.__ec_auth=function(){var a=sessionStorage.getItem('engram_auth');\
           if(a){try{return JSON.parse(a).token}catch(e){}}return null};\
         window.__ec_xhr=function(m,u,b,cb){var x=new XMLHttpRequest();x.open(m,u,false);\
           x.setRequestHeader('Content-Type','application/json');\
           var t=__ec_auth();if(t)x.setRequestHeader('Authorization','Bearer '+t);\
           if(b)x.send(typeof b==='string'?b:JSON.stringify(b));else x.send();\
           if(x.status>=200&&x.status<300){if(cb)cb(x.responseText);\
             window.dispatchEvent(new CustomEvent('engram-tool-result',{detail:x.responseText}));\
           }else{alert(u+' failed: '+x.status+' '+x.responseText.substring(0,100));}return x};\
         window.__ec_suggest=function(inputId,endpoint,field){var inp=document.getElementById(inputId);\
           if(!inp)return;var wrap=inp.parentElement;wrap.style.position='relative';\
           var dd=document.createElement('div');dd.className='tc-autocomplete';dd.style.cssText=\
           'position:absolute;top:100%;left:0;right:0;z-index:100;max-height:150px;overflow-y:auto;\
           background:var(--bg-tertiary,#232730);border:1px solid var(--border,#2d3139);border-radius:4px;\
           display:none;';wrap.appendChild(dd);\
           inp.addEventListener('input',function(){var v=inp.value;if(v.length<2){dd.style.display='none';return;}\
           var x=new XMLHttpRequest();x.open('POST',endpoint,false);x.setRequestHeader('Content-Type','application/json');\
           var t=__ec_auth();if(t)x.setRequestHeader('Authorization','Bearer '+t);\
           x.send(JSON.stringify({query:v,limit:8}));dd.innerHTML='';dd.style.display='none';\
           if(x.status===200){var d=JSON.parse(x.responseText);var items=d.results||d.types||d;\
           if(Array.isArray(items)&&items.length>0){items.forEach(function(it){\
           var label=typeof it==='string'?it:(it[field]||it.label||it.name||JSON.stringify(it));\
           var opt=document.createElement('div');opt.textContent=label;\
           opt.style.cssText='padding:0.25rem 0.5rem;cursor:pointer;font-size:0.75rem;color:var(--text,#c9ccd3);';\
           opt.onmouseenter=function(){this.style.background='var(--accent,#4a9eff)';this.style.color='#fff'};\
           opt.onmouseleave=function(){this.style.background='';this.style.color='var(--text,#c9ccd3)'};\
           opt.onclick=function(){inp.value=label;dd.style.display='none'};dd.appendChild(opt)});\
           dd.style.display='block'}}});inp.addEventListener('blur',function(){setTimeout(function(){dd.style.display='none'},200)})};\
        }"
    );
}

/// Generate an interactive parameter card for a specific tool.
/// Delegates to tool_cards module for the actual card HTML.
pub fn generate_tool_card(tool_name: &str) -> Option<String> {
    super::tool_cards::generate_tool_card(tool_name)
}

/// Generate an interactive path search card for the /path command.
/// If `prefill_from` is provided, pre-fill the From field.
pub fn generate_path_card(prefill_from: &str) -> String {
    let from_val = super::markdown::html_escape(prefill_from);
    format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-route\"></i> Find Path</div>\
            <div class=\"chat-card-body\" style=\"padding:0.5rem 0.6rem;\">\
                <div style=\"display:flex;flex-direction:column;gap:0.4rem;\">\
                    <div>\
                        <label style=\"font-size:0.68rem;color:var(--text-muted);text-transform:uppercase;\">From</label>\
                        <input id=\"path-from\" type=\"text\" placeholder=\"Start entity...\" value=\"{from}\" \
                            style=\"width:100%;padding:0.3rem 0.5rem;font-size:0.78rem;\
                                   background:var(--bg-input, #1e2028);color:var(--text);border:1px solid var(--border);\
                                   border-radius:4px;outline:none;font-family:inherit;box-sizing:border-box;\" />\
                    </div>\
                    <div>\
                        <label style=\"font-size:0.68rem;color:var(--text-muted);text-transform:uppercase;\">To</label>\
                        <input id=\"path-to\" type=\"text\" placeholder=\"Target entity...\" \
                            style=\"width:100%;padding:0.3rem 0.5rem;font-size:0.78rem;\
                                   background:var(--bg-input, #1e2028);color:var(--text);border:1px solid var(--border);\
                                   border-radius:4px;outline:none;font-family:inherit;box-sizing:border-box;\" />\
                    </div>\
                    <div>\
                        <label style=\"font-size:0.68rem;color:var(--text-muted);text-transform:uppercase;\">Via <span style=\"font-size:0.6rem;\">(optional)</span></label>\
                        <input id=\"path-via\" type=\"text\" placeholder=\"Must pass through...\" \
                            style=\"width:100%;padding:0.3rem 0.5rem;font-size:0.78rem;\
                                   background:var(--bg-input, #1e2028);color:var(--text);border:1px solid var(--border);\
                                   border-radius:4px;outline:none;font-family:inherit;box-sizing:border-box;\" />\
                    </div>\
                    <div style=\"display:flex;gap:0.5rem;\">\
                        <div style=\"flex:1;\">\
                            <label style=\"font-size:0.68rem;color:var(--text-muted);text-transform:uppercase;\">Min hops</label>\
                            <input id=\"path-min\" type=\"number\" value=\"1\" min=\"1\" max=\"8\" \
                                style=\"width:100%;padding:0.3rem 0.5rem;font-size:0.78rem;\
                                       background:var(--bg-input, #1e2028);color:var(--text);border:1px solid var(--border);\
                                       border-radius:4px;outline:none;font-family:inherit;box-sizing:border-box;\" />\
                        </div>\
                        <div style=\"flex:1;\">\
                            <label style=\"font-size:0.68rem;color:var(--text-muted);text-transform:uppercase;\">Max hops</label>\
                            <input id=\"path-max\" type=\"number\" value=\"5\" min=\"1\" max=\"8\" \
                                style=\"width:100%;padding:0.3rem 0.5rem;font-size:0.78rem;\
                                       background:var(--bg-input, #1e2028);color:var(--text);border:1px solid var(--border);\
                                       border-radius:4px;outline:none;font-family:inherit;box-sizing:border-box;\" />\
                        </div>\
                    </div>\
                    <button onclick=\"\
                        var f=document.getElementById('path-from').value;\
                        var t=document.getElementById('path-to').value;\
                        var v=document.getElementById('path-via').value;\
                        var mn=document.getElementById('path-min').value;\
                        var mx=document.getElementById('path-max').value;\
                        if(!f||!t){{alert('From and To are required');return;}}\
                        var via=v?',\\x22via\\x22:\\x22'+v+'\\x22':'';\
                        var mind=parseInt(mn)>1?',\\x22min_depth\\x22:'+mn:'';\
                        var xhr=new XMLHttpRequest();\
                        xhr.open('POST','/paths',false);\
                        xhr.setRequestHeader('Content-Type','application/json');\
                        var _a=sessionStorage.getItem('engram_auth');var tok=null;if(_a){{try{{tok=JSON.parse(_a).token}}catch(_){{}}}}\
                        if(tok)xhr.setRequestHeader('Authorization','Bearer '+tok);\
                        xhr.send(JSON.stringify({{from:f,to:t,max_depth:parseInt(mx)||5}}));\
                        if(xhr.status===200){{\
                            var data=JSON.parse(xhr.responseText);\
                            var paths=data.paths||[];\
                            var html='<strong>'+paths.length+' path'+(paths.length!==1?'s':'')+' found</strong>';\
                            paths.forEach(function(p,i){{\
                                if(i>=5)return;\
                                html+='<div style=\\x22padding:0.2rem 0;font-size:0.78rem;\\x22>'+(p.length-1)+' hops: '+p.join(' > ')+'</div>';\
                            }});\
                            if(paths.length>0)window.__engram_graph&&window.__engram_graph.showPaths(JSON.stringify(paths));\
                            window.dispatchEvent(new CustomEvent('engram-chat-send',{{detail:'Found '+paths.length+' paths from \\x22'+f+'\\x22 to \\x22'+t+'\\x22'}}));\
                        }}else{{alert('Path search failed: '+xhr.status);}}\
                    \" style=\"padding:0.35rem 0.75rem;font-size:0.78rem;font-family:inherit;\
                             background:var(--accent, #4a9eff);color:#fff;border:none;border-radius:4px;\
                             cursor:pointer;display:flex;align-items:center;gap:0.3rem;justify-content:center;\">\
                        <i class=\"fa-solid fa-magnifying-glass\"></i> Find Paths\
                    </button>\
                </div>\
            </div>\
         </div>",
        from = from_val,
    )
}

/// Filter slash commands by a partial query string (without leading `/`).
/// Returns matching (command, icon, description) tuples.
pub fn filter_commands(query: &str) -> Vec<(&'static str, &'static str, &'static str)> {
    let query_lower = query.to_lowercase();
    SLASH_COMMANDS
        .iter()
        .filter(|(cmd, _, _)| {
            let cmd_body = &cmd[1..]; // strip leading /
            cmd_body.to_lowercase().contains(&query_lower) || query_lower.is_empty()
        })
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_overview_html_has_all_categories() {
        let html = generate_help_html("/help");
        assert!(html.contains("Knowledge"));
        assert!(html.contains("Analysis"));
        assert!(html.contains("Temporal"));
        assert!(html.contains("Investigation"));
        assert!(html.contains("Assessment"));
        assert!(html.contains("Reasoning"));
        assert!(html.contains("Actions"));
        assert!(html.contains("Reporting"));
        assert!(html.contains("chat-help-grid"));
    }

    #[test]
    fn test_help_category_html_has_tools() {
        let html = generate_help_html("/help knowledge");
        assert!(html.contains("query"));
        assert!(html.contains("search"));
        assert!(html.contains("explain"));
        assert!(html.contains("store"));
        assert!(html.contains("chat-help-write-badge")); // write tools have badge
    }

    #[test]
    fn test_help_category_tools_are_clickable() {
        let html = generate_help_html("/help knowledge");
        assert!(html.contains("onclick"));
        assert!(html.contains("engram-tool-card"));
    }

    #[test]
    fn test_help_overview_categories_are_clickable() {
        let html = generate_help_html("/help");
        assert!(html.contains("onclick"));
        assert!(html.contains("engram-chat-send"));
    }

    #[test]
    fn test_help_unknown_category() {
        let html = generate_help_html("/help nonexistent");
        assert!(html.contains("Unknown category"));
    }

    #[test]
    fn test_help_plain_text_still_works() {
        let text = generate_help_response("/help");
        assert!(text.contains("Knowledge Chat"));
        assert!(text.contains("Knowledge (10)"));
    }

    #[test]
    fn test_path_card_has_inputs() {
        let html = generate_path_card("");
        assert!(html.contains("path-from"));
        assert!(html.contains("path-to"));
        assert!(html.contains("path-via"));
        assert!(html.contains("path-min"));
        assert!(html.contains("path-max"));
        assert!(html.contains("Find Paths"));
    }

    #[test]
    fn test_path_card_prefill() {
        let html = generate_path_card("Russia");
        assert!(html.contains("value=\"Russia\""));
    }

    #[test]
    fn test_path_card_xss_prevention() {
        let html = generate_path_card("<script>alert(1)</script>");
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_filter_commands_path() {
        let results = filter_commands("path");
        assert!(results.iter().any(|(cmd, _, _)| *cmd == "/path"));
    }

    #[test]
    fn test_filter_commands_empty() {
        let results = filter_commands("");
        assert_eq!(results.len(), SLASH_COMMANDS.len());
    }

    #[test]
    fn test_filter_commands_help() {
        let results = filter_commands("help");
        assert!(results.len() >= 9); // /help + 8 categories
    }

    #[test]
    fn test_analysis_has_path_tool() {
        let html = generate_help_html("/help analysis");
        assert!(html.contains("shortest_path"));
    }
}
