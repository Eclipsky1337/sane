# Repository Guidelines

## Scope

- These instructions apply to the entire repository.
- Keep changes focused on the requested task; do not fix unrelated issues.
- Follow `docs/development-plan.md` for feature order and deferred work.
- The no-test rule below overrides every test requirement in
  `docs/development-plan.md` and other documents.

## Branches And Commits

- Develop on `dev`; do not develop directly on `main`.
- The `baseline-clean` branch is the unoptimized measurement baseline; use it
  only for baseline work and comparative optimization re-additions.
- Keep `dev` rebased on `main` and avoid merge commits.
- Do not rewrite release tags or published history.
- Use Conventional Commit prefixes: `feat:`, `fix:`, `doc:`,
  `refactor:`, `perf:`, `build:`, or `ci:`.
- Keep each commit limited to one coherent change and its docs.

## Implementation

- Preserve the compiler pipeline: lexer, parser/AST, semantic resolution,
  basic-block IR lowering, then Brainfuck generation.
- Prefer root-cause fixes and minimal designs consistent with existing code.
- Keep generated Brainfuck valid for ordinary Brainfuck interpreters unless a
  task says otherwise.
- Update user documentation and CLI help when syntax or behavior changes.

## Tests

Do not add, restore, or rewrite tests. This includes integration tests under
`tests/`, unit tests, doctests, examples used only as tests, and
`#[cfg(test)]` modules. Do not run or require `cargo test`. Verify compiler
and runtime changes by running the relevant examples instead.

## Validation

Run before handing off a change:

```sh
cargo fmt -- --check
git diff --check
```

For compiler/runtime changes, also run the relevant examples from
`docs/development-plan.md` and verify their exact output.

## Generated And Temporary Files

- Do not commit `target/`, temporary Brainfuck output, or local scratch files.
- Keep `Cargo.lock` committed and use `--locked` for validation.
