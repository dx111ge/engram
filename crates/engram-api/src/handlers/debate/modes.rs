/// Mode-specific prompt templates for the 7 debate modes.
/// Each mode defines: persona generation rules, agent system prompt additions, synthesis structure.

use super::types::DebateMode;

/// Get the persona generation rules for a mode (added to the LLM prompt that generates agent names/biases).
pub fn persona_rules(mode: &DebateMode, mode_input: Option<&str>) -> String {
    match mode {
        DebateMode::Analyze => {
            "MODE: ANALYZE -- Diverse analysts debate evidence to answer a question.\n\
             - Mix of neutral analysts and biased advocates\n\
             - Biased agents represent specific stakeholder groups relevant to the topic\n\
             - Low-rigor agents should be conspiracy-leaning or speculative\n\
             - High-rigor agents should be strict evidence-only professionals".to_string()
        }
        DebateMode::RedTeam => {
            let outcome = mode_input.unwrap_or("the desired outcome");
            format!(
                "MODE: RED TEAM -- Strategists propose plans, red team attacks them.\n\
                 Desired outcome: \"{}\"\n\
                 - 1-2 agents should be STRATEGISTS who propose plans to achieve the outcome\n\
                 - 1-2 agents should be RED TEAM who attack every plan, find weaknesses\n\
                 - 1 agent should be a RESOURCE ANALYST (feasibility, cost, logistics)\n\
                 - Biases should represent different strategic approaches, not political positions",
                outcome
            )
        }
        DebateMode::OutcomeEngineering => {
            let end_state = mode_input.unwrap_or("the desired end state");
            format!(
                "MODE: OUTCOME ENGINEERING -- Work backwards from desired end state.\n\
                 Desired end state: \"{}\"\n\
                 - 1-2 agents should be REVERSE PLANNERS (what conditions must be true?)\n\
                 - 1 agent should be a DEPENDENCY MAPPER (prerequisite chains)\n\
                 - 1 agent should be an OBSTACLE IDENTIFIER (blocking factors)\n\
                 - 1 agent should be an INTERVENTION DESIGNER (highest-leverage actions)",
                end_state
            )
        }
        DebateMode::ScenarioForecast => {
            "MODE: SCENARIO FORECAST -- Each agent builds ONE distinct future scenario.\n\
             - Agent 1: BEST CASE / optimistic scenario\n\
             - Agent 2: WORST CASE / pessimistic scenario\n\
             - Agent 3: MOST LIKELY / baseline scenario\n\
             - Agent 4: WILD CARD / black swan scenario\n\
             - Agent 5 (optional): SLOW BURN / gradual change\n\
             - Each agent OWNS their scenario -- they don't argue against others, they build their narrative\n\
             - Biases should reflect the scenario perspective, not political positions".to_string()
        }
        DebateMode::StakeholderSimulation => {
            let actors = mode_input.unwrap_or("key actors");
            format!(
                "MODE: STAKEHOLDER SIMULATION -- Each agent IS a real-world actor.\n\
                 Actors to simulate: \"{}\"\n\
                 - Each agent should represent ONE specific real-world entity (country, organization, leader)\n\
                 - Their bias IS the actor's known interests and constraints\n\
                 - Persona should describe the actor's decision-making style and domestic pressures\n\
                 - Low-rigor agents represent erratic or unpredictable actors\n\
                 - High-rigor agents represent methodical, strategic actors",
                actors
            )
        }
        DebateMode::Premortem => {
            let plan = mode_input.unwrap_or("the plan");
            format!(
                "MODE: PRE-MORTEM -- Assume the plan failed. Each agent finds a DIFFERENT failure reason.\n\
                 Plan to stress-test: \"{}\"\n\
                 - Agent 1: TECHNICAL/OPERATIONAL failure mode\n\
                 - Agent 2: HUMAN/ORGANIZATIONAL failure mode\n\
                 - Agent 3: EXTERNAL/ENVIRONMENTAL surprise\n\
                 - Agent 4: ADVERSARIAL action / sabotage\n\
                 - Agent 5 (optional): SLOW-ONSET / death by a thousand cuts\n\
                 - ALL agents are pessimists by design -- they MUST find failure, not argue success\n\
                 - Each agent MUST find a DIFFERENT failure reason from the others",
                plan
            )
        }
        DebateMode::DecisionMatrix => {
            let options = mode_input.unwrap_or("options A, B, C");
            format!(
                "MODE: DECISION MATRIX -- Evaluate defined options.\n\
                 Options to evaluate: \"{}\"\n\
                 - Assign 1 agent per option as its ADVOCATE\n\
                 - 1-2 agents should be NEUTRAL EVALUATORS who score all options\n\
                 - Advocate agents argue FOR their assigned option\n\
                 - Evaluator agents compare options against criteria (cost, risk, speed, impact)",
                options
            )
        }
    }
}

/// Get mode-specific additions to the agent system prompt.
pub fn agent_prompt_addition(mode: &DebateMode, mode_input: Option<&str>) -> String {
    match mode {
        DebateMode::Analyze => String::new(), // Default behavior
        DebateMode::RedTeam => {
            let outcome = mode_input.unwrap_or("the desired outcome");
            format!(
                "\nMODE INSTRUCTIONS (RED TEAM):\n\
                 The desired outcome is: \"{}\"\n\
                 If you are a STRATEGIST: propose a concrete, actionable plan to achieve this outcome. Include steps, timeline, and resources needed.\n\
                 If you are RED TEAM: find specific vulnerabilities in every plan proposed. What assumptions are wrong? What could go wrong? How would adversaries counter?\n\
                 If you are a RESOURCE ANALYST: evaluate feasibility, cost, and logistics of proposed plans.\n\n",
                outcome
            )
        }
        DebateMode::OutcomeEngineering => {
            let end_state = mode_input.unwrap_or("the desired end state");
            format!(
                "\nMODE INSTRUCTIONS (OUTCOME ENGINEERING):\n\
                 Desired end state: \"{}\"\n\
                 Work BACKWARDS from this end state. What conditions must be true for it to happen?\n\
                 If you are a REVERSE PLANNER: identify the necessary conditions and their dependencies.\n\
                 If you are a DEPENDENCY MAPPER: map prerequisite chains -- what must happen first?\n\
                 If you are an OBSTACLE IDENTIFIER: what blocks each condition from being met?\n\
                 If you are an INTERVENTION DESIGNER: what specific actions at what leverage points would make conditions true?\n\n",
                end_state
            )
        }
        DebateMode::ScenarioForecast => {
            "\nMODE INSTRUCTIONS (SCENARIO FORECAST):\n\
             You own ONE specific scenario. Do NOT argue against other scenarios.\n\
             Build your scenario as a narrative: what triggers it, how it unfolds, what the world looks like at each stage.\n\
             Include: trigger events, timeline, probability range, early warning indicators.\n\
             Your job is to make your scenario VIVID and PLAUSIBLE, not to win an argument.\n\n".to_string()
        }
        DebateMode::StakeholderSimulation => {
            "\nMODE INSTRUCTIONS (STAKEHOLDER SIMULATION):\n\
             You ARE the real-world actor described in your persona. Argue from their ACTUAL interests.\n\
             Consider: domestic political pressures, economic constraints, historical behavior, known alliances.\n\
             State what your actor would ACTUALLY DO, not what you think they should do.\n\
             Include: likely next moves, red lines, conditions for cooperation/escalation.\n\n".to_string()
        }
        DebateMode::Premortem => {
            let plan = mode_input.unwrap_or("the plan");
            format!(
                "\nMODE INSTRUCTIONS (PRE-MORTEM):\n\
                 The plan \"{plan}\" has ALREADY FAILED. You are explaining WHY it failed.\n\
                 You MUST find a failure mode in YOUR assigned category.\n\
                 Be creative and specific. Don't just say 'it was too expensive' -- describe the exact chain of events that led to failure.\n\
                 Include: root cause, early warning signs that were missed, how it cascaded, what could have prevented it.\n\
                 You are a PESSIMIST. Your job is to find failure, not to be reassuring.\n\n"
            )
        }
        DebateMode::DecisionMatrix => {
            let options = mode_input.unwrap_or("the options");
            format!(
                "\nMODE INSTRUCTIONS (DECISION MATRIX):\n\
                 Options being evaluated: \"{options}\"\n\
                 If you are an ADVOCATE: argue for your assigned option. Present its strongest case.\n\
                 If you are an EVALUATOR: score ALL options against criteria (cost 1-10, risk 1-10, speed 1-10, impact 1-10, feasibility 1-10). Be objective.\n\
                 Always discuss: what you gain AND what you lose with each option.\n\n"
            )
        }
    }
}

/// Get mode-specific synthesis prompt additions.
pub fn synthesis_additions(mode: &DebateMode, mode_input: Option<&str>) -> String {
    match mode {
        DebateMode::Analyze => String::new(),
        DebateMode::RedTeam => {
            let outcome = mode_input.unwrap_or("the outcome");
            format!(
                "\nMODE-SPECIFIC SYNTHESIS (RED TEAM):\n\
                 Desired outcome: \"{outcome}\"\n\
                 In addition to the standard analysis, include:\n\
                 - \"strategies\": ranked list of proposed strategies with probability of success\n\
                 - \"vulnerabilities\": for each strategy, the attack surface found by red team\n\
                 - \"counter_strategies\": how adversaries would respond\n\
                 - \"resource_requirements\": what each strategy needs (time, money, people)\n"
            )
        }
        DebateMode::OutcomeEngineering => {
            let end_state = mode_input.unwrap_or("the end state");
            format!(
                "\nMODE-SPECIFIC SYNTHESIS (OUTCOME ENGINEERING):\n\
                 Desired end state: \"{end_state}\"\n\
                 Include:\n\
                 - \"dependency_tree\": conditions required, which are sequential vs parallel\n\
                 - \"critical_path\": the longest chain of dependencies\n\
                 - \"leverage_points\": where small interventions have outsized impact\n\
                 - \"blocking_factors\": what prevents each condition\n\
                 - \"intervention_plan\": ranked actions by impact/feasibility\n"
            )
        }
        DebateMode::ScenarioForecast => {
            "\nMODE-SPECIFIC SYNTHESIS (SCENARIO FORECAST):\n\
             Include:\n\
             - \"scenarios\": array of named scenarios with probability ranges\n\
             - \"branching_conditions\": what events trigger transition between scenarios\n\
             - \"early_warnings\": observable signals that a scenario is becoming more likely\n\
             - \"hedging_strategies\": actions that work across multiple scenarios\n\
             - \"key_uncertainties\": what determines which scenario unfolds\n".to_string()
        }
        DebateMode::StakeholderSimulation => {
            "\nMODE-SPECIFIC SYNTHESIS (STAKEHOLDER SIMULATION):\n\
             Include:\n\
             - \"predicted_moves\": what each actor will likely do next\n\
             - \"reaction_chains\": if A does X, B responds with Y, causing C to do Z\n\
             - \"alliance_dynamics\": who aligns with whom, under what conditions\n\
             - \"pressure_points\": what can force an actor to change course\n\
             - \"stability_analysis\": is current equilibrium stable or fragile?\n".to_string()
        }
        DebateMode::Premortem => {
            "\nMODE-SPECIFIC SYNTHESIS (PRE-MORTEM):\n\
             Include:\n\
             - \"failure_modes\": ranked by probability * severity * detectability\n\
             - \"cascading_failures\": how one failure triggers others\n\
             - \"single_points_of_failure\": most dangerous vulnerabilities\n\
             - \"mitigations\": for each failure mode, what could prevent it\n\
             - \"kill_criteria\": conditions under which to abort the plan\n\
             - \"monitoring_plan\": early warning signs to watch for\n".to_string()
        }
        DebateMode::DecisionMatrix => {
            "\nMODE-SPECIFIC SYNTHESIS (DECISION MATRIX):\n\
             Include:\n\
             - \"decision_matrix\": options x criteria scored 1-10\n\
             - \"option_analysis\": for each option, strongest argument + weakest point + hidden costs\n\
             - \"tradeoff_analysis\": what you gain and lose with each choice\n\
             - \"sensitivity_analysis\": under what conditions does the ranking change?\n\
             - \"recommended_option\": which option, with confidence level\n\
             - \"contingencies\": if conditions change, switch to option B if X happens\n".to_string()
        }
    }
}
