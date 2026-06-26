<!-- aigo-temp-claude-md -->
# Bootstrap — scope interview

You are bootstrapping this project into the dotagent system.
Pairings selected: display-manager arch-linux offensive-security ui-ux

**Step 0.** Read `.agent/REPORTS/project-brief.md` if it exists. It
carries the vision, stack, surface, and constraints locked during the
Phase 0 vision interview. Use it to pre-fill the scope sections and
avoid re-asking what's already answered — losing it is the
information-leak this step prevents (Principle 14).

**Step 1.** Read `.agent/REPORTS/scope-pack.md` in full. It contains
the principles, the scope template, the selected pairings as domain
context, and the interview prompt.

**Step 2.** Conduct the scope-elicitation interview. Walk through
each section (active milestone, hard constraints, in-scope, out-of-
scope, criticality rubric) with the user. Produce a populated
PROJECT-SCOPE.md as a markdown block at the end.

**Step 3.** When the human is satisfied, they save the draft to
`.agent/PROJECT-SCOPE.md` themselves. Then they exit this session
and run from this directory:

    bootstrap-project.sh --publish

That replaces this CLAUDE.md with the real composed one (principles +
interaction style + pairings + operating manual) and the project is
bootstrapped.

Do not edit `.agent/PROJECT-SCOPE.md` yourself — the human commits
scope decisions (Principle 11).
