# Repository Guidelines

## Scope

- These instructions apply to the entire repository.
- Keep changes focused on the requested task; do not fix unrelated issues.
- Follow `docs/development-plan.md` for feature order and deferred work.

## Branches And Commits

- Develop on `dev`; do not develop directly on `main`.
- Keep `dev` rebased on `main` and avoid merge commits.
- Do not rewrite release tags or published history.
- Use Conventional Commit prefixes: `feat:`, `fix:`, `doc:`, `test:`,
  `refactor:`, `perf:`, `build:`, or `ci:`.
- Keep each commit limited to one coherent change and its tests/docs.

## Implementation

- Preserve the compiler pipeline: lexer, parser/AST, semantic resolution,
  structured or PC lowering, then Brainfuck generation.
- Prefer root-cause fixes and minimal designs consistent with existing code.
- Keep structured-backend behavior compatible unless a task says otherwise.
- Add tests beside the closest existing integration-test category.
- Update user documentation and CLI help when syntax or behavior changes.

## Validation

Run before handing off a change:

```sh
cargo fmt -- --check
cargo test --locked
git diff --check
```

For compiler/runtime changes, also run the relevant examples from
`docs/development-plan.md` and verify their exact output.

## Generated And Temporary Files

- Do not commit `target/`, temporary Brainfuck output, or local scratch files.
- Keep `Cargo.lock` committed and use `--locked` for validation.
