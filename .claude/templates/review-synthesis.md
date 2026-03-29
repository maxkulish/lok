# Review Synthesis Template

**Purpose**: Consolidate findings from multiple AI reviewers (Gemini, Ollama, optional personas) into a single actionable document.

---

## Review Synthesis: CLO-XX - [Title]

**Synthesized**: [Date]
**Reviewers**: [List of reviewers that completed successfully]
**Design Document**: docs/design-docs/clo-XX-[description].md

---

## Agreement (High Confidence)

Items where 2+ reviewers independently identified the same concern or strength. These are high-confidence findings.

| # | Finding | Reviewers | Severity |
|---|---------|-----------|----------|
| 1 | [finding] | Gemini, Ollama | [CRITICAL/HIGH/MEDIUM/LOW] |

---

## Disagreement (Needs Human Decision)

Items where reviewers hold divergent positions. Present both sides for the user to decide.

| # | Topic | Position A (Reviewer) | Position B (Reviewer) |
|---|-------|----------------------|----------------------|
| 1 | [topic] | [position] (Gemini) | [position] (Ollama) |

---

## Novel Insights (Single Reviewer)

Items found by only one reviewer. Lower confidence but may surface blind spots the other missed.

| # | Finding | Reviewer | Severity |
|---|---------|----------|----------|
| 1 | [finding] | Gemini | [severity] |

---

## Persona Summary (if applicable)

| Persona | Verdict | Key Finding |
|---------|---------|-------------|
| Audio Safety | [SAFE/CONCERNS_HIGH/CONCERNS_MEDIUM] | [1-line summary] |
| FFI Safety | [SAFE/CONCERNS_HIGH/CONCERNS_MEDIUM] | [1-line summary] |
| State Machine | [CORRECT/CONCERNS_HIGH/CONCERNS_MEDIUM] | [1-line summary] |

---

## Consolidated Verdict

**Consensus Rules**:
- If ANY reviewer says NEEDS_REVISION -> Overall: NEEDS_REVISION
- If all say APPROVE -> Overall: APPROVE
- Otherwise -> Overall: APPROVE_WITH_SUGGESTIONS

**Overall Verdict**: [APPROVE | APPROVE_WITH_SUGGESTIONS | NEEDS_REVISION]

---

## Priority Actions

Ordered by severity, with agreement items first:

1. **[CRITICAL]** [action] (agreed by: [reviewers])
2. **[HIGH]** [action] (source: [reviewer])
3. **[MEDIUM]** [action] (source: [reviewer])
