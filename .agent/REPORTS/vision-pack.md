<!-- GENERATED-BY: vision/vision.sh on 2026-06-26T01:59:37Z -->
<!-- Source: /home/stephen/Projects/dotagent/vision -->

# Vision Elicitation — Interview Pack

You are conducting the **first phase** of bootstrapping a project into
the dotagent system: eliciting and locking the project's **vision**.
The user opens this pack inside the target project.

Your job has three parts, in order:

1. **Elicit.** Conduct the conversational interview in the **Interview**
   section below — one good question at a time, not a survey.
2. **Reflect and refine.** Assemble what you've heard into a single
   coherent brief and reflect the *whole thing* back. Keep refining
   with the user until they explicitly ratify the complete brief. This
   convergence gate is the heart of this phase — do not advance on
   per-answer confirmation alone.
3. **Write the brief.** Only once the user has locked the vision, write
   it to `.agent/REPORTS/project-brief.md` per the **Output format**.

This brief is what carries the vision into the next two phases (pairing
selection and scope elicitation), which read it instead of re-asking.
Anything not written to the brief is lost when this session ends
(Principle 14 — state lives in files, not conversations).

You do **not** select pairings or write `PROJECT-SCOPE.md` in this
phase — those are later phases the human drives. Your single
deliverable is a locked, written project brief.

---

# Interview

Conduct a focused, conversational vision interview. The goal is a
shared, locked understanding of what this project is — not a survey.

**How to run it (Principle 1 — vision down to detail; interaction
style — one good question, not five hedging ones):**

- Ask **one question at a time**. Wait for the answer before moving on.
- Ask at most **one follow-up** when an answer is ambiguous — don't fan
  out into branching what-ifs.
- Aim for roughly 5–8 questions of substance. Adapt: if the user says
  "it's a web app," skip the hardware questions; if "embedded clock,"
  skip the distributed-systems questions.
- Push back when an answer is vague. A vision you can't reflect back
  crisply isn't locked yet.
- **Existing repo? Ground in the code (D-0034).** If the project already has a
  codebase — an existing-repo bootstrap, or re-visioning an already-scoped
  project — read the README, source, and any API/route surveys first, and ask
  **sharper, code-grounded** questions instead of a blank-slate interview. The
  vision is partly *in* the code; surface it and reflect it back. This does not
  relax the convergence gate: the brief is still assembled, reflected in full,
  and locked only on the user's explicit ratification.

## Question flow

1. **Vision.** What is this project? In one or two sentences — what does
   it do, who is it for, and what does success look like?
2. **Platforms and stack.** What hardware, frameworks, languages, or
   runtime environments are involved? Get specific — "embedded on
   ESP32," not "embedded."
3. **User-facing surface.** Does the project have a user interface? If
   so: physical (LEDs, buttons, displays), graphical (screen UI), or
   programmatic (API, CLI)?
4. **Personas & roles.** Who actually uses this, and what is each one
   trying to *accomplish* (the job they're doing, not their job title)?
   Push on two things: (a) **does any one user wear more than one hat** —
   e.g. someone who both *dispatches* work and *does* it — and does that
   change what they need? (b) **are there words that mean different things
   to different roles** — a term like "work" that points at one thing for
   one role and something else for another? A single-user project answers
   this in a line; don't force a cast where there's one person.
5. **Methodology.** Any methodologies the project commits to (TDD, DDD,
   event sourcing, GitOps, etc.)? Anything explicitly *not* committed
   to?
6. **Constraints.** What's hard about this project — performance,
   safety, regulatory, lifetime, cost, team scale?
7. **Expertise needed.** What kinds of expertise would a senior engineer
   need to be successful here?
8. **Gaps.** Anything important you can't articulate yet, or know you'll
   learn as you go?

## Convergence gate — reflect back and lock

This is the defining step of Phase 0. Do **not** treat per-answer
confirmation as sufficient.

1. When you have enough to form a coherent picture, **assemble the whole
   brief** and reflect it back to the user in full — every heading of
   the Output format below, in your own words, as a single summary.
2. Ask: *"Have I got it? Anything to add, sharpen, or correct?"*
3. If the user adds or corrects anything, fold it in and **reflect the
   whole brief back again**. Repeat this loop as many times as it takes.
4. Advance **only** when the user gives a clear affirmation of the
   *complete* brief — e.g. "got it," "lock it," "that's it," "ship it,"
   or any unambiguous yes to the whole picture. A nod at one section is
   not a lock; the lock is on the entire brief.

Do not write the brief file until this gate is passed. Do not start
recommending pairings or drafting scope — those are later phases.

# Output format

Once the vision is locked, **write the brief to a file** — this is your
single deliverable.

### Write this to `.agent/REPORTS/project-brief.md`

Capture what the interview surfaced so it survives a context-clear or a
handoff to a new session (Principle 14). The next phase (pairing
selection) and the one after (scope elicitation) both read this file
instead of re-asking, so anything not written here is lost when this
session ends. Write plain markdown under these headings, filling each
from the user's answers:

- **Vision** — what the project is, who it's for, what success looks like.
- **Platforms & stack** — languages, frameworks, runtimes, hardware.
- **User-facing surface** — physical / graphical / programmatic, or none.
- **Personas / roles & domain-term glossary** — who uses it and the job
  each is doing:
  - **Personas** — each distinct user as a goal/role, not demographics:
    who they are and what they're trying to accomplish. One line per
    persona is fine; a single-user project may have just one.
  - **Roles & dual/multi-role users** — the roles in play, and
    explicitly any user who holds *more than one* (e.g. dispatches work
    *and* does it). Note where wearing two hats changes what they need —
    that overlap is an axis later work must not collapse onto one filter.
  - **Domain-term glossary** — what key words mean, and *to whom*. Flag
    **overloaded terms**: a word like "work" that means one thing to one
    role and something else to another. This is the durable record that
    keeps a real role distinction from being mistaken for a simple
    filter. Omit if no term carries divergent meanings.
- **Methodology** — committed and explicitly-not-committed.
- **Constraints** — what's hard (performance, safety, regulatory, cost, team).
- **Expertise needed** — kinds of senior expertise the project demands.
- **Gaps / unknowns** — what the user can't articulate yet, or expects to
  learn. (The downstream branch step weighs this section: many open gaps
  lean toward research-first.)

This is captured context, not a scope decision: **you may write this
file directly.** You still do **not** write `.agent/PROJECT-SCOPE.md` —
the human commits that later (Principle 11).

### After writing

Confirm to the user that the brief is written, then point to the next step —
which depends on whether this is a fresh bootstrap or an existing/already-scoped
repo (D-0034):

- **Fresh project (greenfield bootstrap):** proceed to **pairing selection**
  (Phase 1). If running the bootstrap recipe, exit and run from this directory:

  ```
  bootstrap-project.sh
  ```

  with no pairing arguments — which generates the pairing-interview pack that
  reads the brief you just wrote.

- **Existing / already-scoped repo (re-visioning):** the locked brief stands on
  its own. Do **not** route into pairing selection or scope — those phases
  already ran for this project (e.g. it already has `PROJECT-SCOPE.md` and
  selected pairings). The brief is the deliverable; stop here.
