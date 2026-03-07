import requests

API = "http://127.0.0.1:3030"

def store(entity, entity_type=None, props=None, source="factchecker", confidence=None):
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

# -- Register Sources with Reliability Ratings --

# Tier 1: High-reliability sources (peer-reviewed, official records)
store("Source:WHO", "source", {
    "type": "international_organization",
    "reliability_tier": "1",
    "track_record": "high"
}, confidence=0.95)

store("Source:Nature", "source", {
    "type": "peer_reviewed_journal",
    "reliability_tier": "1",
    "track_record": "high"
}, confidence=0.95)

# Tier 2: Moderate-reliability sources (major news outlets)
store("Source:Reuters", "source", {
    "type": "news_agency",
    "reliability_tier": "2",
    "track_record": "high"
}, confidence=0.85)

store("Source:BBC", "source", {
    "type": "news_outlet",
    "reliability_tier": "2",
    "track_record": "moderate-high"
}, confidence=0.82)

# Tier 3: Low-reliability sources (blogs, social media)
store("Source:RandomBlog", "source", {
    "type": "blog",
    "reliability_tier": "3",
    "track_record": "unknown"
}, confidence=0.40)

store("Source:SocialMediaPost", "source", {
    "type": "social_media",
    "reliability_tier": "3",
    "track_record": "low"
}, confidence=0.30)

# -- Claim 1: A factual claim from a reliable source --

store("Claim:EarthAge", "claim", {
    "text": "The Earth is approximately 4.54 billion years old",
    "category": "science",
    "date_first_seen": "2024-01-10"
}, source="factchecker", confidence=0.95)

relate("Claim:EarthAge", "Source:Nature", "sourced_from", confidence=0.95)

# Corroborated by another reliable source
relate("Claim:EarthAge", "Source:WHO", "corroborated_by", confidence=0.90)

# Reinforce via independent confirmation
requests.post(f"{API}/learn/reinforce", json={
    "entity": "Claim:EarthAge",
    "source": "Source:Nature"
})

# -- Claim 2: A disputed claim --

store("Claim:VitaminCCuresCold", "claim", {
    "text": "Vitamin C cures the common cold",
    "category": "health",
    "date_first_seen": "2024-02-15"
}, source="factchecker", confidence=0.50)

# Sourced from a blog (low reliability)
relate("Claim:VitaminCCuresCold", "Source:RandomBlog",
       "sourced_from", confidence=0.40)

# Contradicted by a high-reliability source
store("Evidence:CochraneMeta2024", "evidence", {
    "text": "Meta-analysis: Vitamin C does not prevent or cure colds",
    "study_type": "meta-analysis",
    "sample_size": "11306",
    "year": "2024"
}, source="factchecker", confidence=0.92)

relate("Evidence:CochraneMeta2024", "Source:Nature",
       "published_in", confidence=0.95)
relate("Evidence:CochraneMeta2024", "Claim:VitaminCCuresCold",
       "contradicts", confidence=0.90)

# -- Claim 3: A claim gaining credibility --

store("Claim:MicroplasticsInBlood", "claim", {
    "text": "Microplastics have been found in human blood",
    "category": "health",
    "date_first_seen": "2024-03-01"
}, source="factchecker", confidence=0.60)

relate("Claim:MicroplasticsInBlood", "Source:Reuters",
       "sourced_from", confidence=0.85)

# Second source confirms
relate("Claim:MicroplasticsInBlood", "Source:BBC",
       "corroborated_by", confidence=0.82)
requests.post(f"{API}/learn/reinforce", json={
    "entity": "Claim:MicroplasticsInBlood",
    "source": "Source:BBC"
})

# Third source: peer-reviewed study
store("Evidence:VUAmsterdam2022", "evidence", {
    "text": "Plasticenta study: microplastics detected in 17 of 22 blood samples",
    "study_type": "peer-reviewed",
    "year": "2022"
}, source="factchecker", confidence=0.90)

relate("Evidence:VUAmsterdam2022", "Claim:MicroplasticsInBlood",
       "supports", confidence=0.90)
requests.post(f"{API}/learn/reinforce", json={
    "entity": "Claim:MicroplasticsInBlood",
    "source": "Source:Nature"
})

# -- Debunk a claim --

result = requests.post(f"{API}/learn/correct", json={
    "entity": "Claim:VitaminCCuresCold",
    "source": "factchecker",
    "reason": "Debunked by Cochrane meta-analysis (11,306 participants)"
}).json()

print(f"Corrected: Claim:VitaminCCuresCold")
print(f"Propagated to: {result['propagated_to']}")

# -- Discredit a source --

result = requests.post(f"{API}/learn/correct", json={
    "entity": "Source:RandomBlog",
    "source": "factchecker",
    "reason": "Source publishes fabricated health claims"
}).json()

print(f"Source discredited. Propagated to: {result['propagated_to']}")

# -- Inference rules for automated fact-checking --

rules = [
    # If a claim is contradicted by peer-reviewed evidence, flag it
    """rule peer_reviewed_contradiction
when edge(A, "contradicts", B)
when edge(A, "published_in", C)
when prop(C, "reliability_tier", "1")
then flag(B, "contradicted by tier-1 evidence")""",

    # If a claim has multiple corroborations, it gains credibility
    """rule multi_source_corroboration
when edge(A, "corroborated_by", B)
when edge(A, "corroborated_by", C)
then flag(A, "multi-source corroboration detected")"""
]

result = requests.post(f"{API}/learn/derive", json={"rules": rules}).json()
print(f"Rules fired: {result['rules_fired']}, Flags: {result['flags_raised']}")
