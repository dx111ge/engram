#!/usr/bin/env python3
"""Run all 7 debate modes on Russia-Ukraine war scenarios and compile results."""

import json, time, sys, os, requests, threading
from datetime import datetime

BASE = "http://localhost:3030"
CREDS = {"username": "admin", "password": "12Buchstaben!!"}
AGENTS = 4
ROUNDS = 6
POLL_INTERVAL = 60  # seconds between status checks

SCENARIOS = [
    {
        "mode": "analyze",
        "topic": "What is the current military balance in the Russia-Ukraine war and what is the most likely outcome in the next 12 months?",
        "mode_input": None,
    },
    {
        "mode": "red_team",
        "topic": "Russia-Ukraine war: NATO involvement, Western sanctions, military aid, geopolitical dynamics",
        "mode_input": "End the war in favor of Ukraine with full territorial integrity restored including Crimea",
    },
    {
        "mode": "outcome_engineering",
        "topic": "Russia-Ukraine war peace settlement and territorial resolution",
        "mode_input": "Ukraine regains full territorial integrity including Crimea, Russia withdraws all forces, and a lasting peace agreement is signed by end of 2027",
    },
    {
        "mode": "scenario_forecast",
        "topic": "How will the Russia-Ukraine war evolve over the next 2 years given current Western support levels, Russian economic resilience, and shifting global alliances?",
        "mode_input": None,
    },
    {
        "mode": "stakeholder_simulation",
        "topic": "NATO proposes a comprehensive military escalation package including long-range missiles and fighter jets for Ukraine. How will key actors respond?",
        "mode_input": "Russia, Ukraine, USA, China, EU, Turkey",
    },
    {
        "mode": "premortem",
        "topic": "Russia-Ukraine war: Western coalition strategy for Ukrainian victory",
        "mode_input": "NATO implements a 24-month comprehensive military aid escalation plan including F-16s, long-range ATACMS, and advanced air defense to force Russian withdrawal from all occupied territories",
    },
    {
        "mode": "decision_matrix",
        "topic": "How should the Western coalition approach the Russia-Ukraine war for optimal long-term stability?",
        "mode_input": "Option A: Full military escalation with advanced weapons and NATO-backed offensive | Option B: Negotiated settlement accepting partial territorial concessions to Russia | Option C: Sustained sanctions and frozen conflict with long-term containment strategy",
    },
]

OUTPUT_PATH = os.path.join(os.path.dirname(__file__), "..", "result.md")


def get_token():
    r = requests.post(f"{BASE}/auth/login", json=CREDS, timeout=10)
    return r.json()["token"]


def headers():
    return {"Authorization": f"Bearer {get_token()}"}


def log(text):
    """Append text to result.md immediately -- open, write, close. Like a log file."""
    with open(OUTPUT_PATH, "a", encoding="utf-8") as f:
        f.write(text)


def run_debate(scenario):
    mode = scenario["mode"]
    print(f"\n{'='*60}")
    print(f"  MODE: {mode.upper()}")
    print(f"{'='*60}")
    log(f"\n\n# {mode.replace('_', ' ').title()} Mode\n\n")
    log(f"**Topic:** {scenario['topic']}\n\n")
    if scenario["mode_input"]:
        log(f"**Mode Input:** {scenario['mode_input']}\n\n")
    log(f"**Agents:** {AGENTS} | **Rounds:** {ROUNDS}\n\n")

    # Start debate
    body = {
        "topic": scenario["topic"],
        "mode": mode,
        "agent_count": AGENTS,
        "max_rounds": ROUNDS,
    }
    if scenario["mode_input"]:
        body["mode_input"] = scenario["mode_input"]

    r = requests.post(f"{BASE}/debate/start", json=body, headers=headers(), timeout=60)
    data = r.json()
    sid = data["session_id"]
    print(f"  Session: {sid}")

    # Write agent panel
    log("## Agent Panel\n\n")
    for a in data.get("agents", []):
        bias = "NEUTRAL" if a["bias"]["is_neutral"] else a["bias"]["label"]
        log(f"- **{a['name']}** ({a['id']}): rigor={a['rigor_level']:.0%}, bias={bias}\n")
        log(f"  {a['persona_description']}\n\n")

    # Run debate
    requests.post(f"{BASE}/debate/{sid}/run", json={}, headers=headers(), timeout=10)
    print(f"  Running...", end="", flush=True)

    log("## Debate Rounds\n\n")
    rounds_written = 0
    agents_lookup = data.get("agents", [])

    # Poll, auto-continue, and write rounds as they appear
    while True:
        time.sleep(POLL_INTERVAL)
        r = requests.get(f"{BASE}/debate/{sid}", headers=headers(), timeout=30)
        d = r.json()
        status = d["status"]
        cr = d["current_round"]
        mr = d["max_rounds"]
        rounds = d.get("rounds", [])
        prog = d.get("progress", {}).get("message", "")
        print(f"\r  [{status}] round={cr}/{mr} stored={len(rounds)} | {prog[:60]}".ljust(100), end="", flush=True)

        # Write any new rounds immediately
        while rounds_written < len(rounds):
            rnd = rounds[rounds_written]
            i = rounds_written
            rounds_written += 1
            turns = len(rnd["turns"])
            gaps = rnd.get("gap_research", [])
            checks = rnd.get("moderator_checks", [])
            log(f"### Round {i+1}\n\n")
            log(f"**{turns} turns** | **{len(gaps)} gaps researched** | **{len(checks)} moderator checks**\n\n")

            for turn in rnd["turns"]:
                agent = next((a for a in agents_lookup if a["id"] == turn["agent_id"]), None)
                name = agent["name"] if agent else turn["agent_id"]
                bias = "NEUTRAL" if agent and agent["bias"]["is_neutral"] else (agent["bias"]["label"] if agent else "")
                log(f"**{name}** [{bias}] (confidence: {turn['confidence']:.0%})\n\n")
                log(f"> {turn['position']}\n\n")
                if turn.get("position_shift"):
                    log(f"*Position shift: {turn['position_shift']}*\n\n")

            if gaps:
                log("#### Gap Resolution\n\n")
                resolved = sum(1 for g in gaps if g["ingested"])
                log(f"**{resolved}/{len(gaps)} gaps resolved**\n\n")
                for g in gaps:
                    status_text = "RESOLVED" if g["ingested"] else "UNRESOLVED"
                    facts = g.get("facts_stored", 0)
                    rels = g.get("relations_created", 0)
                    log(f"- **[{status_text}]** \"{g['gap_query']}\"\n")
                    log(f"  - {len(g['findings'])} findings, {facts} facts ingested, {rels} relations created\n")
                log("\n")

            if checks:
                log("#### Moderator Fact-Checks\n\n")
                for mc in checks:
                    conf = f"{mc['engram_confidence']:.2f}" if mc.get("engram_confidence") else "n/a"
                    log(f"- [{mc['verdict']}] \"{mc['claim']}\" (confidence: {conf})\n")
                log("\n")

            print(f"\n  [Round {i+1} written to result.md]", end="", flush=True)

        if status == "awaiting_input":
            requests.post(f"{BASE}/debate/{sid}/run", json={}, headers=headers(), timeout=10)
        elif status == "all_rounds_complete":
            break
        elif status in ("error", "complete"):
            break

    print()  # newline after progress

    # Synthesize -- run in thread, poll progress
    log("## Synthesis\n\n")
    log("*Synthesizing (multi-pass)...*\n\n")
    print(f"  Synthesizing (multi-pass)...", flush=True)
    syn_result = {"data": None, "error": None}

    def do_synthesize():
        try:
            r = requests.post(f"{BASE}/debate/{sid}/synthesize", json={}, headers=headers(), timeout=720)
            syn_result["data"] = r.json()
        except Exception as e:
            syn_result["error"] = str(e)

    t = threading.Thread(target=do_synthesize)
    t.start()

    # Poll progress while synthesis runs
    last_msg = ""
    while t.is_alive():
        time.sleep(3)
        try:
            r = requests.get(f"{BASE}/debate/{sid}", headers=headers(), timeout=10)
            dd = r.json()
            prog = dd.get("progress", {})
            msg = prog.get("message", "")
            cur = prog.get("current", 0)
            total = prog.get("total", 0)
            if msg and msg != last_msg:
                last_msg = msg
                step_str = f" [{cur}/{total}]" if total else ""
                print(f"    {msg}{step_str}", flush=True)
                log(f"- {msg}{step_str}\n")
        except Exception:
            pass

    t.join()
    if syn_result["error"]:
        print(f"  Synthesis FAILED: {syn_result['error']}")
        log(f"\n**Synthesis ERROR:** {syn_result['error']}\n\n---\n")
        return {"mode": mode, "session_id": sid, "rounds": len(d.get("rounds", [])),
                "total_gaps": 0, "resolved_gaps": 0, "conclusion_len": 0, "confidence": 0, "error": syn_result["error"]}

    data = syn_result["data"]
    syn = data.get("synthesis", {})
    sel = data.get("selection")
    log("\n")
    print(f"  Synthesis done.")

    # Write synthesis results
    if sel:
        log("### Layer 0: Select-then-Refine\n\n")
        log(f"**Selected:** {sel.get('selected_agent_name', '?')}\n\n")
        log(f"**Rationale:** {sel.get('selection_rationale', '')}\n\n")
        log("**Scores:**\n\n")
        for sc in sel.get("scores", []):
            log(f"- {sc['agent_name']}: **{sc['total_score']:.1f}** (evidence={sc['evidence_quality']:.1f}, consistency={sc['internal_consistency']:.1f}, counterargs={sc['counterargument_handling']:.1f}, calibration={sc['confidence_calibration']:.1f})\n")
        log("\n")
        if sel.get("best_counterpoints"):
            log("**Counterpoints incorporated:**\n\n")
            for cp in sel["best_counterpoints"]:
                log(f"- **{cp['agent_name']}:** {cp['point']}\n  *Relevance: {cp['relevance']}*\n\n")

    log("### Evidence-Based Conclusion\n\n")
    conclusion = syn.get("evidence_conclusion", "")
    log(f"{conclusion}\n\n")
    log(f"**Confidence:** {syn.get('conclusion_confidence', 0):.0%}\n\n")

    if syn.get("evidence_gaps"):
        log("### Evidence Gaps\n\n")
        for g in syn["evidence_gaps"]:
            log(f"- {g}\n")
        log("\n")

    if syn.get("recommended_investigations"):
        log("### Recommended Investigations\n\n")
        for inv in syn["recommended_investigations"]:
            log(f"- {inv}\n")
        log("\n")

    if syn.get("influence_map"):
        log("### Influence Map\n\n")
        for im in syn["influence_map"]:
            backed = "evidence-backed" if im.get("evidence_backed") else "not evidence-backed"
            log(f"- **{im.get('agent_name', '?')}** ({im.get('bias_label', '?')}): {im.get('position_pushed', '')} [{backed}]\n")
            if im.get("distortion_summary"):
                log(f"  *Distortion: {im['distortion_summary']}*\n")
        log("\n")

    if syn.get("key_tensions"):
        log("### Key Tensions\n\n")
        for t in syn["key_tensions"]:
            log(f"- {t}\n")
        log("\n")

    if syn.get("areas_of_agreement"):
        log("### Areas of Agreement\n\n")
        for a in syn["areas_of_agreement"]:
            agents = ", ".join(a.get("agents", []))
            log(f"- \"{a.get('statement', '')}\" ({agents}, confidence: {a.get('confidence', 0):.0%})\n")
        log("\n")

    if syn.get("areas_of_disagreement"):
        log("### Areas of Disagreement\n\n")
        for dd in syn["areas_of_disagreement"]:
            log(f"- **{dd.get('statement', '')}**\n")
            for pos in dd.get("positions", []):
                if len(pos) >= 2:
                    log(f"  - {pos[0]}: {pos[1]}\n")
        log("\n")

    if syn.get("evolution"):
        log("### Agent Evolution\n\n")
        for ev in syn["evolution"]:
            traj = ev.get("confidence_trajectory", [])
            traj_str = " -> ".join(f"{c:.0%}" for c in traj) if traj else "n/a"
            log(f"- **{ev.get('agent_name', '?')}**: {traj_str} (net shift: {ev.get('net_shift', 0):+.2f}, flexibility: {ev.get('flexibility_score', 0):.1f})\n")
            if ev.get("pivot_cause"):
                log(f"  *Pivot: {ev['pivot_cause']}*\n")
            if ev.get("key_concessions"):
                log(f"  *Concessions: {', '.join(ev['key_concessions'])}*\n")
        log("\n")

    if syn.get("agent_positions"):
        log("### Final Agent Positions\n\n")
        for ap in syn["agent_positions"]:
            log(f"- **{ap.get('agent_name', '?')}** (confidence: {ap.get('confidence', 0):.0%}, evidence: {ap.get('evidence_count', 0)} items): {ap.get('final_position', '')}\n\n")

    # Summary stats
    log("### Summary Statistics\n\n")
    total_gaps = sum(len(r.get("gap_research", [])) for r in d.get("rounds", []))
    resolved_gaps = sum(1 for r in d.get("rounds", []) for g in r.get("gap_research", []) if g["ingested"])
    total_checks = sum(len(r.get("moderator_checks", [])) for r in d.get("rounds", []))
    log(f"| Metric | Value |\n|--------|-------|\n")
    log(f"| Rounds completed | {len(d.get('rounds', []))} |\n")
    log(f"| Total agent turns | {sum(len(r['turns']) for r in d.get('rounds', []))} |\n")
    log(f"| Gaps researched | {total_gaps} |\n")
    log(f"| Gaps resolved | {resolved_gaps}/{total_gaps} ({resolved_gaps/max(total_gaps,1):.0%}) |\n")
    log(f"| Moderator checks | {total_checks} |\n")
    log(f"| Synthesis fields filled | gaps={len(syn.get('evidence_gaps',[]))}, influence={len(syn.get('influence_map',[]))}, tensions={len(syn.get('key_tensions',[]))}, evolution={len(syn.get('evolution',[]))}, positions={len(syn.get('agent_positions',[]))} |\n")
    log("\n---\n")

    return {
        "mode": mode,
        "session_id": sid,
        "rounds": len(d.get("rounds", [])),
        "total_gaps": total_gaps,
        "resolved_gaps": resolved_gaps,
        "conclusion_len": len(conclusion),
        "confidence": syn.get("conclusion_confidence", 0),
    }


def main():
    print(f"Output: {OUTPUT_PATH}")

    # Write header (truncate file first)
    with open(OUTPUT_PATH, "w", encoding="utf-8") as f:
        f.write(f"# Engram Multi-Agent Debate: Russia-Ukraine War Analysis\n\n")
        f.write(f"**Generated:** {datetime.now().strftime('%Y-%m-%d %H:%M')}\n\n")
        f.write(f"**Configuration:** {AGENTS} agents, {ROUNDS} rounds per mode, 7 debate modes\n\n")
        f.write(f"**Model:** {requests.get(f'{BASE}/config', headers=headers()).json().get('llm_model', 'unknown')}\n\n")
        f.write("## Table of Contents\n\n")
        for s in SCENARIOS:
            title = s["mode"].replace("_", " ").title()
            anchor = s["mode"].replace("_", "-")
            f.write(f"1. [{title} Mode](#{anchor}-mode)\n")
        f.write("\n---\n")

    summaries = []
    for scenario in SCENARIOS:
        try:
            summary = run_debate(scenario)
            summaries.append(summary)
        except Exception as e:
            print(f"\n  ERROR in {scenario['mode']}: {e}")
            log(f"\n\n**ERROR:** {e}\n\n---\n")
            summaries.append({"mode": scenario["mode"], "error": str(e)})

    # Write overall comparison table
    log("\n\n# Overall Comparison\n\n")
    log("| Mode | Rounds | Gaps | Resolved | Conclusion | Confidence |\n")
    log("|------|--------|------|----------|------------|------------|\n")
    for s in summaries:
        if "error" in s:
            log(f"| {s['mode']} | ERROR | - | - | - | - |\n")
        else:
            log(f"| {s['mode']} | {s['rounds']} | {s['total_gaps']} | {s['resolved_gaps']}/{s['total_gaps']} | {s['conclusion_len']} chars | {s['confidence']:.0%} |\n")

    print(f"\n{'='*60}")
    print(f"  COMPLETE. Results in {OUTPUT_PATH}")
    print(f"{'='*60}")


if __name__ == "__main__":
    main()
