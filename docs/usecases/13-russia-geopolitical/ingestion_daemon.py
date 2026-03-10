#!/usr/bin/env python3
"""
Ingestion Daemon for Russia Geopolitical Analysis

Continuously polls public news APIs, classifies articles by topic,
assigns source-tier confidence, stores in engram, links to prediction
nodes, and recalculates probabilities after each batch.

Usage:
    # First run russia_demo.py to build the base graph
    engram serve russia.brain 127.0.0.1:3030
    python russia_demo.py

    # Then start the daemon
    python ingestion_daemon.py [--interval 900] [--port 3030]

    Ctrl+C to stop gracefully.

Data sources:
    - GDELT Project: global news events (15-min updates)
    - Exchange Rate API: live RUB/USD (hourly)
"""

import argparse
import hashlib
import json
import os
import signal
import sys
import time
from datetime import datetime, timezone
from urllib.parse import quote

import requests as http

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

BASE = "http://127.0.0.1:3030"
GDELT = "https://api.gdeltproject.org/api/v2/doc/doc"
EXCHANGE = "https://open.er-api.com/v6/latest/USD"
UA = {"User-Agent": "EngRAM/0.1 (geopolitical-ingestion-daemon)"}

# How GDELT topics map to prediction nodes
TOPIC_PREDICTION_MAP = {
    "sanctions": [
        ("Prediction:TurkeySanctionEvasion", "supports"),
        ("Prediction:RubleInstability", "supports"),
    ],
    "nato_tensions": [
        ("Prediction:BalticProvocation", "supports"),
    ],
    "baltic": [
        ("Prediction:BalticProvocation", "supports"),
    ],
    "ceasefire": [
        ("Prediction:UkraineFrontlineFreeze", "supports"),
        ("Prediction:FrozenConflict", "supports"),
        ("Prediction:UkraineFullRecovery", "weakens"),
    ],
    "negotiations": [
        ("Prediction:UkraineFrontlineFreeze", "supports"),
        ("Prediction:FrozenConflict", "supports"),
    ],
    "ukraine_offensive": [
        ("Prediction:UkraineFullRecovery", "supports"),
        ("Prediction:FrozenConflict", "weakens"),
        ("Prediction:UkraineFrontlineFreeze", "weakens"),
    ],
    "crimea": [
        ("Prediction:UkraineFullRecovery", "supports"),
    ],
    "moldova": [
        ("Prediction:MoldovaDestabilization", "supports"),
    ],
    "transnistria": [
        ("Prediction:MoldovaDestabilization", "supports"),
    ],
    "north_korea_russia": [
        ("Prediction:NKRussiaAxis", "supports"),
    ],
    "brics": [
        ("Prediction:BRICSCurrency", "supports"),
    ],
    "ruble": [
        ("Prediction:RubleInstability", "supports"),
    ],
    "turkey_russia": [
        ("Prediction:TurkeySanctionEvasion", "supports"),
    ],
    "nuclear": [
        ("Prediction:BalticProvocation", "weakens"),
        ("Prediction:UkraineFullRecovery", "weakens"),
    ],
    "aid_ukraine": [
        ("Prediction:UkraineFullRecovery", "supports"),
        ("Prediction:FrozenConflict", "weakens"),
    ],
    "economy_russia": [
        ("Prediction:RubleInstability", "supports"),
    ],
}

# Source domain -> confidence tier
DOMAIN_CONFIDENCE = {
    # Tier 1: institutional
    "un.org": 0.92, "worldbank.org": 0.93, "imf.org": 0.92,
    "nato.int": 0.90, "europa.eu": 0.90, "state.gov": 0.88,
    # Tier 2: quality journalism / think tanks
    "reuters.com": 0.85, "apnews.com": 0.85, "bbc.com": 0.82, "bbc.co.uk": 0.82,
    "nytimes.com": 0.82, "washingtonpost.com": 0.80, "theguardian.com": 0.80,
    "aljazeera.com": 0.78, "dw.com": 0.80, "france24.com": 0.78,
    "understandingwar.org": 0.85,  # ISW
    "rusi.org": 0.85,
    "csis.org": 0.82, "chathamhouse.org": 0.82,
    "globalsecurity.org": 0.78,
    # Tier 2.5: regional quality
    "kyivindependent.com": 0.78, "ukrinform.net": 0.75,
    "unian.net": 0.72, "unian.ua": 0.72,
    "pravda.com.ua": 0.70,
    "interfax.com.ua": 0.75,
    # Tier 3: state-controlled / propaganda
    "tass.com": 0.25, "rt.com": 0.25, "sputniknews.com": 0.20,
    "ria.ru": 0.25, "iz.ru": 0.25, "rg.ru": 0.25,
    "inosmi.ru": 0.30,
    "xinhuanet.com": 0.35, "globaltimes.cn": 0.30,
}

# GDELT search queries and their topic classification
GDELT_QUERIES = [
    ("Russia sanctions", ["sanctions"]),
    ("Russia NATO Baltic", ["nato_tensions", "baltic"]),
    ("Ukraine ceasefire negotiations", ["ceasefire", "negotiations"]),
    ("Ukraine offensive counteroffensive", ["ukraine_offensive"]),
    ("Crimea Ukraine attack", ["crimea", "ukraine_offensive"]),
    ("Moldova Transnistria Russia", ["moldova", "transnistria"]),
    ("North Korea Russia weapons", ["north_korea_russia"]),
    ("BRICS currency dollar", ["brics"]),
    ("Russian ruble economy", ["ruble", "economy_russia"]),
    ("Turkey Russia trade sanctions", ["turkey_russia", "sanctions"]),
    ("nuclear escalation Russia", ["nuclear"]),
    ("Ukraine military aid weapons", ["aid_ukraine"]),
]

# All prediction labels
PREDICTIONS = [
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

# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------

seen_articles = set()  # article URLs we've already processed
history_file = "prediction_history.jsonl"
running = True

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
        else:
            return {}
        if not r.text or not r.text.strip():
            return {}
        try:
            return r.json()
        except Exception:
            return {"raw": r.text}
    except http.exceptions.ConnectionError:
        return None

def store(label, node_type=None, confidence=None, source=None, props=None):
    body = {"entity": label}
    if node_type: body["type"] = node_type
    if confidence is not None: body["confidence"] = confidence
    if source: body["source"] = source
    if props: body["properties"] = props
    return api("POST", "/store", body)

def relate(from_l, rel, to_l, confidence=None):
    body = {"from": from_l, "to": to_l, "relationship": rel}
    if confidence is not None: body["confidence"] = confidence
    return api("POST", "/relate", body)

def explain(label):
    return api("GET", f"/explain/{quote(label, safe='')}")

def reinforce(label, source=None):
    body = {"entity": label}
    if source: body["source"] = source
    return api("POST", "/learn/reinforce", body)

def stats():
    return api("GET", "/stats")

# ---------------------------------------------------------------------------
# Domain confidence lookup
# ---------------------------------------------------------------------------

def domain_confidence(url):
    """Extract domain from URL and return source tier confidence."""
    try:
        from urllib.parse import urlparse
        parsed = urlparse(url)
        domain = parsed.netloc.lower()
        # Strip www.
        if domain.startswith("www."):
            domain = domain[4:]
        # Check exact match
        if domain in DOMAIN_CONFIDENCE:
            return DOMAIN_CONFIDENCE[domain]
        # Check suffix match (e.g. bbc.co.uk)
        for known, conf in DOMAIN_CONFIDENCE.items():
            if domain.endswith(known):
                return conf
        # Unknown source: moderate confidence
        return 0.50
    except Exception:
        return 0.50

def classify_article(title, topics_hint):
    """Classify an article into topics based on title keywords and query hint."""
    title_lower = title.lower()
    matched = set(topics_hint)  # start with query-hinted topics

    # Keyword-based refinement
    keyword_map = {
        "sanctions": ["sanction", "embargo", "freeze", "asset", "swift"],
        "ceasefire": ["ceasefire", "cease-fire", "truce", "armistice"],
        "negotiations": ["negotiat", "diplomat", "peace talk", "summit", "deal"],
        "ukraine_offensive": ["offensive", "counterattack", "liberat", "advance", "recaptur"],
        "crimea": ["crimea", "sevastopol", "kerch"],
        "nato_tensions": ["nato", "article 5", "alliance"],
        "baltic": ["baltic", "estonia", "latvia", "lithuania", "kaliningrad", "suwalki"],
        "moldova": ["moldova", "chisinau"],
        "transnistria": ["transnistria", "tiraspol"],
        "north_korea_russia": ["north korea", "pyongyang", "kim jong"],
        "brics": ["brics", "de-dollar", "dedollar"],
        "ruble": ["ruble", "rouble", "rub/usd"],
        "turkey_russia": ["turkey", "ankara", "erdogan", "bosphorus"],
        "nuclear": ["nuclear", "atomic", "escalat"],
        "aid_ukraine": ["military aid", "weapons deliver", "f-16", "atacms", "storm shadow",
                        "leopard", "abrams", "himars"],
        "economy_russia": ["russian economy", "russia gdp", "inflation russia", "oil price"],
    }

    for topic, keywords in keyword_map.items():
        for kw in keywords:
            if kw in title_lower:
                matched.add(topic)
                break

    return list(matched)

# ---------------------------------------------------------------------------
# GDELT polling
# ---------------------------------------------------------------------------

def fetch_gdelt_articles(query, max_results=10):
    """Fetch recent articles from GDELT for a given query."""
    try:
        params = {
            "query": query,
            "mode": "artlist",
            "maxrecords": str(max_results),
            "format": "json",
            "sort": "datedesc",
            "timespan": "1d",  # last 24 hours only
        }
        r = http.get(GDELT, params=params, headers=UA, timeout=15)
        if r.status_code != 200:
            return []
        data = r.json()
        return data.get("articles", [])
    except Exception:
        return []

def process_articles(articles, topics_hint):
    """Process a batch of articles: classify, store, link to predictions."""
    new_count = 0
    links = []

    for art in articles:
        url = art.get("url", "")
        title = art.get("title", "").strip()
        domain = art.get("domain", "")
        date = art.get("seendate", "")[:8]

        if not url or not title:
            continue

        # Deduplicate by URL hash
        url_hash = hashlib.md5(url.encode()).hexdigest()[:12]
        if url_hash in seen_articles:
            continue
        seen_articles.add(url_hash)

        # Classify
        topics = classify_article(title, topics_hint)
        if not topics:
            continue

        # Source confidence
        conf = domain_confidence(url)

        # Sanitize title for label
        safe_title = title.encode('ascii', 'replace').decode('ascii')[:80]
        label = f"News:{date}:{url_hash}"

        # Store article
        store(label, "news_article", conf, f"Source:{domain}", {
            "title": safe_title,
            "url": url,
            "date": date,
            "domain": domain,
            "topics": ",".join(topics),
        })

        # Link to prediction nodes
        for topic in topics:
            if topic in TOPIC_PREDICTION_MAP:
                for pred_label, rel_type in TOPIC_PREDICTION_MAP[topic]:
                    relate(label, rel_type, pred_label, conf)
                    links.append((safe_title[:50], rel_type, pred_label, conf))

        new_count += 1

    return new_count, links

# ---------------------------------------------------------------------------
# Exchange rate polling
# ---------------------------------------------------------------------------

def fetch_exchange_rate():
    """Fetch live RUB/USD rate and update the graph."""
    try:
        r = http.get(EXCHANGE, headers=UA, timeout=10)
        if r.status_code != 200:
            return None
        data = r.json()
        rate = data.get("rates", {}).get("RUB")
        if rate:
            ts = data.get("time_last_update_utc", "")
            store("ExchangeRate:RUB-USD", "exchange_rate", 0.95, "Source:ExchangeRateAPI", {
                "rate": str(round(rate, 2)),
                "currency_pair": "RUB/USD",
                "timestamp": ts,
            })
            # If rate > 100, it supports ruble instability
            if rate > 100:
                relate("ExchangeRate:RUB-USD", "supports", "Prediction:RubleInstability", 0.85)
            elif rate > 90:
                relate("ExchangeRate:RUB-USD", "supports", "Prediction:RubleInstability", 0.60)

            return rate
    except Exception:
        return None

# ---------------------------------------------------------------------------
# Prediction recalculation
# ---------------------------------------------------------------------------

def recalculate_predictions():
    """Query each prediction's evidence chain and recalculate probability."""
    results = {}

    for pred_label in PREDICTIONS:
        info = explain(pred_label)
        if not info or "entity" not in info:
            continue

        old_conf = info.get("confidence", 0)

        # edges_to = incoming edges (from evidence TO this prediction)
        edges_to = info.get("edges_to", [])

        # Separate supporting and weakening evidence
        supporting = []
        weakening = []

        for edge in edges_to:
            rel = edge.get("relationship", "")
            conf = edge.get("confidence", 0.5)

            if rel == "supports":
                supporting.append(conf)
            elif rel == "weakens":
                weakening.append(conf)

        if not supporting and not weakening:
            results[pred_label] = {"probability": old_conf, "shift": 0.0,
                                   "for": 0, "against": 0}
            continue

        # Bayesian calculation
        if supporting:
            total_for = sum(supporting)
            weighted_for = sum(c * c for c in supporting) / total_for
        else:
            weighted_for = 0.0

        if weakening:
            total_against = sum(weakening)
            weighted_against = sum(c * c for c in weakening) / total_against
        else:
            weighted_against = 0.0

        n_total = len(supporting) + len(weakening)
        discount = len(weakening) / n_total if n_total > 0 else 0
        new_prob = weighted_for * (1 - weighted_against * discount)
        new_prob = round(max(0.05, min(0.95, new_prob)), 4)

        # Update prediction confidence
        props = info.get("properties", {})
        store(pred_label, "prediction", new_prob, "engram-daemon", {
            "hypothesis": props.get("hypothesis", ""),
            "timeframe": props.get("timeframe", ""),
            "probability": str(new_prob),
            "evidence_for_count": str(len(supporting)),
            "evidence_against_count": str(len(weakening)),
            "category": props.get("category", ""),
            "methodology": "Bayesian evidence aggregation (daemon recalculation)",
            "last_updated": datetime.now(timezone.utc).isoformat(),
        })

        shift = new_prob - old_conf
        results[pred_label] = {
            "probability": new_prob,
            "old_probability": old_conf,
            "shift": shift,
            "for": len(supporting),
            "against": len(weakening),
        }

    return results

# ---------------------------------------------------------------------------
# History logging
# ---------------------------------------------------------------------------

def log_snapshot(predictions, new_articles, exchange_rate):
    """Append a prediction snapshot to the history file."""
    entry = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "new_articles": new_articles,
        "exchange_rate": exchange_rate,
        "predictions": predictions,
    }
    with open(history_file, "a", encoding="utf-8") as f:
        f.write(json.dumps(entry, ensure_ascii=False) + "\n")

# ---------------------------------------------------------------------------
# Main loop
# ---------------------------------------------------------------------------

def signal_handler(sig, frame):
    global running
    print("\n  Shutting down gracefully...")
    running = False

def run_daemon(interval=900, port=3030):
    global BASE, running
    BASE = f"http://127.0.0.1:{port}"

    signal.signal(signal.SIGINT, signal_handler)

    # Check server
    health = api("GET", "/health")
    if not health:
        print("Cannot reach engram server. Start with:")
        print(f"  engram serve russia.brain 127.0.0.1:{port}")
        print("  python russia_demo.py  # build base graph first")
        sys.exit(1)

    s = stats()
    print("=" * 70)
    print("  ENGRAM: Geopolitical Ingestion Daemon")
    print("=" * 70)
    print(f"  Server: {health.get('version', '?')}")
    print(f"  Graph: {s.get('nodes', '?')} nodes, {s.get('edges', '?')} edges")
    print(f"  Polling interval: {interval}s ({interval//60}min)")
    print(f"  GDELT queries: {len(GDELT_QUERIES)}")
    print(f"  Predictions tracked: {len(PREDICTIONS)}")
    print(f"  History file: {history_file}")
    print(f"  Domain confidence tiers: {len(DOMAIN_CONFIDENCE)} domains")
    print("=" * 70)

    # Show initial prediction state
    print("\n  Initial prediction state:")
    for pred in PREDICTIONS:
        info = explain(pred)
        if info and "entity" in info:
            conf = info.get("confidence", 0)
            hyp = info.get("properties", {}).get("hypothesis", pred)
            short = hyp[:55] + "..." if len(hyp) > 58 else hyp
            print(f"    {conf:>5.0%}  {short}")

    cycle = 0
    while running:
        cycle += 1
        now = datetime.now(timezone.utc)
        print(f"\n{'='*70}")
        print(f"  Cycle {cycle} -- {now.strftime('%Y-%m-%d %H:%M:%S UTC')}")
        print(f"{'='*70}")

        # 1. Fetch GDELT articles
        total_new = 0
        all_links = []

        for query, topics in GDELT_QUERIES:
            if not running:
                break
            articles = fetch_gdelt_articles(query, max_results=5)
            new, links = process_articles(articles, topics)
            total_new += new
            all_links.extend(links)

        print(f"\n  Articles: {total_new} new (total seen: {len(seen_articles)})")

        if all_links:
            print(f"  Links created: {len(all_links)}")
            # Show a sample
            for title, rel, pred, conf in all_links[:5]:
                arrow = "->" if rel == "supports" else "-|"
                print(f"    [{conf:.0%}] {title}... {arrow} {pred.split(':')[1]}")
            if len(all_links) > 5:
                print(f"    ... and {len(all_links) - 5} more")

        # 2. Fetch exchange rate
        rate = fetch_exchange_rate()
        if rate:
            print(f"\n  RUB/USD: {rate:.2f}")

        # 3. Recalculate predictions
        predictions = recalculate_predictions()

        if predictions:
            print(f"\n  {'Prediction':<45} {'Prob':>6} {'Shift':>7} {'For':>4} {'Vs':>4}")
            print(f"  {'-'*45} {'-'*6} {'-'*7} {'-'*4} {'-'*4}")

            significant_shifts = []
            for pred_label in PREDICTIONS:
                if pred_label in predictions:
                    p = predictions[pred_label]
                    prob = p["probability"]
                    shift = p["shift"]
                    n_for = p["for"]
                    n_against = p["against"]
                    name = pred_label.split(":")[1]

                    shift_str = f"{shift:+.1%}" if shift != 0 else "   --"
                    print(f"  {name:<45} {prob:>5.0%} {shift_str:>7} {n_for:>4} {n_against:>4}")

                    if abs(shift) >= 0.02:
                        significant_shifts.append((name, shift, prob))

            if significant_shifts:
                print(f"\n  ** Significant shifts (>2%):")
                for name, shift, prob in significant_shifts:
                    direction = "UP" if shift > 0 else "DOWN"
                    print(f"     {name}: {direction} {abs(shift):.1%} -> now {prob:.0%}")

        # 4. Log to history
        log_snapshot(predictions, total_new, rate)

        s = stats()
        print(f"\n  Graph: {s.get('nodes', '?')} nodes, {s.get('edges', '?')} edges")

        # Wait for next cycle
        if running:
            print(f"\n  Next cycle in {interval}s... (Ctrl+C to stop)")
            # Sleep in small increments so we can catch SIGINT
            for _ in range(interval):
                if not running:
                    break
                time.sleep(1)

    print("\n  Daemon stopped.")
    print(f"  Total articles processed: {len(seen_articles)}")
    print(f"  History saved to: {history_file}")

# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Engram Geopolitical Ingestion Daemon")
    parser.add_argument("--interval", type=int, default=900,
                        help="Polling interval in seconds (default: 900 = 15min)")
    parser.add_argument("--port", type=int, default=3030,
                        help="Engram server port (default: 3030)")
    args = parser.parse_args()

    run_daemon(interval=args.interval, port=args.port)
