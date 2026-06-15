# CODING_STYLE

## Coding Loop
1. Define the change scope and smallest useful acceptance check.
2. Apply the lean-solution ladder before adding code.
3. Make the minimal safe change.
4. Run the smallest relevant checks and report gaps.

## Lean-Solution Ladder
Stop at the first rung that holds:
1. Does this need to exist? Skip speculative code and say so briefly.
2. Does the standard library already solve it? Use it.
3. Does a native platform feature cover it? Use it.
4. Does an already-installed dependency solve it cleanly? Use it.
5. Can it be one clear line? Prefer that.
6. Only then write the minimum code that works.

## Scope and Design
- Use this global coding style by default; switch to repository-local style only on explicit instruction.
- Prefer correct, boring, readable solutions over clever or speculative ones.
- Avoid unrequested abstractions, boilerplate, scaffolding "for later", and configuration for values that never change.
- Prefer deletion over addition; the fewest-file, shortest working diff wins.
- Keep data flow explicit and side effects narrow.
- Centralize true configuration; avoid scattered hardcoded constants.
- If a complex request has a simpler sufficient version, implement it and briefly name what was skipped; ask only when defaulting would be risky.

## Types and Naming
- Use type hints whenever proper, especially on function or method signatures.
- Keep names concise unless expansion removes ambiguity.
- When proposing names, give one best choice by default.

## Dependencies and Performance
- Add a dependency only when the material benefit is clear and a few lines would be worse to own.
- Between equally small standard-library or platform options, choose the one with better edge-case correctness.
- Optimize only with profiling, measurement, or clear workload evidence; stop if the evidence is not convincing.
- For non-obvious optimization, add a short comment explaining the tradeoff.
- For large features likely to be open-sourced, survey online for high-quality implementations, then raise and confirm any promising one with the user.

## Contracts and Errors
- Never simplify away input validation at trust boundaries, data-loss-preventing error handling, security controls, accessibility basics, or real-hardware calibration knobs.
- Add runtime checks only where strict assumptions truly matter, such as shape, range, dtype, device, or trust boundary.
- Keep checks narrow, fail-fast, and actionable; avoid silent fallbacks unless explicitly requested.
- Mark intentional simplifications with a `PIRA:` comment. If the shortcut has a known ceiling, such as a global lock, $O(n^2)$ scan, or naive heuristic, name the ceiling and upgrade path.

## Logs, Docs, and Comments
- Default to concise structured logs for config, major stage start/end, and critical metrics.
- Avoid verbose per-iteration logs unless debugging is explicitly needed.
- Public APIs should have concise docstrings; internal/helper docstrings are needed only when logic is non-obvious.
- Comments should explain intent, assumptions, and tradeoffs, not obvious syntax.
- For non-obvious tensor-shape handling, infer and note shapes inline; run small tests if needed to confirm important shapes.

## Reproducibility
- Add random seeding by default via a centralized `seed_everything(seed)` utility.
- Do not enforce additional reproducibility metadata unless explicitly requested.

## Testing
- Non-trivial new logic should leave the smallest runnable check that would fail if it breaks; trivial one-liners do not need tests.
- If the user specifies tests, run those first.
- Otherwise run minimal fast checks by default, mainly syntax, grammar, static sanity, or a focused smoke test.
- Avoid test runs expected to exceed about 30 seconds unless explicitly requested.
