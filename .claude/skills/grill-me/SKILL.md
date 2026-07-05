---
name: grill-me
description: A relentless one-question-at-a-time interview that sharpens a plan or design before any code is written, capturing decisions where they belong as they crystallise — plan-scoped ones into the ExecPlan's Decision Log, cross-plan ones as ADRs indexed in CLAUDE.md. Use when the user wants a plan stress-tested or an idea sharpened before building — "grill me", "poke holes in this", "interview me about this design", "stress-test this plan".
disable-model-invocation: true
---

# Grill Me

Two things go wrong when a plan moves straight from someone's head into implementation. First, **misalignment**: the user and the agent hold different understandings of the same words, and the gap only surfaces after code is written. Second, **evaporation**: the sharp definitions and hard-won trade-offs from the discussion live only in the conversation, and the next session starts from zero. This skill addresses both: interview until a genuinely shared understanding exists, and write each conclusion into its durable home *the moment it crystallises* — not at the end, when half of them are already forgotten.

## The interview

Interview the user relentlessly about every aspect of the plan. Walk down each branch of the design tree, resolving dependencies between decisions one by one — settle the decisions other decisions hang on before descending further.

- **Ask one question at a time.** Multiple questions at once are bewildering and produce shallow answers. Let each answer shape the next question. If the harness provides a structured question tool, you may use it — still one question per turn.
- **Lead with your recommended answer** and a one-line reason. A concrete recommendation gives the user something to react to, which is faster and more precise than an open-ended question.
- **If the codebase can answer a question, explore the codebase instead of asking.** The user's time is for judgment calls, not for facts you can look up. Likewise check the user's claims against the code and surface contradictions: "the code cancels entire Orders, but you just said partial cancellation is possible — which is right?"
- **Sharpen fuzzy language.** When the user uses a vague or overloaded term, propose a precise canonical one, and challenge usage that conflicts with terminology already established in `CLAUDE.md` or the active ExecPlan.
- **Stress-test with concrete scenarios.** Invent specific edge cases that force precision about where one concept ends and another begins.
- **Do not enact the plan** until the user confirms shared understanding has been reached.

## Capture as you go

Each time a decision crystallises, route it to its home immediately:

- **Plan-scoped decisions and definitions** → the ExecPlan draft. If the work warrants an ExecPlan (see `docs/PLANS.md`) start the draft during the session, not after — the grilling session is exactly the design conversation PLANS.md warns is ephemeral. Decisions go in its Decision Log with the reasoning; sharpened terms get defined where the plan first uses them.
- **Cross-plan decisions** → an ADR, following `docs/adr/README.md`. Apply its three-gate test (hard to reverse, surprising without context, a real trade-off — all three or skip). Write the ADR with its `Source:` line right then, add the one-line pointer to CLAUDE.md's ADR index, and move on. Expect most sessions to produce zero ADRs; that is the filter working.
- **Neither** (easy to reverse, obvious, or no real alternative) → nowhere. Recording everything buries the decisions that matter.

## After the interview

When shared understanding is reached, confirm the concrete next step. Usually that is finishing the ExecPlan draft and beginning its first milestone; sometimes the grilling reveals the work is too small for a plan, in which case just proceed — the ADRs and sharpened language, if any, were still worth capturing.

---

*Interview protocol adapted from [mattpocock/skills](https://github.com/mattpocock/skills) (`grilling`, MIT); capture model re-targeted to this repo's ExecPlan + ADR conventions.*
