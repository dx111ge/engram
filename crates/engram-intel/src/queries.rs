/// Dynamic query generation for any target country.
/// Generates GDELT search queries and Wikidata SPARQL queries
/// based on the country name and geopolitical context.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct GdeltQuery {
    pub query: String,
    pub topics: Vec<String>,
}

/// Generate GDELT search queries for a target country.
/// Returns a set of topic-tagged queries tailored to the country.
pub fn gdelt_queries(country: &str) -> Vec<GdeltQuery> {
    let c = country;
    vec![
        GdeltQuery {
            query: format!("{c} sanctions"),
            topics: vec!["sanctions".into(), "economic".into()],
        },
        GdeltQuery {
            query: format!("{c} military buildup troops"),
            topics: vec!["military_buildup".into()],
        },
        GdeltQuery {
            query: format!("{c} war offensive attack"),
            topics: vec!["conflict".into()],
        },
        GdeltQuery {
            query: format!("{c} ceasefire peace negotiations"),
            topics: vec!["ceasefire".into(), "negotiations".into()],
        },
        GdeltQuery {
            query: format!("{c} nuclear weapons escalation"),
            topics: vec!["nuclear".into()],
        },
        GdeltQuery {
            query: format!("{c} NATO alliance"),
            topics: vec!["alliance".into()],
        },
        GdeltQuery {
            query: format!("{c} territory sovereignty border"),
            topics: vec!["territorial".into()],
        },
        GdeltQuery {
            query: format!("{c} economy GDP recession"),
            topics: vec!["economic".into()],
        },
        GdeltQuery {
            query: format!("{c} oil gas energy pipeline"),
            topics: vec!["energy".into()],
        },
        GdeltQuery {
            query: format!("{c} weapons arms military aid"),
            topics: vec!["arms_transfer".into()],
        },
        GdeltQuery {
            query: format!("{c} cyber hack electronic warfare"),
            topics: vec!["cyber".into()],
        },
        GdeltQuery {
            query: format!("{c} disinformation hybrid warfare"),
            topics: vec!["hybrid_warfare".into()],
        },
        GdeltQuery {
            query: format!("{c} trade import export evasion"),
            topics: vec!["trade".into(), "sanctions".into()],
        },
        GdeltQuery {
            query: format!("{c} humanitarian refugees war crimes"),
            topics: vec!["humanitarian".into()],
        },
        GdeltQuery {
            query: format!("{c} naval fleet maritime"),
            topics: vec!["naval".into()],
        },
    ]
}

/// Generate Wikidata SPARQL query to fetch country borders, alliances, and metadata.
pub fn wikidata_country_query(wikidata_id: &str) -> String {
    format!(r#"
SELECT ?borderLabel ?borderPop ?borderArea WHERE {{
  wd:{wikidata_id} wdt:P47 ?border.
  OPTIONAL {{ ?border wdt:P1082 ?borderPop. }}
  OPTIONAL {{ ?border wdt:P2046 ?borderArea. }}
  SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en". }}
}}
"#)
}

/// Generate Wikidata SPARQL query to fetch organization memberships.
pub fn wikidata_memberships_query(wikidata_id: &str) -> String {
    format!(r#"
SELECT ?orgLabel WHERE {{
  wd:{wikidata_id} wdt:P463 ?org.
  SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en". }}
}}
"#)
}

/// Generate Wikidata SPARQL query to fetch current leaders/heads of state & government.
pub fn wikidata_leaders_query(wikidata_id: &str) -> String {
    format!(r#"
SELECT DISTINCT ?leaderLabel ?positionLabel WHERE {{
  {{
    wd:{wikidata_id} p:P35 ?stmt.
    ?stmt ps:P35 ?leader.
    FILTER NOT EXISTS {{ ?stmt pq:P582 ?end. }}
    BIND("Head of State" AS ?fallbackPos)
  }} UNION {{
    wd:{wikidata_id} p:P6 ?stmt.
    ?stmt ps:P6 ?leader.
    FILTER NOT EXISTS {{ ?stmt pq:P582 ?end. }}
    BIND("Head of Government" AS ?fallbackPos)
  }}
  OPTIONAL {{ ?leader wdt:P39 ?position. }}
  SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en". }}
}} LIMIT 10
"#)
}

/// World Bank indicator codes for economic data.
pub struct WorldBankIndicator {
    pub code: &'static str,
    pub name: &'static str,
    pub format: &'static str,
}

pub static WORLD_BANK_INDICATORS: &[WorldBankIndicator] = &[
    WorldBankIndicator { code: "NY.GDP.MKTP.CD", name: "GDP (current USD)", format: "currency" },
    WorldBankIndicator { code: "FP.CPI.TOTL.ZG", name: "Inflation (CPI %)", format: "percent" },
    WorldBankIndicator { code: "MS.MIL.XPND.GD.ZS", name: "Military spending (% GDP)", format: "percent" },
    WorldBankIndicator { code: "SP.POP.TOTL", name: "Population", format: "number" },
    WorldBankIndicator { code: "BN.CAB.XOKA.CD", name: "Current account balance", format: "currency" },
];

/// Map common country names to ISO 3166 alpha-3 codes for World Bank API.
pub fn country_to_iso3(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "russia" | "russian federation" => Some("RUS"),
        "ukraine" => Some("UKR"),
        "china" | "people's republic of china" => Some("CHN"),
        "united states" | "usa" | "us" => Some("USA"),
        "iran" | "islamic republic of iran" => Some("IRN"),
        "north korea" | "dprk" => Some("PRK"),
        "south korea" | "republic of korea" => Some("KOR"),
        "taiwan" => Some("TWN"),
        "israel" => Some("ISR"),
        "turkey" | "turkiye" => Some("TUR"),
        "india" => Some("IND"),
        "pakistan" => Some("PAK"),
        "saudi arabia" => Some("SAU"),
        "belarus" => Some("BLR"),
        "georgia" => Some("GEO"),
        "moldova" => Some("MDA"),
        "poland" => Some("POL"),
        "finland" => Some("FIN"),
        "estonia" => Some("EST"),
        "latvia" => Some("LVA"),
        "lithuania" => Some("LTU"),
        "germany" => Some("DEU"),
        "france" => Some("FRA"),
        "united kingdom" | "uk" => Some("GBR"),
        "japan" => Some("JPN"),
        "brazil" => Some("BRA"),
        "syria" => Some("SYR"),
        "iraq" => Some("IRQ"),
        "afghanistan" => Some("AFG"),
        "egypt" => Some("EGY"),
        "ethiopia" => Some("ETH"),
        "nigeria" => Some("NGA"),
        "south africa" => Some("ZAF"),
        "venezuela" => Some("VEN"),
        "cuba" => Some("CUB"),
        "myanmar" | "burma" => Some("MMR"),
        "yemen" => Some("YEM"),
        "libya" => Some("LBY"),
        "somalia" => Some("SOM"),
        "sudan" => Some("SDN"),
        "philippines" => Some("PHL"),
        "indonesia" => Some("IDN"),
        "thailand" => Some("THA"),
        "vietnam" | "viet nam" => Some("VNM"),
        "malaysia" => Some("MYS"),
        "singapore" => Some("SGP"),
        "australia" => Some("AUS"),
        "new zealand" => Some("NZL"),
        "canada" => Some("CAN"),
        "mexico" => Some("MEX"),
        "colombia" => Some("COL"),
        "argentina" => Some("ARG"),
        "chile" => Some("CHL"),
        "peru" => Some("PER"),
        "south sudan" => Some("SSD"),
        "democratic republic of congo" | "drc" | "congo" => Some("COD"),
        "morocco" => Some("MAR"),
        "algeria" => Some("DZA"),
        "tunisia" => Some("TUN"),
        "kenya" => Some("KEN"),
        "tanzania" => Some("TZA"),
        "uganda" => Some("UGA"),
        "angola" => Some("AGO"),
        "mozambique" => Some("MOZ"),
        "cameroon" => Some("CMR"),
        "jordan" => Some("JOR"),
        "lebanon" => Some("LBN"),
        "qatar" => Some("QAT"),
        "united arab emirates" | "uae" => Some("ARE"),
        "kuwait" => Some("KWT"),
        "oman" => Some("OMN"),
        "bahrain" => Some("BHR"),
        "bangladesh" => Some("BGD"),
        "sri lanka" => Some("LKA"),
        "nepal" => Some("NPL"),
        "cambodia" => Some("KHM"),
        "laos" => Some("LAO"),
        "mongolia" => Some("MNG"),
        "kazakhstan" => Some("KAZ"),
        "uzbekistan" => Some("UZB"),
        "turkmenistan" => Some("TKM"),
        "kyrgyzstan" => Some("KGZ"),
        "tajikistan" => Some("TJK"),
        "armenia" => Some("ARM"),
        "azerbaijan" => Some("AZE"),
        "romania" => Some("ROU"),
        "hungary" => Some("HUN"),
        "czech republic" | "czechia" => Some("CZE"),
        "slovakia" => Some("SVK"),
        "bulgaria" => Some("BGR"),
        "croatia" => Some("HRV"),
        "serbia" => Some("SRB"),
        "greece" => Some("GRC"),
        "italy" => Some("ITA"),
        "spain" => Some("ESP"),
        "portugal" => Some("PRT"),
        "netherlands" => Some("NLD"),
        "belgium" => Some("BEL"),
        "sweden" => Some("SWE"),
        "norway" => Some("NOR"),
        "denmark" => Some("DNK"),
        "ireland" => Some("IRL"),
        "switzerland" => Some("CHE"),
        "austria" => Some("AUT"),
        _ => None,
    }
}

/// Map common country names to Wikidata entity IDs.
pub fn country_to_wikidata(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "russia" | "russian federation" => Some("Q159"),
        "ukraine" => Some("Q212"),
        "china" | "people's republic of china" => Some("Q148"),
        "united states" | "usa" | "us" => Some("Q30"),
        "iran" | "islamic republic of iran" => Some("Q794"),
        "north korea" | "dprk" => Some("Q423"),
        "south korea" | "republic of korea" => Some("Q884"),
        "taiwan" => Some("Q865"),
        "israel" => Some("Q801"),
        "turkey" | "turkiye" => Some("Q43"),
        "india" => Some("Q668"),
        "pakistan" => Some("Q843"),
        "saudi arabia" => Some("Q851"),
        "belarus" => Some("Q184"),
        "georgia" => Some("Q230"),
        "moldova" => Some("Q217"),
        "poland" => Some("Q36"),
        "finland" => Some("Q33"),
        "germany" => Some("Q183"),
        "france" => Some("Q142"),
        "united kingdom" | "uk" => Some("Q145"),
        "japan" => Some("Q17"),
        "brazil" => Some("Q155"),
        "syria" => Some("Q858"),
        "iraq" => Some("Q796"),
        "egypt" => Some("Q79"),
        "ethiopia" => Some("Q115"),
        "nigeria" => Some("Q1033"),
        "south africa" => Some("Q258"),
        "myanmar" | "burma" => Some("Q836"),
        "yemen" => Some("Q805"),
        "libya" => Some("Q1016"),
        "sudan" => Some("Q1049"),
        "philippines" => Some("Q928"),
        "indonesia" => Some("Q252"),
        "thailand" => Some("Q869"),
        "vietnam" | "viet nam" => Some("Q881"),
        "malaysia" => Some("Q833"),
        "singapore" => Some("Q334"),
        "australia" => Some("Q408"),
        "new zealand" => Some("Q664"),
        "canada" => Some("Q16"),
        "mexico" => Some("Q96"),
        "colombia" => Some("Q739"),
        "argentina" => Some("Q414"),
        "chile" => Some("Q298"),
        "peru" => Some("Q419"),
        "south sudan" => Some("Q958"),
        "democratic republic of congo" | "drc" | "congo" => Some("Q974"),
        "morocco" => Some("Q1028"),
        "algeria" => Some("Q262"),
        "tunisia" => Some("Q948"),
        "kenya" => Some("Q114"),
        "tanzania" => Some("Q924"),
        "uganda" => Some("Q1036"),
        "angola" => Some("Q916"),
        "mozambique" => Some("Q1029"),
        "cameroon" => Some("Q1009"),
        "jordan" => Some("Q810"),
        "lebanon" => Some("Q822"),
        "qatar" => Some("Q846"),
        "united arab emirates" | "uae" => Some("Q878"),
        "kuwait" => Some("Q817"),
        "oman" => Some("Q842"),
        "bahrain" => Some("Q398"),
        "bangladesh" => Some("Q902"),
        "sri lanka" => Some("Q854"),
        "nepal" => Some("Q837"),
        "cambodia" => Some("Q424"),
        "laos" => Some("Q819"),
        "mongolia" => Some("Q711"),
        "kazakhstan" => Some("Q232"),
        "uzbekistan" => Some("Q265"),
        "turkmenistan" => Some("Q874"),
        "kyrgyzstan" => Some("Q813"),
        "tajikistan" => Some("Q863"),
        "armenia" => Some("Q399"),
        "azerbaijan" => Some("Q227"),
        "romania" => Some("Q218"),
        "hungary" => Some("Q28"),
        "czech republic" | "czechia" => Some("Q213"),
        "slovakia" => Some("Q214"),
        "bulgaria" => Some("Q219"),
        "croatia" => Some("Q224"),
        "serbia" => Some("Q403"),
        "greece" => Some("Q41"),
        "italy" => Some("Q38"),
        "spain" => Some("Q29"),
        "portugal" => Some("Q45"),
        "netherlands" => Some("Q55"),
        "belgium" => Some("Q31"),
        "sweden" => Some("Q34"),
        "norway" => Some("Q20"),
        "denmark" => Some("Q35"),
        "ireland" => Some("Q27"),
        "switzerland" => Some("Q39"),
        "austria" => Some("Q40"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_queries() {
        let queries = gdelt_queries("Iran");
        assert!(queries.len() >= 10);
        assert!(queries[0].query.contains("Iran"));
    }

    #[test]
    fn iso3_lookup() {
        assert_eq!(country_to_iso3("Russia"), Some("RUS"));
        assert_eq!(country_to_iso3("taiwan"), Some("TWN"));
        assert_eq!(country_to_iso3("Narnia"), None);
    }

    #[test]
    fn wikidata_lookup() {
        assert_eq!(country_to_wikidata("Russia"), Some("Q159"));
        assert_eq!(country_to_wikidata("Iran"), Some("Q794"));
    }
}
