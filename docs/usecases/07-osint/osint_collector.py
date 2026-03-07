import requests
import json

API = "http://127.0.0.1:3030"

def store(entity, entity_type=None, props=None, source="osint", confidence=None):
    body = {"entity": entity, "source": source}
    if entity_type:
        body["type"] = entity_type
    if props:
        body["properties"] = props
    if confidence:
        body["confidence"] = confidence
    return requests.post(f"{API}/store", json=body).json()

def relate(from_e, to_e, rel, confidence=None):
    body = {"from": from_e, "to": to_e, "relationship": rel}
    if confidence:
        body["confidence"] = confidence
    return requests.post(f"{API}/relate", json=body).json()

def reinforce(entity, source=None):
    body = {"entity": entity}
    if source:
        body["source"] = source
    return requests.post(f"{API}/learn/reinforce", json=body).json()

# -- Phase 1: Domain Intelligence --

# Store domains and their registration data
store("example-target.com", "domain", {
    "registrar": "NameCheap",
    "registered": "2024-01-15",
    "nameservers": "ns1.hostingco.net",
    "country": "RU"
}, source="whois-lookup", confidence=0.95)

store("target-services.net", "domain", {
    "registrar": "NameCheap",
    "registered": "2024-01-16",
    "nameservers": "ns1.hostingco.net",
    "country": "RU"
}, source="whois-lookup", confidence=0.95)

# Same registrar + same nameserver + one day apart = likely same operator
relate("example-target.com", "target-services.net",
       "likely_same_operator", confidence=0.7)

# -- Phase 2: IP Infrastructure --

store("198.51.100.42", "ip_address", {
    "asn": "AS12345",
    "isp": "BulletProof Hosting Ltd",
    "country": "NL",
    "first_seen": "2024-02-01"
}, source="passive-dns", confidence=0.90)

relate("example-target.com", "198.51.100.42",
       "resolves_to", confidence=0.95)
relate("target-services.net", "198.51.100.42",
       "resolves_to", confidence=0.95)

# Shared IP reinforces the "same operator" link
reinforce("example-target.com", source="passive-dns")
reinforce("target-services.net", source="passive-dns")

# -- Phase 3: Social Media Correlation --

store("@target_user_42", "social_account", {
    "platform": "twitter",
    "created": "2023-11-20",
    "followers": "127",
    "bio_mentions": "example-target.com"
}, source="social-media-scan", confidence=0.80)

# Bio mentions the domain -- moderate confidence link
relate("@target_user_42", "example-target.com",
       "associated_with", confidence=0.6)

store("targetuser42@proton.me", "email", {
    "provider": "ProtonMail",
    "first_seen_in": "forum-post-2024-03"
}, source="forum-scrape", confidence=0.70)

# Email handle matches social handle -- possible link
relate("targetuser42@proton.me", "@target_user_42",
       "possible_same_person", confidence=0.5)

# -- Phase 4: Organizational Attribution --

store("APT-Phantom", "threat_group", {
    "aliases": "PhantomBear, Group-42",
    "region": "Eastern Europe",
    "active_since": "2022"
}, source="threat-report-vendor-A", confidence=0.75)

# Vendor A attributes the infrastructure to APT-Phantom
relate("198.51.100.42", "APT-Phantom",
       "attributed_to", confidence=0.60)

# Vendor B independently confirms
store("APT-Phantom", "threat_group",
      source="threat-report-vendor-B", confidence=0.85)
reinforce("APT-Phantom", source="threat-report-vendor-B")

# The independent confirmation boosts confidence
print("Two independent sources now corroborate APT-Phantom attribution")
