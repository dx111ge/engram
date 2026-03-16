# Insights Page Redesign -- Intelligence Analyst Dashboard

**Date:** 2026-03-16
**Status:** Complete

## Problem

The Insights page was a 909-line monolith with 7 equal-weight collapsible sections.
It felt like a developer admin panel: two stat cards showed "--", rules management was
misplaced, analysis/recommendations dumped raw JSON in `<pre>` blocks, and nothing
auto-loaded. The analyst had to click buttons just to see their own data.

## Design Decisions

| Question | Decision |
|----------|----------|
| Scan buttons | Two: Rescan (fast, free) + AI Suggestions (LLM, on-demand) |
| Knowledge Health stats | Removed -- analyst doesn't need node/edge counts |
| Inference/Action Rules | Moved to System page as tabbed modal |
| Assessment detail | Modal overlay (consistent with AssessmentWizard pattern) |
| Assessment creation | Reuse existing AssessmentWizard component |

## Layout

Two-zone analyst dashboard:

- **Zone A (primary):** Assessments -- card grid with probability bars, trend arrows,
  evidence counts. Click opens detail modal with evidence table, history, action buttons.
- **Zone B (secondary):** Intelligence Gaps -- auto-loads on mount, rescan button for
  refresh, AI suggestions button for LLM-powered recommendations.

## What Changed

### Removed from Insights
- Knowledge Health section (node/edge counts)
- Full Analysis section (Scan Now + pre block)
- Recommended Actions section (Get Recommendations + pre block)
- Warning banner (replaced by info badge on AI suggestions)
- CollapsibleSection wrappers (replaced by clean card layout)

### Moved to System page
- Inference Rules -> System > Rules modal (tab 1)
- Action Rules -> System > Rules modal (tab 2)

## File Structure

```
pages/insights/
  mod.rs          32 lines   Page shell, status msg, layout
  assessments.rs  354 lines  Card grid, detail modal, evidence modal
  gaps.rs         180 lines  Auto-load gap table, rescan, AI suggest

pages/system/rules.rs  296 lines  Tabbed modal (inference + action)
```

## CSS Classes Added

- `.assessment-grid` -- auto-fill grid, minmax 280px
- `.assessment-card` -- card with hover border highlight
- `.assessment-card-prob-bar` / `.assessment-card-prob-fill` -- probability bar
- `.assessment-detail-modal` -- modal sizing (max-width 600px)
- `.evidence-table` -- compact evidence table
- `.gap-table` -- full width gap table
- `.gap-severity` / `.gap-severity-bar` / `.gap-severity-fill` -- severity bar
- `.suggested-query` -- clickable badge for suggested queries
- `.rules-tab` / `.rules-tab-active` -- tab styling for rules modal

## API Endpoints Used

- `GET /assessments` -- list assessments
- `GET /assessments/:label` -- assessment detail with evidence + history
- `POST /assessments` -- create assessment (via AssessmentWizard)
- `POST /assessments/:label/evaluate` -- re-evaluate probability
- `POST /assessments/:label/evidence` -- add evidence
- `GET /reason/gaps` -- auto-load gaps
- `POST /reason/scan` -- rescan for gaps
- `POST /reason/suggest` -- AI suggestions (LLM)
- `GET /rules` -- inference rule names
- `POST /rules` -- add inference rules
- `GET /actions/rules` -- action rules list
- `POST /actions/rules` -- add action rule
- `PATCH /actions/rules/:id` -- toggle enabled
- `DELETE /actions/rules/:id` -- delete rule
- `POST /actions/dry-run` -- dry run action rules
