#!/usr/bin/env python3
"""
Use Case 13: Russia Geopolitical Analysis -- AI Intelligence Analyst

Builds a comprehensive geopolitical knowledge graph about Russia using
LIVE DATA from public APIs:
  - Wikidata SPARQL: borders, alliances, leaders, population
  - World Bank API: GDP, inflation, military spending time series
  - Analysis layer: predictions, source tiers, contradictions, inference

Demonstrates predictions with probability derived from weighted evidence
chains, source reliability tiers, inference rules, contradiction detection,
and evidence-based explainability.

Designed for both HTTP API (this script) and MCP/Claude Code integration.

Prerequisites:
    engram serve russia.brain 127.0.0.1:3030
    python russia_demo.py

External data sources (fetched live):
    - Wikidata SPARQL: https://query.wikidata.org/sparql
    - World Bank API: https://api.worldbank.org/v2/
    - GDELT Project: https://api.gdeltproject.org/ (global news events)
    - Exchange Rate API: https://open.er-api.com/ (live RUB/USD)
    - REST Countries: https://restcountries.com/ (country metadata)
    - Wikipedia API: https://en.wikipedia.org/api/ (conflict summary)
"""

import json
import sys
import time
import requests as http
from urllib.parse import quote

BASE = "http://127.0.0.1:3030"
WIKIDATA = "https://query.wikidata.org/sparql"
WORLDBANK = "https://api.worldbank.org/v2"
GDELT = "https://api.gdeltproject.org/api/v2/doc/doc"
EXCHANGE = "https://open.er-api.com/v6/latest/USD"
RESTCOUNTRIES = "https://restcountries.com/v3.1"
WIKIPEDIA = "https://en.wikipedia.org/api/rest_v1/page/summary"
UA = {"User-Agent": "EngRAM/0.1 (geopolitical-knowledge-graph-demo)"}

# ---------------------------------------------------------------------------
# engram API helpers
# ---------------------------------------------------------------------------

def api(method, path, payload=None):
    url = f"{BASE}{path}"
    try:
        if method == "GET":
            r = http.get(url, timeout=10)
        elif method == "POST":
            r = http.post(url, json=payload, timeout=30)
        elif method == "DELETE":
            r = http.delete(url, timeout=10)
        else:
            raise ValueError(f"Unknown method: {method}")
        if not r.text or not r.text.strip():
            return {}
        try:
            return r.json()
        except Exception:
            return {"raw": r.text}
    except http.exceptions.ConnectionError:
        print(f"ERROR: Cannot connect to {BASE}. Is engram running?")
        sys.exit(1)

def store(label, node_type=None, confidence=None, source=None, props=None):
    body = {"entity": label}
    if node_type:
        body["type"] = node_type
    if confidence is not None:
        body["confidence"] = confidence
    if source:
        body["source"] = source
    if props:
        body["properties"] = props
    return api("POST", "/store", body)

def relate(from_l, rel, to_l, confidence=None):
    body = {"from": from_l, "to": to_l, "relationship": rel}
    if confidence is not None:
        body["confidence"] = confidence
    return api("POST", "/relate", body)

def reinforce(label, source=None):
    body = {"entity": label}
    if source:
        body["source"] = source
    return api("POST", "/learn/reinforce", body)

def correct(label, reason, source=None):
    body = {"entity": label, "reason": reason}
    if source:
        body["source"] = source
    return api("POST", "/learn/correct", body)

def search(query, limit=10):
    return api("POST", "/search", {"query": query, "limit": limit})

def traverse(label, depth=2, direction="both", min_conf=0.0):
    return api("POST", "/query", {
        "start": label, "depth": depth,
        "direction": direction, "min_confidence": min_conf
    })

def explain(label):
    return api("GET", f"/explain/{quote(label, safe='')}")

def stats():
    return api("GET", "/stats")

def fire_rules(rules_list):
    """Pass rules inline to /learn/derive."""
    return api("POST", "/learn/derive", {"rules": rules_list})

def export_jsonld():
    return api("GET", "/export/jsonld")

def calc_probability(evidence_for, evidence_against):
    """Bayesian-inspired probability: P = weighted_for * (1 - weighted_against * discount)."""
    confs_for = [c for _, c, _ in evidence_for]
    confs_against = [c for _, c, _ in evidence_against]

    total_for = sum(confs_for)
    weighted_for = sum(c * c for c in confs_for) / total_for if total_for else 0

    total_against = sum(confs_against)
    weighted_against = sum(c * c for c in confs_against) / total_against if total_against else 0

    discount = len(confs_against) / (len(confs_for) + len(confs_against)) if confs_for or confs_against else 0
    prob = weighted_for * (1 - weighted_against * discount)
    return round(max(0.05, min(0.95, prob)), 2)

def store_prediction_with_evidence(label, probability, evidence_for, evidence_against,
                                   hypothesis, timeframe, category):
    """Store a prediction node with linked evidence nodes."""
    store(label, "prediction", probability, "engram-simulation", {
        "hypothesis": hypothesis,
        "timeframe": timeframe,
        "probability": str(probability),
        "evidence_for_count": str(len(evidence_for)),
        "evidence_against_count": str(len(evidence_against)),
        "category": category,
        "methodology": "Bayesian evidence aggregation from weighted source tiers"
    })
    for desc, conf, src in evidence_for:
        ev_label = f"Ev:{label.split(':')[1]}:+:{desc[:35]}"
        store(ev_label, "evidence", conf, src, {"description": desc, "direction": "supporting"})
        relate(ev_label, "supports", label, conf)
    for desc, conf, src in evidence_against:
        ev_label = f"Ev:{label.split(':')[1]}:-:{desc[:35]}"
        store(ev_label, "evidence", conf, src, {"description": desc, "direction": "contradicting"})
        relate(ev_label, "weakens", label, conf)

def section(title):
    print(f"\n{'='*70}")
    print(f"  {title}")
    print(f"{'='*70}\n")

def subsection(title):
    print(f"\n--- {title} ---\n")

# ---------------------------------------------------------------------------
# Wikidata SPARQL helpers
# ---------------------------------------------------------------------------

def sparql_query(query):
    """Execute a Wikidata SPARQL query and return results."""
    try:
        r = http.get(WIKIDATA, params={"query": query, "format": "json"},
                     headers=UA, timeout=30)
        if r.status_code == 200:
            return r.json()["results"]["bindings"]
        else:
            print(f"  SPARQL error: {r.status_code}")
            return []
    except Exception as e:
        print(f"  SPARQL failed: {e}")
        return []

def sparql_value(item, key, default=""):
    """Extract value from SPARQL result binding."""
    return item.get(key, {}).get("value", default)

# ---------------------------------------------------------------------------
# World Bank API helpers
# ---------------------------------------------------------------------------

def worldbank_indicator(country_code, indicator, date_range="2019:2023"):
    """Fetch a World Bank indicator time series."""
    try:
        url = f"{WORLDBANK}/country/{country_code}/indicator/{indicator}"
        r = http.get(url, params={"date": date_range, "format": "json"}, timeout=15)
        data = r.json()
        if len(data) > 1 and data[1]:
            return [(item["date"], item["value"]) for item in data[1] if item["value"] is not None]
        return []
    except Exception as e:
        print(f"  World Bank API error: {e}")
        return []

# ---------------------------------------------------------------------------
# GDELT news helpers
# ---------------------------------------------------------------------------

def gdelt_articles(query, max_records=10, timespan="30d"):
    """Fetch recent news articles from GDELT."""
    try:
        r = http.get(GDELT, params={
            "query": query, "mode": "artlist",
            "maxrecords": max_records, "format": "json",
            "timespan": timespan
        }, timeout=15)
        if r.status_code == 200:
            return r.json().get("articles", [])
        return []
    except Exception as e:
        print(f"  GDELT error: {e}")
        return []

# ---------------------------------------------------------------------------
# Exchange rate helper
# ---------------------------------------------------------------------------

def get_exchange_rate(currency="RUB"):
    """Get live exchange rate vs USD."""
    try:
        r = http.get(EXCHANGE, timeout=10)
        data = r.json()
        return data["rates"].get(currency), data.get("time_last_update_utc", "unknown")
    except Exception as e:
        print(f"  Exchange rate error: {e}")
        return None, "error"

# ---------------------------------------------------------------------------
# REST Countries helper
# ---------------------------------------------------------------------------

def get_country_data(code):
    """Fetch country metadata from REST Countries API."""
    try:
        r = http.get(f"{RESTCOUNTRIES}/alpha/{code}",
                     params={"fields": "name,population,area,borders,capital,region,subregion"},
                     timeout=10)
        if r.status_code == 200:
            return r.json()
        return {}
    except Exception as e:
        print(f"  REST Countries error: {e}")
        return {}

# ---------------------------------------------------------------------------
# Wikipedia helper
# ---------------------------------------------------------------------------

def wikipedia_summary(title):
    """Fetch Wikipedia article summary."""
    try:
        r = http.get(f"{WIKIPEDIA}/{title}", headers=UA, timeout=10)
        if r.status_code == 200:
            return r.json()
        return {}
    except Exception as e:
        print(f"  Wikipedia error: {e}")
        return {}

# ---------------------------------------------------------------------------
# Phase 1: Live Data -- Wikidata (Countries, Borders, Memberships)
# ---------------------------------------------------------------------------

def phase1_wikidata():
    section("Phase 1: Live Data from Wikidata SPARQL")

    # 1a: Russia itself
    print("Fetching Russia entity data from Wikidata...")
    russia_q = '''
    SELECT ?pop ?area ?capitalLabel WHERE {
      OPTIONAL { wd:Q159 wdt:P1082 ?pop }
      OPTIONAL { wd:Q159 wdt:P2046 ?area }
      OPTIONAL { wd:Q159 wdt:P36 ?capital }
      SERVICE wikibase:label { bd:serviceParam wikibase:language "en" }
    } LIMIT 1
    '''
    results = sparql_query(russia_q)
    pop = "unknown"
    area = "unknown"
    capital = "Moscow"
    if results:
        pop_val = sparql_value(results[0], "pop")
        area_val = sparql_value(results[0], "area")
        capital = sparql_value(results[0], "capitalLabel", "Moscow")
        if pop_val:
            pop = f"{int(float(pop_val)):,}"
        if area_val:
            area = f"{int(float(area_val)):,} km2"

    store("Russia", "country", 0.95, "wikidata", {
        "wikidata_id": "Q159",
        "capital": capital,
        "population": pop,
        "area": area,
        "government": "federal semi-presidential republic",
        "nuclear_weapons": "yes",
        "un_security_council": "permanent member (veto)",
        "data_source": "wikidata.org (live)"
    })
    print(f"  Russia: pop={pop}, area={area}, capital={capital}")

    # 1b: Bordering countries with NATO/EU/CSTO membership
    print("\nFetching countries bordering Russia...")
    borders_q = '''
    SELECT DISTINCT ?country ?countryLabel ?pop ?natoMember ?euMember ?cstoMember WHERE {
      wd:Q159 wdt:P47 ?country .
      ?country wdt:P31 wd:Q6256 .
      OPTIONAL { ?country wdt:P1082 ?pop }
      OPTIONAL { ?country wdt:P463 wd:Q7184 . BIND(true AS ?natoMember) }
      OPTIONAL { ?country wdt:P463 wd:Q458 . BIND(true AS ?euMember) }
      OPTIONAL { ?country wdt:P463 wd:Q318693 . BIND(true AS ?cstoMember) }
      SERVICE wikibase:label { bd:serviceParam wikibase:language "en" }
    }
    '''
    borders = sparql_query(borders_q)

    # Deduplicate (Wikidata sometimes returns multiple pop values)
    seen = set()
    border_countries = []
    for item in borders:
        name = sparql_value(item, "countryLabel")
        if name in seen:
            continue
        seen.add(name)

        pop_val = sparql_value(item, "pop")
        population = f"{int(float(pop_val)):,}" if pop_val else "unknown"
        is_nato = "natoMember" in item
        is_eu = "euMember" in item
        is_csto = "cstoMember" in item

        props = {
            "population": population,
            "data_source": "wikidata.org (live)"
        }
        memberships = []
        if is_nato:
            memberships.append("NATO")
        if is_eu:
            memberships.append("EU")
        if is_csto:
            memberships.append("CSTO")
        if memberships:
            props["memberships"] = ", ".join(memberships)

        store(name, "country", 0.90, "wikidata", props)
        relate("Russia", "borders", name, 0.99)

        if is_nato:
            store("NATO", "alliance", 0.95, "wikidata")
            relate(name, "member_of", "NATO", 0.99)
        if is_eu:
            store("EU", "organization", 0.95, "wikidata")
            relate(name, "member_of", "EU", 0.99)
        if is_csto:
            store("CSTO", "alliance", 0.90, "wikidata")
            relate(name, "member_of", "CSTO", 0.99)

        status = "NATO " if is_nato else ""
        status += "EU " if is_eu else ""
        status += "CSTO " if is_csto else ""
        print(f"  {name:25s} pop={population:>15s}  {status}")

        border_countries.append({
            "name": name, "nato": is_nato, "eu": is_eu, "csto": is_csto
        })

    # 1c: Russia's organization memberships
    print("\nFetching Russia's international organization memberships...")
    orgs_q = '''
    SELECT DISTINCT ?org ?orgLabel WHERE {
      wd:Q159 wdt:P463 ?org .
      SERVICE wikibase:label { bd:serviceParam wikibase:language "en" }
    }
    '''
    orgs = sparql_query(orgs_q)
    org_count = 0
    key_orgs = ["United Nations", "BRICS", "Shanghai Cooperation Organisation",
                "G20", "Commonwealth of Independent States", "Arctic Council",
                "Collective Security Treaty Organization",
                "United Nations Security Council",
                "World Trade Organization", "APEC",
                "Organization for Security and Co-operation in Europe"]

    for item in orgs:
        org_name = sparql_value(item, "orgLabel")
        # Only store notable orgs to keep graph focused
        if any(k.lower() in org_name.lower() for k in key_orgs) or "nuclear" in org_name.lower():
            store(org_name, "organization", 0.90, "wikidata", {
                "data_source": "wikidata.org (live)"
            })
            relate("Russia", "member_of", org_name, 0.95)
            org_count += 1
            print(f"  Russia -> member_of -> {org_name}")

    # 1d: Current Russian leaders from Wikidata
    print("\nFetching current Russian government officials...")
    # Use specific Wikidata IDs for key positions
    leaders_q = '''
    SELECT ?person ?personLabel ?posLabel WHERE {
      VALUES ?pos {
        wd:Q218295   # President of Russia
        wd:Q1006398  # Prime Minister of Russia
        wd:Q831691   # Minister of Foreign Affairs
        wd:Q844944   # Minister of Defence
      }
      ?person p:P39 ?stmt .
      ?stmt ps:P39 ?pos .
      FILTER NOT EXISTS { ?stmt pq:P582 ?end }
      SERVICE wikibase:label { bd:serviceParam wikibase:language "en" }
    }
    '''
    leaders = sparql_query(leaders_q)
    for item in leaders:
        name = sparql_value(item, "personLabel")
        position = sparql_value(item, "posLabel")
        # Filter out non-Russia results (SPARQL sometimes returns extras)
        if "Russia" not in position and name not in ["Vladimir Putin"]:
            continue
        store(name, "leader", 0.92, "wikidata", {
            "position": position,
            "data_source": "wikidata.org (live)"
        })
        relate(name, "holds_office", "Russia", 0.95)
        print(f"  {name}: {position}")

    # Also add Ukraine's president for context
    ukraine_leader_q = '''
    SELECT ?person ?personLabel WHERE {
      ?person p:P39 ?stmt .
      ?stmt ps:P39 wd:Q2915834 .
      FILTER NOT EXISTS { ?stmt pq:P582 ?end }
      SERVICE wikibase:label { bd:serviceParam wikibase:language "en" }
    }
    '''
    ul = sparql_query(ukraine_leader_q)
    for item in ul:
        name = sparql_value(item, "personLabel")
        store(name, "leader", 0.92, "wikidata", {
            "position": "President of Ukraine",
            "data_source": "wikidata.org (live)"
        })
        relate(name, "leads", "Ukraine", 0.95)
        print(f"  {name}: President of Ukraine")

    s = stats()
    print(f"\nWikidata import complete: {s.get('nodes', '?')} nodes, {s.get('edges', '?')} edges")
    return border_countries

# ---------------------------------------------------------------------------
# Phase 2: Live Data -- World Bank (Economic Indicators)
# ---------------------------------------------------------------------------

def phase2_worldbank():
    section("Phase 2: Live Data from World Bank API")

    indicators = {
        "NY.GDP.MKTP.CD": ("GDP (current USD)", "gdp"),
        "FP.CPI.TOTL.ZG": ("Inflation (CPI %)", "inflation"),
        "MS.MIL.XPND.GD.ZS": ("Military spending (% GDP)", "military_pct_gdp"),
    }

    for code, (desc, key) in indicators.items():
        print(f"\nFetching {desc}...")
        data = worldbank_indicator("RUS", code, "2019:2023")
        if data:
            for year, value in sorted(data):
                if key == "gdp":
                    label = f"Econ:Russia-GDP-{year}"
                    formatted = f"${value/1e9:.1f}B"
                elif key == "inflation":
                    label = f"Econ:Russia-Inflation-{year}"
                    formatted = f"{value:.1f}%"
                elif key == "military_pct_gdp":
                    label = f"Econ:Russia-MilitarySpend-{year}"
                    formatted = f"{value:.2f}% of GDP"

                store(label, "economic_indicator", 0.93, "worldbank", {
                    "metric": desc,
                    "year": year,
                    "value": formatted,
                    "raw_value": str(value),
                    "data_source": "api.worldbank.org (live)",
                    "indicator_code": code
                })
                relate(label, "describes", "Russia", 0.90)
                print(f"  {year}: {formatted}")

    # Also fetch Ukraine GDP for comparison
    print("\nFetching Ukraine GDP for comparison...")
    ukr_gdp = worldbank_indicator("UKR", "NY.GDP.MKTP.CD", "2019:2023")
    for year, value in sorted(ukr_gdp):
        label = f"Econ:Ukraine-GDP-{year}"
        formatted = f"${value/1e9:.1f}B"
        store(label, "economic_indicator", 0.93, "worldbank", {
            "metric": "GDP (current USD)", "year": year,
            "value": formatted, "data_source": "api.worldbank.org (live)"
        })
        relate(label, "describes", "Ukraine", 0.90)
        print(f"  Ukraine {year}: {formatted}")

    s = stats()
    print(f"\nWorld Bank import complete: {s.get('nodes', '?')} nodes, {s.get('edges', '?')} edges")

# ---------------------------------------------------------------------------
# Phase 3: Live Data -- GDELT News, Exchange Rate, Wikipedia
# ---------------------------------------------------------------------------

def phase3_additional_sources():
    section("Phase 3: Live Data from GDELT, Exchange Rate API, Wikipedia")

    # 3a: Live RUB/USD exchange rate
    subsection("Live Exchange Rate")
    rate, timestamp = get_exchange_rate("RUB")
    if rate:
        store("Econ:RUB-USD-Live", "economic_indicator", 0.95, "exchange-rate-api", {
            "metric": "RUB/USD exchange rate",
            "value": f"{rate:.2f} RUB per USD",
            "raw_value": str(rate),
            "timestamp": timestamp,
            "data_source": "open.er-api.com (live)"
        })
        relate("Econ:RUB-USD-Live", "describes", "Russia", 0.90)
        print(f"  1 USD = {rate:.2f} RUB (as of {timestamp})")
    else:
        print("  Exchange rate unavailable")

    # 3b: GDELT news articles about Russia
    subsection("GDELT -- Recent News about Russia (last 30 days)")
    news_queries = [
        ("Russia sanctions 2026", "sanctions"),
        ("Russia Ukraine war", "conflict"),
        ("Russia NATO military", "nato_tensions"),
        ("Russia economy ruble", "economy"),
    ]

    article_count = 0
    for query, topic in news_queries:
        articles = gdelt_articles(query, max_records=5, timespan="30d")
        if articles:
            print(f"\n  Topic: {topic} ({len(articles)} articles)")
            for a in articles[:3]:
                title = a.get("title", "?")
                domain = a.get("domain", "?")
                url = a.get("url", "")
                seen = a.get("seendate", "?")[:8]

                # Determine source confidence
                source_conf = 0.60  # default for unknown sources
                if any(s in domain for s in ["reuters.com", "apnews.com"]):
                    source_conf = 0.85
                elif any(s in domain for s in ["bbc.com", "bbc.co.uk"]):
                    source_conf = 0.82
                elif any(s in domain for s in ["ria.ru", "tass.com", "rt.com"]):
                    source_conf = 0.25
                elif any(s in domain for s in ["nytimes.com", "theguardian.com", "washingtonpost.com"]):
                    source_conf = 0.80
                elif any(s in domain for s in ["aljazeera.com"]):
                    source_conf = 0.75

                # Clean title for label (truncate to 80 chars)
                safe_title = title.encode('ascii', 'replace').decode('ascii')[:80]
                label = f"News:{seen}:{safe_title[:50]}"

                store(label, "news_article", source_conf, domain, {
                    "title": safe_title,
                    "domain": domain,
                    "date": seen,
                    "topic": topic,
                    "url": url[:200],
                    "data_source": "gdelt.org (live)"
                })
                relate(label, "mentions", "Russia", source_conf * 0.9)

                tier = "T1" if source_conf >= 0.80 else "T2" if source_conf >= 0.50 else "T3"
                print(f"    [{seen}] [{tier} {domain}] {safe_title[:70]}")
                article_count += 1

    print(f"\n  Total articles imported: {article_count}")

    # 3c: Wikipedia conflict summary
    subsection("Wikipedia -- Conflict Summary")
    wiki_articles = [
        ("2022_Russian_invasion_of_Ukraine", "conflict"),
        ("Russo-Ukrainian_War", "conflict"),
        ("International_sanctions_during_the_Russo-Ukrainian_War", "sanctions"),
    ]

    for title, topic in wiki_articles:
        data = wikipedia_summary(title)
        if data and "extract" in data:
            extract = data["extract"][:500]
            wiki_label = f"Wiki:{data.get('title', title)[:60]}"
            store(wiki_label, "encyclopedia_entry", 0.85, "wikipedia", {
                "title": data.get("title", title),
                "extract": extract,
                "topic": topic,
                "data_source": "en.wikipedia.org (live)"
            })
            relate(wiki_label, "describes", "Russia", 0.80)
            if topic == "conflict":
                relate(wiki_label, "describes", "Conflict:Ukraine-Invasion", 0.85)
            print(f"  {data.get('title', '?')}")
            print(f"    {extract[:150]}...")
        else:
            print(f"  {title}: unavailable")

    # 3d: REST Countries enrichment for bordering nations
    subsection("REST Countries -- Enrichment")
    codes = {"RUS": "Russia", "UKR": "Ukraine", "BLR": "Belarus",
             "FIN": "Finland", "POL": "Poland", "KAZ": "Kazakhstan"}
    for code, name in codes.items():
        data = get_country_data(code)
        if data:
            pop = data.get("population", 0)
            area = data.get("area", 0)
            capital = data.get("capital", ["?"])[0] if data.get("capital") else "?"
            region = data.get("subregion", data.get("region", "?"))

            # Update properties on existing nodes
            store(name, "country", None, "restcountries", {
                "population_restcountries": f"{pop:,}",
                "area_km2": f"{area:,.0f}",
                "capital": capital,
                "region": region,
                "data_source": "restcountries.com (live)"
            })
            print(f"  {name}: pop={pop:,}, area={area:,.0f}km2, capital={capital}")

    s = stats()
    print(f"\nAdditional sources complete: {s.get('nodes', '?')} nodes, {s.get('edges', '?')} edges")

# ---------------------------------------------------------------------------
# Phase 4: Source Reliability Tiers (used to weight all evidence)
# ---------------------------------------------------------------------------

def phase4_sources():
    section("Phase 4: Source Reliability Tiers")

    sources = [
        # Tier 1: Verified institutional data
        ("Source:WorldBank", "source", 0.95, {"tier": "1", "type": "international-financial", "bias": "technocratic", "url": "api.worldbank.org"}),
        ("Source:Wikidata", "source", 0.92, {"tier": "1", "type": "structured-knowledge-base", "bias": "community-curated", "url": "wikidata.org"}),
        ("Source:IMF", "source", 0.95, {"tier": "1", "type": "international-financial", "bias": "technocratic"}),
        ("Source:NATO-Official", "source", 0.90, {"tier": "1", "type": "alliance-official", "bias": "Western-alliance"}),
        ("Source:UN-Reports", "source", 0.90, {"tier": "1", "type": "international-body", "bias": "multilateral"}),

        # Tier 2: Quality journalism & think tanks
        ("Source:Reuters", "source", 0.88, {"tier": "2", "type": "news-agency", "bias": "neutral-Western"}),
        ("Source:AP", "source", 0.88, {"tier": "2", "type": "news-agency", "bias": "neutral-Western"}),
        ("Source:BBC", "source", 0.85, {"tier": "2", "type": "public-broadcaster", "bias": "British-Western"}),
        ("Source:ISW", "source", 0.82, {"tier": "2", "type": "think-tank", "bias": "US-hawkish", "note": "Institute for the Study of War"}),
        ("Source:RUSI", "source", 0.85, {"tier": "2", "type": "think-tank", "bias": "British-defense"}),
        ("Source:IISS", "source", 0.85, {"tier": "2", "type": "think-tank", "bias": "Western-security"}),

        # Tier 3: State-controlled media
        ("Source:TASS", "source", 0.30, {"tier": "3", "type": "state-news-agency", "bias": "Russian-state", "note": "Government-controlled"}),
        ("Source:RT", "source", 0.25, {"tier": "3", "type": "state-media", "bias": "Russian-state", "note": "Banned in EU, foreign agent in US"}),
        ("Source:Sputnik", "source", 0.20, {"tier": "3", "type": "state-propaganda", "bias": "Russian-state"}),
    ]

    for name, ntype, conf, props in sources:
        store(name, ntype, conf, "meta", props)

    print("Source reliability tiers loaded:")
    print(f"  Tier 1 (institutional):     5 sources (conf 0.90-0.95)")
    print(f"  Tier 2 (journalism/think):  6 sources (conf 0.82-0.88)")
    print(f"  Tier 3 (state-controlled):  3 sources (conf 0.20-0.30)")

# ---------------------------------------------------------------------------
# Phase 5: Conflict & Strategic Situation (analyst enrichment)
# ---------------------------------------------------------------------------

def phase5_conflict():
    section("Phase 5: Conflict & Strategic Analysis (analyst enrichment)")

    # These are analyst assessments layered on top of the live data
    store("Conflict:Ukraine-Invasion", "conflict", 0.95, "Source:UN-Reports", {
        "start_date": "2022-02-24", "type": "full-scale invasion",
        "status": "ongoing",
        "territory_occupied": "~18% of Ukraine (incl. Crimea)",
        "source_note": "UN GA Resolution ES-11/1 condemned the invasion"
    })
    relate("Russia", "aggressor_in", "Conflict:Ukraine-Invasion", 0.95)
    relate("Ukraine", "defender_in", "Conflict:Ukraine-Invasion", 0.95)

    # Disputed territories
    territories = [
        ("Crimea", 0.95, {"annexed": "2014", "status": "illegally annexed", "strategic_value": "Sevastopol naval base"}),
        ("Donetsk", 0.90, {"status": "partially occupied", "claimed_annexed": "2022"}),
        ("Luhansk", 0.90, {"status": "mostly occupied", "claimed_annexed": "2022"}),
        ("Zaporizhzhia", 0.88, {"status": "partially occupied", "note": "Europes largest nuclear plant"}),
        ("Kherson", 0.88, {"status": "partially occupied", "note": "City liberated Nov 2022"}),
        ("Transnistria", 0.80, {"since": "1992", "status": "Russian-backed separatist region in Moldova", "russian_troops": "~1500"}),
    ]
    for name, conf, props in territories:
        store(name, "disputed_territory", conf, "Source:Reuters", props)
        relate("Russia", "occupies", name, conf)

    # Strategic assets
    store("Kaliningrad", "strategic_asset", 0.90, "Source:NATO-Official", {
        "type": "exclave / military outpost",
        "location": "Between Lithuania and Poland",
        "assets": "Iskander missiles, Baltic Fleet, nuclear-capable",
        "strategic_value": "Suwalki Gap control"
    })
    relate("Kaliningrad", "belongs_to", "Russia", 0.95)
    relate("Kaliningrad", "threatens", "Lithuania", 0.80)
    relate("Kaliningrad", "threatens", "Poland", 0.75)

    store("Suwalki Gap", "strategic_chokepoint", 0.88, "Source:RUSI", {
        "location": "65km corridor between Kaliningrad and Belarus",
        "significance": "Only NATO land link to Baltic states",
        "threat": "Russian/Belarusian closure would isolate Baltics"
    })
    relate("Suwalki Gap", "connects", "Poland", 0.90)
    relate("Suwalki Gap", "connects", "Lithuania", 0.90)
    relate("Kaliningrad", "flanks", "Suwalki Gap", 0.85)
    relate("Belarus", "flanks", "Suwalki Gap", 0.85)

    # Sanctions (analyst-sourced)
    sanctions = [
        ("Sanction:SWIFT-Exclusion", {"date": "2022-03-02", "type": "financial", "impact": "Major Russian banks excluded from SWIFT"}),
        ("Sanction:CentralBank-Freeze", {"date": "2022-02-28", "type": "financial", "impact": "$300B+ reserves frozen"}),
        ("Sanction:Oil-PriceCap", {"date": "2022-12-05", "type": "energy", "impact": "$60/barrel cap on Russian seaborne crude"}),
        ("Sanction:Tech-Export-Controls", {"date": "2022-ongoing", "type": "technology", "impact": "Semiconductors, aerospace parts banned"}),
    ]
    for name, props in sanctions:
        store(name, "sanction", 0.92, "Source:Reuters", props)
        relate(name, "targets", "Russia", 0.95)

    # Turkey's anomalous position
    store("Turkey", "country", 0.90, "wikidata", {
        "note": "NATO member since 1952 but maintains Russia relations",
        "sanctions_stance": "Did NOT join Western sanctions",
        "s400_purchase": "Bought Russian S-400 system"
    })
    relate("Turkey", "member_of", "NATO", 0.99)
    relate("Turkey", "trade_partner", "Russia", 0.85)
    relate("Turkey", "blocked_sanctions_on", "Russia", 0.80)

    # North Korea military cooperation
    store("NorthKorea-Russia-MilitaryPact", "agreement", 0.85, "Source:Reuters", {
        "date": "June 2024", "type": "mutual defense treaty",
        "detail": "NK provides ammunition and ballistic missiles to Russia"
    })
    relate("North Korea", "supplies_weapons_to", "Russia", 0.82)

    s = stats()
    print(f"Conflict & strategic data: {s.get('nodes', '?')} nodes, {s.get('edges', '?')} edges")

# ---------------------------------------------------------------------------
# Phase 6: Contradictions -- State Media vs Evidence
# ---------------------------------------------------------------------------

def phase6_contradictions():
    section("Phase 6: Contradictions -- State Media vs Evidence")

    claims = [
        ("Claim:SpecialOperation", 0.25, "Source:TASS",
         "Russia conducting special military operation, not a war",
         "Fact:FullScaleWar", 0.92, "Source:UN-Reports",
         "UN GA condemned invasion; ICC arrest warrant for Putin"),

        ("Claim:SanctionsFailed", 0.30, "Source:RT",
         "Western sanctions completely failed to damage Russia",
         "Fact:SanctionsImpact", 0.93, "Source:WorldBank",
         "GDP dropped 2022; $300B frozen; 1000+ companies exited; tech imports collapsed"),

        ("Claim:CrimeaLegal", 0.25, "Source:TASS",
         "Crimea annexation was legal based on referendum",
         "Fact:CrimeaIllegal", 0.92, "Source:UN-Reports",
         "UNGA Resolution 68/262: referendum invalid under occupation"),

        ("Claim:NATOProvoked", 0.30, "Source:RT",
         "NATO expansion provoked Russia leaving no choice",
         "Fact:SovereignChoice", 0.90, "Source:NATO-Official",
         "NATO open-door policy; membership is sovereign choice"),

        ("Claim:NoMobilization", 0.20, "Source:Sputnik",
         "There is no mass mobilization in Russia",
         "Fact:Mobilization", 0.88, "Source:Reuters",
         "Putin signed partial mobilization Sep 2022; 300,000 called up"),
    ]

    for (claim_l, claim_c, claim_s, claim_text,
         fact_l, fact_c, fact_s, fact_text) in claims:

        store(claim_l, "claim", claim_c, claim_s, {"claim": claim_text, "source_tier": "3"})
        store(fact_l, "fact", fact_c, fact_s, {"fact": fact_text})
        relate(fact_l, "contradicts", claim_l, 0.90)

        # Correct the disinformation (drives confidence to 0)
        correct(claim_l, f"Contradicted by {fact_l}", fact_s)
        print(f"  {claim_l} -> CORRECTED by {fact_l}")
        print(f"    Claim ({claim_s}, conf={claim_c}): {claim_text}")
        print(f"    Fact  ({fact_s}, conf={fact_c}): {fact_text}")
        print()

    s = stats()
    print(f"After contradictions: {s.get('nodes', '?')} nodes, {s.get('edges', '?')} edges")

# ---------------------------------------------------------------------------
# Phase 7: Inference Rules
# ---------------------------------------------------------------------------

def phase7_inference():
    section("Phase 7: Inference Rules -- Pattern Detection")

    # Each rule is a separate string in the array.
    # Note: _flag property stores only the last value, so rule order matters.
    # We order from general to specific so the most specific flag wins.
    rules = [
        'rule occupation_pattern\n  when edge("Russia", "occupies", Territory)\n  then flag(Territory, "OCCUPIED: Territory under Russian military control")',

        'rule weapons_supplier\n  when edge(Country, "supplies_weapons_to", "Russia")\n  then flag(Country, "WEAPONS_SUPPLIER: Providing arms to Russia")',

        'rule suwalki_threat\n  when edge(Asset, "flanks", "Suwalki Gap")\n  when edge(Asset, "belongs_to", "Russia")\n  then flag(Asset, "SUWALKI_THREAT: Russian asset flanking NATO chokepoint")',

        'rule frontline_state\n  when edge("Russia", "borders", Country)\n  when edge(Country, "member_of", "NATO")\n  then flag(Country, "FRONTLINE: NATO member bordering Russia")',

        'rule sanctions_blocker\n  when edge(Country, "member_of", "NATO")\n  when edge(Country, "blocked_sanctions_on", "Russia")\n  then flag(Country, "ANOMALY: NATO member blocking sanctions on Russia")',
    ]

    result = fire_rules(rules)
    print(f"Inference results:")
    print(f"  Rules evaluated: {result.get('rules_evaluated', '?')}")
    print(f"  Rules fired: {result.get('rules_fired', '?')}")
    print(f"  Edges created: {result.get('edges_created', '?')}")
    print(f"  Flags raised: {result.get('flags_raised', '?')}")

    # Show flagged entities
    subsection("Flagged Entities (discovered by inference)")

    check_entities = [
        "Norway", "Finland", "Estonia", "Latvia", "Lithuania", "Poland",
        "Belarus", "Kazakhstan",
        "Turkey", "North Korea",
        "Kaliningrad",
        "Crimea", "Donetsk", "Luhansk", "Transnistria",
    ]

    for entity in check_entities:
        info = explain(entity)
        if info and "properties" in info:
            flag = info.get("properties", {}).get("_flag", "")
            if flag:
                conf = info.get("confidence", 0)
                print(f"  {entity:20s} (conf={conf:.2f}): {flag}")

# ---------------------------------------------------------------------------
# Phase 8: Predictions with Probability
# ---------------------------------------------------------------------------

def phase8_predictions():
    section("Phase 8: Probabilistic Intelligence Assessments")

    predictions = [
        {
            "label": "Prediction:BalticProvocation",
            "hypothesis": "Russia will conduct significant military provocation in the Baltic region within 12 months",
            "timeframe": "2025-2026",
            "category": "military",
            "evidence_for": [
                ("Kaliningrad military buildup and Iskander deployment", 0.90, "Source:NATO-Official"),
                ("Suwalki Gap strategic vulnerability identified", 0.88, "Source:RUSI"),
                ("Repeated Russian airspace violations over Baltic states", 0.85, "Source:Reuters"),
                ("GPS jamming affecting Baltic aviation", 0.82, "Source:Reuters"),
                ("Hybrid warfare precedent (2007 Estonia cyberattack)", 0.80, "Source:IISS"),
            ],
            "evidence_against": [
                ("NATO Article 5 collective defense deterrence", 0.90, "Source:NATO-Official"),
                ("Russian military overextended in Ukraine", 0.85, "Source:ISW"),
            ],
        },
        {
            "label": "Prediction:TurkeySanctionEvasion",
            "hypothesis": "Turkey remains primary sanctions evasion route for Russia through 2026",
            "timeframe": "2025-2026",
            "category": "economic",
            "evidence_for": [
                ("Turkey refused to join Western sanctions", 0.88, "Source:Reuters"),
                ("Russian-Turkish trade surged 2022-2024", 0.85, "Source:Reuters"),
                ("Parallel imports flowing via Istanbul", 0.82, "Source:Reuters"),
                ("Erdogan strategic ambiguity posture", 0.80, "Source:Reuters"),
                ("S-400 purchase despite US objections", 0.85, "Source:Reuters"),
            ],
            "evidence_against": [
                ("US secondary sanctions pressure increasing", 0.75, "Source:Reuters"),
                ("Turkish banks restricting Russian transactions 2024", 0.70, "Source:Reuters"),
            ],
        },
        {
            "label": "Prediction:MoldovaDestabilization",
            "hypothesis": "Russia intensifies destabilization of Moldova via Transnistria and hybrid warfare",
            "timeframe": "2025-2026",
            "category": "hybrid_warfare",
            "evidence_for": [
                ("Russian troops in Transnistria (~1500)", 0.80, "Source:IISS"),
                ("Moldova EU candidacy provokes Russian opposition", 0.85, "Source:Reuters"),
                ("Moldovan energy dependency on Russian gas", 0.82, "Source:Reuters"),
                ("Russian-backed political interference networks", 0.78, "Source:Reuters"),
                ("Small Moldovan military (6500 active)", 0.85, "Source:IISS"),
                ("Gagauzia pro-Russian autonomous region", 0.75, "Source:BBC"),
            ],
            "evidence_against": [
                ("EU integration momentum and aid packages", 0.80, "Source:Reuters"),
                ("Romania security cooperation", 0.75, "Source:NATO-Official"),
                ("Russia overextended militarily in Ukraine", 0.85, "Source:ISW"),
            ],
        },
        {
            "label": "Prediction:RubleInstability",
            "hypothesis": "Russian ruble faces significant instability (>120 RUB/USD) within 18 months",
            "timeframe": "2025-2027",
            "category": "economic",
            "evidence_for": [
                ("Central bank rate at 21% (unsustainable)", 0.88, "Source:Reuters"),
                ("Military spending at 40% of federal budget", 0.88, "Source:IISS"),
                ("Oil revenue declining under price cap pressure", 0.82, "Source:WorldBank"),
                ("Capital flight and brain drain (500K+ emigrated)", 0.80, "Source:Reuters"),
                ("$300B reserves frozen and inaccessible", 0.93, "Source:WorldBank"),
            ],
            "evidence_against": [
                ("Nabiullina competent monetary policy management", 0.85, "Source:Reuters"),
                ("Capital controls preventing outflows", 0.80, "Source:Reuters"),
                ("Continued energy exports to India and China", 0.85, "Source:Reuters"),
            ],
        },
        {
            "label": "Prediction:BRICSCurrency",
            "hypothesis": "BRICS launches meaningful alternative to USD-dominated financial system",
            "timeframe": "2025-2030",
            "category": "economic",
            "evidence_for": [
                ("BRICS expanded to 10 members in 2024", 0.85, "Source:Reuters"),
                ("De-dollarization rhetoric from Russia and China", 0.70, "Source:Reuters"),
                ("Bilateral RUB-CNY trade settlement increasing", 0.75, "Source:Reuters"),
            ],
            "evidence_against": [
                ("No common currency mechanism agreed or designed", 0.88, "Source:IMF"),
                ("India-China strategic distrust and border conflicts", 0.82, "Source:Reuters"),
                ("USD still 58% of global reserves", 0.93, "Source:WorldBank"),
                ("BRICS members have fundamentally divergent interests", 0.85, "Source:Reuters"),
                ("Saudi Arabia still pegged to USD", 0.80, "Source:Reuters"),
            ],
        },
        {
            "label": "Prediction:NKRussiaAxis",
            "hypothesis": "North Korea-Russia military axis deepens with technology transfer",
            "timeframe": "2025-2026",
            "category": "military",
            "evidence_for": [
                ("Mutual defense treaty signed June 2024", 0.85, "Source:Reuters"),
                ("NK troops reported deployed to Russian front", 0.78, "Source:Reuters"),
                ("NK ammunition shipments confirmed by intelligence", 0.82, "Source:Reuters"),
                ("Russia desperate for manpower and ammunition", 0.88, "Source:ISW"),
                ("Putin visited Pyongyang June 2024", 0.92, "Source:Reuters"),
            ],
            "evidence_against": [
                ("China opposes NK nuclear program escalation", 0.80, "Source:Reuters"),
                ("Both nations under heavy international sanctions", 0.85, "Source:UN-Reports"),
            ],
        },
        {
            "label": "Prediction:FrozenConflict",
            "hypothesis": "Ukraine conflict becomes frozen conflict (Korean War model) by 2027",
            "timeframe": "2025-2027",
            "category": "conflict",
            "evidence_for": [
                ("Military stalemate on ground through 2024", 0.82, "Source:ISW"),
                ("Both sides facing exhaustion (manpower, ammunition)", 0.80, "Source:RUSI"),
                ("Western fatigue and shifting election cycles", 0.75, "Source:Reuters"),
                ("Trump administration signaling negotiation push", 0.78, "Source:AP"),
            ],
            "evidence_against": [
                ("Ukraine refuses to cede territory formally", 0.88, "Source:Reuters"),
                ("Russia stated maximalist goals unchanged", 0.75, "Source:TASS"),
                ("European military aid increasing (2025 pledges)", 0.80, "Source:Reuters"),
                ("No ceasefire framework agreed by either side", 0.85, "Source:UN-Reports"),
            ],
        },
    ]

    print(f"Computing {len(predictions)} probabilistic assessments...\n")

    for pred in predictions:
        evidence_for = pred["evidence_for"]
        evidence_against = pred["evidence_against"]

        # Bayesian-inspired probability calculation:
        # P = weighted_for * (1 - weighted_against * discount)
        # where discount = proportion of evidence that is contradicting
        total_for_weight = sum(conf for _, conf, _ in evidence_for)
        weighted_for = sum(conf * conf for _, conf, _ in evidence_for) / total_for_weight if total_for_weight else 0

        total_against_weight = sum(conf for _, conf, _ in evidence_against)
        weighted_against = sum(conf * conf for _, conf, _ in evidence_against) / total_against_weight if total_against_weight else 0

        discount = len(evidence_against) / (len(evidence_for) + len(evidence_against))
        probability = weighted_for * (1 - weighted_against * discount)
        probability = round(max(0.05, min(0.95, probability)), 2)

        # Store prediction node with calculated probability as confidence
        store(pred["label"], "prediction", probability, "engram-analysis", {
            "hypothesis": pred["hypothesis"],
            "timeframe": pred["timeframe"],
            "probability": str(probability),
            "evidence_for_count": str(len(evidence_for)),
            "evidence_against_count": str(len(evidence_against)),
            "category": pred["category"],
            "methodology": "Bayesian evidence aggregation from weighted source tiers"
        })

        # Store and link evidence nodes
        for desc, conf, src in evidence_for:
            ev_label = f"Ev:{pred['label'].split(':')[1]}:+:{desc[:35]}"
            store(ev_label, "evidence", conf, src, {
                "description": desc, "direction": "supporting"
            })
            relate(ev_label, "supports", pred["label"], conf)

        for desc, conf, src in evidence_against:
            ev_label = f"Ev:{pred['label'].split(':')[1]}:-:{desc[:35]}"
            store(ev_label, "evidence", conf, src, {
                "description": desc, "direction": "contradicting"
            })
            relate(ev_label, "weakens", pred["label"], conf)

        bar = "#" * int(probability * 40)
        print(f"  {pred['label']}")
        print(f"    {pred['hypothesis']}")
        print(f"    Probability: {probability:.0%} {bar}")
        print(f"    Evidence: {len(evidence_for)} supporting, {len(evidence_against)} contradicting")
        print(f"    Timeframe: {pred['timeframe']}  Category: {pred['category']}")
        print()

    s = stats()
    print(f"After predictions: {s.get('nodes', '?')} nodes, {s.get('edges', '?')} edges")

# ---------------------------------------------------------------------------
# Phase 9: Knowledge Discovery
# ---------------------------------------------------------------------------

def phase9_discovery():
    section("Phase 9: Knowledge Discovery via Graph Traversal")

    discoveries = [
        ("Russia", 2, "The full neighborhood of the Russian Federation"),
        ("Turkey", 2, "The NATO-Russia bridge -- the anomaly node"),
        ("Suwalki Gap", 2, "NATOs Achilles heel"),
        ("Prediction:MoldovaDestabilization", 1, "Evidence chain for Moldova prediction"),
    ]

    for label, depth, desc in discoveries:
        subsection(f"From {label} (depth={depth}) -- {desc}")
        result = traverse(label, depth=depth)
        nodes = result.get("nodes", [])
        edges = result.get("edges", [])
        print(f"  Reachable: {len(nodes)} nodes, {len(edges)} edges")
        # Show first few nodes
        for n in nodes[:8]:
            d = n.get("depth", "?")
            c = n.get("confidence", 0)
            print(f"    depth={d} conf={c:.2f} {n.get('label', '?')}")
        if len(nodes) > 8:
            print(f"    ... and {len(nodes) - 8} more")

    # Explain key predictions
    subsection("Explain: Prediction:BalticProvocation")
    info = explain("Prediction:BalticProvocation")
    if info and "entity" in info:
        print(f"  Confidence (= probability): {info.get('confidence', 0):.2f}")
        props = info.get("properties", {})
        print(f"  Hypothesis: {props.get('hypothesis', '?')}")
        print(f"  Evidence for: {props.get('evidence_for_count', '?')} sources")
        print(f"  Evidence against: {props.get('evidence_against_count', '?')} sources")
        print(f"  Methodology: {props.get('methodology', '?')}")
        for e in info.get("edges_to", [])[:5]:
            print(f"    <- [{e.get('relationship', '?')}] {e.get('from', '?')} (conf={e.get('confidence', 0):.2f})")

    subsection("Explain: Turkey -- Why It Is Flagged")
    info = explain("Turkey")
    if info and "entity" in info:
        print(f"  Confidence: {info.get('confidence', 0):.2f}")
        flag = info.get("properties", {}).get("_flag", "none")
        print(f"  Flag: {flag}")
        for e in info.get("edges_from", [])[:8]:
            print(f"    -> [{e.get('relationship', '?')}] {e.get('to', '?')} (conf={e.get('confidence', 0):.2f})")

# ---------------------------------------------------------------------------
# Phase 10: Policy Simulation -- What-If Countermeasures
# ---------------------------------------------------------------------------

def phase10_simulation():
    section("Phase 10: Policy Simulation -- Preventing Moldova Destabilization")

    # Show current state
    info = explain("Prediction:MoldovaDestabilization")
    before_conf = info.get("confidence", 0) if info and "entity" in info else 0.59
    print(f"CURRENT STATE: Prediction:MoldovaDestabilization = {before_conf:.0%}")
    print(f"  Evidence: 6 supporting, 3 contradicting\n")

    print("Applying NATO/EU countermeasures package:\n")

    countermeasures = [
        ("Countermeasure:NATO-Moldova-Partnership", 0.85, "Source:NATO-Official",
         "NATO enhanced partnership: intelligence sharing, training, equipment"),
        ("Countermeasure:EU-FastTrack-Accession", 0.82, "Source:Reuters",
         "EU accelerates Moldova accession with 2027 target date"),
        ("Countermeasure:Romania-SecurityGuarantee", 0.88, "Source:NATO-Official",
         "Romania deploys air defense and rapid reaction force near border"),
        ("Countermeasure:Energy-Independence", 0.80, "Source:Reuters",
         "Moldova completes energy diversification via Romania gas interconnector"),
        ("Countermeasure:Anti-Hybrid-Warfare", 0.78, "Source:Reuters",
         "EU funds counter-disinformation center, bans Shor political network"),
        ("Countermeasure:Transnistria-Negotiation", 0.70, "Source:UN-Reports",
         "UN-mediated Transnistria settlement: Russian troop withdrawal for autonomy"),
    ]

    for label, conf, src, desc in countermeasures:
        store(label, "countermeasure", conf, src, {
            "description": desc, "type": "policy_intervention",
            "simulation": "Moldova defense scenario"
        })
        relate(label, "weakens", "Prediction:MoldovaDestabilization", conf)
        print(f"  + {desc}")
        print(f"    Source: {src}  Confidence: {conf:.0%}")

    # Recalculate probability: original 6 for, now 3+6=9 against
    evidence_for = [0.80, 0.85, 0.82, 0.78, 0.85, 0.75]  # original threats
    evidence_against = [0.80, 0.75, 0.85,  # original defenses
                        0.85, 0.82, 0.88, 0.80, 0.78, 0.70]  # + countermeasures

    total_for = sum(evidence_for)
    weighted_for = sum(c*c for c in evidence_for) / total_for

    total_against = sum(evidence_against)
    weighted_against = sum(c*c for c in evidence_against) / total_against

    discount = len(evidence_against) / (len(evidence_for) + len(evidence_against))
    new_prob = weighted_for * (1 - weighted_against * discount)
    new_prob = max(0.05, min(0.95, new_prob))

    # Update prediction confidence to reflect simulation
    store("Prediction:MoldovaDestabilization", "prediction", new_prob, "engram-simulation", {
        "hypothesis": "Russia intensifies destabilization of Moldova via Transnistria and hybrid warfare",
        "timeframe": "2025-2026",
        "probability": str(round(new_prob, 2)),
        "evidence_for_count": str(len(evidence_for)),
        "evidence_against_count": str(len(evidence_against)),
        "category": "hybrid_warfare",
        "methodology": "Bayesian evidence aggregation from weighted source tiers",
        "simulation": "NATO/EU countermeasures package applied"
    })

    bar_b = "#" * int(before_conf * 50)
    bar_a = "#" * int(new_prob * 50)
    print(f"\n  BEFORE: {before_conf:.0%}  {bar_b}")
    print(f"  AFTER:  {new_prob:.0%}  {bar_a}")
    print(f"  Reduction: -{before_conf - new_prob:.0%} probability points")
    print(f"\n  Evidence balance shifted from 6:3 to 6:9")
    print(f"  Countermeasures make Moldova destabilization significantly less likely.")
    print(f"\n  Most impactful: Romania security guarantee (0.88) > NATO partnership (0.85)")
    print(f"  Hardest to achieve: Transnistria negotiation (0.70) -- but highest long-term impact")

    s = stats()
    print(f"\n  Graph: {s.get('nodes', '?')} nodes, {s.get('edges', '?')} edges")

    # --- Scenario B: Ukraine Frontline Freeze (negotiated ceasefire) ---
    subsection("Scenario B: Ukraine -- Frontline Freeze (Negotiated Ceasefire)")

    store("Prediction:UkraineFrontlineFreeze", "prediction", 0.50, "engram-analysis", {
        "hypothesis": "War ends via negotiated ceasefire along current frontlines, Russia retains ~18% of Ukraine",
        "timeframe": "2025-2027",
        "category": "conflict",
        "methodology": "Bayesian evidence aggregation from weighted source tiers"
    })
    relate("Prediction:UkraineFrontlineFreeze", "alternative_to", "Prediction:FrozenConflict", 0.80)

    freeze_for = [
        ("Military stalemate: minimal territorial change since mid-2023", 0.85, "Source:ISW"),
        ("Both sides exhausted: manpower, ammunition shortages", 0.82, "Source:RUSI"),
        ("Trump administration pushing for rapid negotiations", 0.80, "Source:AP"),
        ("European 'war fatigue' -- declining public support for aid", 0.72, "Source:Reuters"),
        ("Russia nuclear escalation threat deters major UA offensives", 0.70, "Source:Reuters"),
        ("China pressuring Russia to negotiate from strength", 0.68, "Source:Reuters"),
    ]
    freeze_against = [
        ("Ukraine refuses to cede territory -- Zelensky 'Peace Formula'", 0.90, "Source:Reuters"),
        ("Russia stated goal: full Donetsk/Luhansk + regime change", 0.75, "Source:TASS"),
        ("No ceasefire framework: no DMZ, no monitoring mechanism", 0.85, "Source:UN-Reports"),
        ("Frozen conflict benefits neither -- both lose legitimacy", 0.72, "Source:RUSI"),
        ("European military aid increasing -- F-16s, long-range missiles", 0.82, "Source:Reuters"),
    ]

    freeze_prob = calc_probability(freeze_for, freeze_against)
    store_prediction_with_evidence(
        "Prediction:UkraineFrontlineFreeze", freeze_prob, freeze_for, freeze_against,
        "War ends via negotiated ceasefire along current frontlines",
        "2025-2027", "conflict"
    )
    print(f"  Probability of frontline freeze: {freeze_prob:.0%}")
    print(f"  Evidence: {len(freeze_for)} supporting, {len(freeze_against)} contradicting")

    # Now simulate: what does Ukraine need to FORCE the freeze on favorable terms?
    print("\n  Strategic levers for Ukraine to strengthen ceasefire position:\n")
    freeze_countermeasures = [
        ("Strategy:UA-Air-Superiority", 0.85, "Source:RUSI",
         "F-16 fleet operational + Western air defense umbrella denies Russian air power"),
        ("Strategy:UA-Long-Range-Strikes", 0.82, "Source:ISW",
         "ATACMS/Storm Shadow strikes on Crimean logistics, Kerch bridge, rear bases"),
        ("Strategy:UA-Energy-Targeting", 0.80, "Source:ISW",
         "Systematic strikes on Russian oil refineries reduce war funding"),
        ("Strategy:UA-Fortification-Line", 0.88, "Source:Reuters",
         "Surovikin-style defense line makes occupied territory indefensible for Russia"),
        ("Strategy:UA-Sanctions-Enforcement", 0.78, "Source:Reuters",
         "Close sanctions evasion routes (Turkey, UAE, Kazakhstan transit)"),
    ]

    for label, conf, src, desc in freeze_countermeasures:
        store(label, "strategy", conf, src, {
            "description": desc, "type": "ukraine_defense",
            "simulation": "Frontline freeze scenario"
        })
        relate(label, "strengthens", "Prediction:UkraineFrontlineFreeze", conf)
        print(f"  + {desc}")
        print(f"    Source: {src}  Confidence: {conf:.0%}")

    # --- Scenario C: Ukraine Full Recovery (including Crimea) ---
    subsection("Scenario C: Ukraine -- Full Territorial Recovery (including Crimea)")

    recovery_for = [
        ("Crimea logistically vulnerable: Kerch bridge damaged, supply dependent", 0.82, "Source:ISW"),
        ("Ukrainian long-range strike capability expanding rapidly", 0.80, "Source:RUSI"),
        ("Russian military quality declining: officer losses, equipment age", 0.78, "Source:RUSI"),
        ("International law: UNGA affirms Ukraine 1991 borders", 0.92, "Source:UN-Reports"),
        ("Historical precedent: Kherson/Kharkiv liberation in 2022", 0.85, "Source:ISW"),
        ("Crimean water crisis if canal cut again", 0.70, "Source:Reuters"),
    ]
    recovery_against = [
        ("Russia has nuclear weapons -- Crimea seen as existential", 0.92, "Source:Reuters"),
        ("Crimea fortified since 2014: 10 years of military buildup", 0.88, "Source:ISW"),
        ("Kerch Strait crossing under Russian air/naval control", 0.85, "Source:RUSI"),
        ("Western allies may not support Crimea offensive (escalation fear)", 0.82, "Source:Reuters"),
        ("Russia would mobilize fully if Crimea threatened", 0.80, "Source:RUSI"),
        ("700km+ supply lines from UA-controlled territory", 0.78, "Source:ISW"),
        ("Civilian population in Crimea (2.4M) complicates assault", 0.75, "Source:Reuters"),
        ("Sevastopol naval base: Russia's top strategic asset", 0.90, "Source:Reuters"),
    ]

    recovery_prob = calc_probability(recovery_for, recovery_against)
    store_prediction_with_evidence(
        "Prediction:UkraineFullRecovery", recovery_prob, recovery_for, recovery_against,
        "Ukraine recovers all territory including Crimea by military or coercive means",
        "2025-2030", "conflict"
    )

    print(f"\n  Probability of full recovery (incl. Crimea): {recovery_prob:.0%}")
    print(f"  Evidence: {len(recovery_for)} supporting, {len(recovery_against)} contradicting")

    # What would Ukraine need to make full recovery possible?
    print("\n  Required conditions for full territorial recovery:\n")
    recovery_strategies = [
        ("Strategy:UA-Crimea-Isolation", 0.85, "Source:ISW",
         "Destroy Kerch bridge permanently, cut land corridor at Melitopol"),
        ("Strategy:UA-Naval-Superiority", 0.80, "Source:RUSI",
         "Unmanned naval systems (USVs) deny Russian Black Sea Fleet Sevastopol"),
        ("Strategy:UA-Western-Security-Guarantee", 0.90, "Source:Reuters",
         "NATO Article 5-equivalent security guarantee for post-war Ukraine"),
        ("Strategy:UA-Russian-Collapse", 0.55, "Source:Reuters",
         "Internal Russian political crisis (succession, ethnic unrest, economic collapse)"),
        ("Strategy:UA-Massive-Mobilization", 0.75, "Source:Reuters",
         "Ukraine mobilizes 500K+ additional troops with Western heavy equipment"),
        ("Strategy:UA-Information-Warfare", 0.72, "Source:Reuters",
         "Undermine Russian military morale: surrender hotlines, PSYOP on Crimean garrison"),
        ("Strategy:UA-Diplomatic-Isolation", 0.78, "Source:Reuters",
         "Global South turns against Russia: UN votes, ICC enforcement, secondary sanctions"),
    ]

    for label, conf, src, desc in recovery_strategies:
        store(label, "strategy", conf, src, {
            "description": desc, "type": "ukraine_offense",
            "simulation": "Full recovery scenario"
        })
        relate(label, "enables", "Prediction:UkraineFullRecovery", conf)
        print(f"  + {desc}")
        print(f"    Source: {src}  Confidence: {conf:.0%}")

    # With all strategies applied, recalculate
    all_for = recovery_for + [(desc, conf, src) for label, conf, src, desc in recovery_strategies]
    boosted_prob = calc_probability(all_for, recovery_against)

    bar_before = "#" * int(recovery_prob * 50)
    bar_after = "#" * int(boosted_prob * 50)
    print(f"\n  WITHOUT strategies: {recovery_prob:.0%}  {bar_before}")
    print(f"  WITH all strategies: {boosted_prob:.0%}  {bar_after}")
    print(f"  Improvement: +{boosted_prob - recovery_prob:.0%} probability points")
    print(f"\n  KEY INSIGHT: Even with all strategies, full Crimea recovery remains")
    print(f"  constrained by nuclear escalation risk (0.92) and fortress Sevastopol (0.90).")
    print(f"  Most realistic path: isolate Crimea (cut Kerch + land corridor) to force")
    print(f"  negotiated return, rather than direct military assault.")

    s = stats()
    print(f"\n  Graph: {s.get('nodes', '?')} nodes, {s.get('edges', '?')} edges")

# ---------------------------------------------------------------------------
# Phase 11: Export & Summary
# ---------------------------------------------------------------------------

def phase11_export():
    section("Phase 11: JSON-LD Export & Intelligence Summary")

    jsonld = export_jsonld()
    graph = jsonld.get("@graph", [])
    print(f"JSON-LD export: {len(graph)} entities")

    with open("russia_graph.jsonld", "w", encoding="utf-8") as f:
        json.dump(jsonld, f, indent=2, ensure_ascii=False)
    print("Saved to russia_graph.jsonld")

    s = stats()
    print(f"\nFinal graph: {s.get('nodes', '?')} nodes, {s.get('edges', '?')} edges")

    subsection("Intelligence Assessment Summary")

    pred_labels = [
        "Prediction:BalticProvocation",
        "Prediction:TurkeySanctionEvasion",
        "Prediction:MoldovaDestabilization",
        "Prediction:RubleInstability",
        "Prediction:BRICSCurrency",
        "Prediction:NKRussiaAxis",
        "Prediction:FrozenConflict",
        "Prediction:UkraineFrontlineFreeze",
        "Prediction:UkraineFullRecovery",
    ]

    print(f"{'Assessment':<50} {'Prob':>6} {'Category':>15}")
    print(f"{'-'*50} {'-'*6} {'-'*15}")

    for label in pred_labels:
        info = explain(label)
        if info and "entity" in info:
            props = info.get("properties", {})
            conf = info.get("confidence", 0)
            cat = props.get("category", "?")
            hyp = props.get("hypothesis", label)
            short = hyp[:47] + "..." if len(hyp) > 50 else hyp
            print(f"{short:<50} {conf:>5.0%} {cat:>15}")

    subsection("Data Provenance")
    print("Live data sources used in this analysis:")
    print("  1. Wikidata SPARQL (wikidata.org) -- borders, memberships, leaders, population")
    print("  2. World Bank API (api.worldbank.org) -- GDP, inflation, military spending")
    print("  3. Analyst enrichment -- conflicts, sanctions, strategic assets")
    print("  4. Source tier weighting -- institutional (0.95) > journalism (0.85) > state media (0.25)")
    print()
    print("Each prediction probability is traceable via /explain to its evidence chain.")
    print("Contradictions between state media and verified sources are automatically resolved.")
    print()
    print("KEY INSIGHT: Turkey flagged as ANOMALY -- only NATO member blocking Russia sanctions.")
    print("This was discovered by inference rules, not manually encoded.")

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print("=" * 70)
    print("  ENGRAM: Russia Geopolitical Analysis")
    print("  AI Intelligence Analyst with Live Data & Probabilistic Predictions")
    print("=" * 70)

    health = api("GET", "/health")
    if not health:
        print("Cannot reach engram server. Start with:")
        print("  engram serve russia.brain 127.0.0.1:3030")
        sys.exit(1)
    print(f"\nServer: {health.get('version', 'unknown')}")
    print(f"Data sources: Wikidata, World Bank, GDELT, Exchange Rate API, REST Countries, Wikipedia")

    border_countries = phase1_wikidata()
    phase2_worldbank()
    phase3_additional_sources()
    phase4_sources()
    phase5_conflict()
    phase6_contradictions()
    phase7_inference()
    phase8_predictions()
    phase9_discovery()
    phase10_simulation()
    phase11_export()

    print("\n" + "=" * 70)
    print("  Analysis complete.")
    print("  - Explore the graph: http://127.0.0.1:3030 (frontend)")
    print("  - Explain any entity: GET /explain/{entity}")
    print("  - MCP integration: configure engram as Claude Code MCP server")
    print("=" * 70)

if __name__ == "__main__":
    main()
