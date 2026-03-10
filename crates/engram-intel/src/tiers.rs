/// Source reliability tier database.
/// Maps news domains to confidence scores based on editorial standards,
/// state control, and historical accuracy.

pub struct SourceTier {
    pub domain: &'static str,
    pub confidence: f32,
    pub tier: u8,
}

/// Tier 1: Institutional / regulatory (0.88-0.95)
/// Tier 2: Quality journalism / think tanks (0.75-0.88)
/// Tier 3: State-controlled / propaganda (0.20-0.35)
static TIERS: &[SourceTier] = &[
    // Tier 1: Institutional
    SourceTier { domain: "un.org", confidence: 0.92, tier: 1 },
    SourceTier { domain: "worldbank.org", confidence: 0.93, tier: 1 },
    SourceTier { domain: "imf.org", confidence: 0.92, tier: 1 },
    SourceTier { domain: "nato.int", confidence: 0.90, tier: 1 },
    SourceTier { domain: "europa.eu", confidence: 0.90, tier: 1 },
    SourceTier { domain: "state.gov", confidence: 0.88, tier: 1 },
    SourceTier { domain: "gov.uk", confidence: 0.88, tier: 1 },
    SourceTier { domain: "iaea.org", confidence: 0.90, tier: 1 },
    SourceTier { domain: "icj-cij.org", confidence: 0.92, tier: 1 },
    SourceTier { domain: "icc-cpi.int", confidence: 0.90, tier: 1 },
    // Tier 2: Quality journalism
    SourceTier { domain: "reuters.com", confidence: 0.85, tier: 2 },
    SourceTier { domain: "apnews.com", confidence: 0.85, tier: 2 },
    SourceTier { domain: "bbc.com", confidence: 0.82, tier: 2 },
    SourceTier { domain: "bbc.co.uk", confidence: 0.82, tier: 2 },
    SourceTier { domain: "nytimes.com", confidence: 0.82, tier: 2 },
    SourceTier { domain: "washingtonpost.com", confidence: 0.80, tier: 2 },
    SourceTier { domain: "theguardian.com", confidence: 0.80, tier: 2 },
    SourceTier { domain: "aljazeera.com", confidence: 0.78, tier: 2 },
    SourceTier { domain: "dw.com", confidence: 0.80, tier: 2 },
    SourceTier { domain: "france24.com", confidence: 0.78, tier: 2 },
    SourceTier { domain: "economist.com", confidence: 0.82, tier: 2 },
    SourceTier { domain: "ft.com", confidence: 0.82, tier: 2 },
    SourceTier { domain: "bloomberg.com", confidence: 0.80, tier: 2 },
    // Tier 2: Think tanks / OSINT
    SourceTier { domain: "understandingwar.org", confidence: 0.85, tier: 2 },
    SourceTier { domain: "rusi.org", confidence: 0.85, tier: 2 },
    SourceTier { domain: "csis.org", confidence: 0.82, tier: 2 },
    SourceTier { domain: "chathamhouse.org", confidence: 0.82, tier: 2 },
    SourceTier { domain: "iiss.org", confidence: 0.82, tier: 2 },
    SourceTier { domain: "sipri.org", confidence: 0.85, tier: 2 },
    SourceTier { domain: "crisisgroup.org", confidence: 0.82, tier: 2 },
    SourceTier { domain: "globalsecurity.org", confidence: 0.78, tier: 2 },
    SourceTier { domain: "janes.com", confidence: 0.85, tier: 2 },
    // Tier 2: Regional quality
    SourceTier { domain: "kyivindependent.com", confidence: 0.78, tier: 2 },
    SourceTier { domain: "ukrinform.net", confidence: 0.75, tier: 2 },
    SourceTier { domain: "unian.net", confidence: 0.72, tier: 2 },
    SourceTier { domain: "unian.ua", confidence: 0.72, tier: 2 },
    SourceTier { domain: "pravda.com.ua", confidence: 0.70, tier: 2 },
    SourceTier { domain: "interfax.com.ua", confidence: 0.75, tier: 2 },
    SourceTier { domain: "timesofisrael.com", confidence: 0.78, tier: 2 },
    SourceTier { domain: "haaretz.com", confidence: 0.78, tier: 2 },
    SourceTier { domain: "scmp.com", confidence: 0.72, tier: 2 },
    SourceTier { domain: "straitstimes.com", confidence: 0.78, tier: 2 },
    SourceTier { domain: "japantimes.co.jp", confidence: 0.78, tier: 2 },
    SourceTier { domain: "hindustantimes.com", confidence: 0.72, tier: 2 },
    // Tier 3: State-controlled
    SourceTier { domain: "tass.com", confidence: 0.25, tier: 3 },
    SourceTier { domain: "rt.com", confidence: 0.25, tier: 3 },
    SourceTier { domain: "sputniknews.com", confidence: 0.20, tier: 3 },
    SourceTier { domain: "ria.ru", confidence: 0.25, tier: 3 },
    SourceTier { domain: "iz.ru", confidence: 0.25, tier: 3 },
    SourceTier { domain: "rg.ru", confidence: 0.25, tier: 3 },
    SourceTier { domain: "inosmi.ru", confidence: 0.30, tier: 3 },
    SourceTier { domain: "xinhuanet.com", confidence: 0.35, tier: 3 },
    SourceTier { domain: "globaltimes.cn", confidence: 0.30, tier: 3 },
    SourceTier { domain: "cgtn.com", confidence: 0.30, tier: 3 },
    SourceTier { domain: "presstv.ir", confidence: 0.20, tier: 3 },
    SourceTier { domain: "irna.ir", confidence: 0.25, tier: 3 },
    SourceTier { domain: "kcna.kp", confidence: 0.10, tier: 3 },
    SourceTier { domain: "telesurtv.net", confidence: 0.30, tier: 3 },
];

/// Look up domain confidence. Strips "www." prefix and checks suffix matches.
/// Returns (confidence, tier). Unknown domains get (0.50, 0).
pub fn domain_confidence(url: &str) -> (f32, u8) {
    let domain = extract_domain(url);

    // Exact match
    for t in TIERS {
        if domain == t.domain {
            return (t.confidence, t.tier);
        }
    }
    // Suffix match (e.g., "news.bbc.co.uk" matches "bbc.co.uk")
    for t in TIERS {
        if domain.ends_with(t.domain) {
            return (t.confidence, t.tier);
        }
    }
    // Unknown
    (0.50, 0)
}

fn extract_domain(url: &str) -> String {
    let s = url.trim();
    // Strip protocol
    let s = if let Some(rest) = s.strip_prefix("https://") {
        rest
    } else if let Some(rest) = s.strip_prefix("http://") {
        rest
    } else {
        s
    };
    // Take everything before first /
    let s = s.split('/').next().unwrap_or(s);
    // Strip www.
    let s = s.strip_prefix("www.").unwrap_or(s);
    s.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier1_sources() {
        let (c, t) = domain_confidence("https://www.reuters.com/article/foo");
        assert_eq!(t, 2);
        assert!((c - 0.85).abs() < 0.01);
    }

    #[test]
    fn tier3_propaganda() {
        let (c, t) = domain_confidence("https://rt.com/news/something");
        assert_eq!(t, 3);
        assert!((c - 0.25).abs() < 0.01);
    }

    #[test]
    fn unknown_domain() {
        let (c, t) = domain_confidence("https://randomnews.example.com/article");
        assert_eq!(t, 0);
        assert!((c - 0.50).abs() < 0.01);
    }
}
