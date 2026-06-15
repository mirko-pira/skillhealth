# Contributing to skillhealth

Thanks for taking the time. skillhealth is a small, focused Rust CLI, and
contributions of any size are welcome — bug reports, reproductions, docs, and
code.

## Getting started

You need a recent stable Rust toolchain (see the MSRV in the root
`Cargo.toml`). Then:

```bash
git clone https://github.com/mirko-pira/skillhealth
cd skillhealth
cargo build
cargo run -p skillhealth -- doctor   # run the CLI from source
```

## Before you open a pull request

The same four gates CI enforces — run them locally first:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All four must pass. If you touched the security audit, `cargo deny check` should
stay green too.

Guidelines:

- **Tests come with the change.** New behaviour gets a test; a bug fix gets a
  regression test that fails before your fix. Snapshot tests use `insta` — review
  changes with `cargo insta review`.
- **Keep diffs surgical.** One logical change per PR. Don't reformat or refactor
  code unrelated to your change.
- **User-facing changes update the docs.** README flags/keymap/behaviour and a
  `CHANGELOG.md` entry under the unreleased heading.
- **AI-assisted work is fine — a human owns the PR.** Use whatever tools you
  like, but you must understand, run, and stand behind the change. Unreviewed
  bot-generated patches will be closed.

## Commits and history

Conventional-commit-style prefixes (`feat:`, `fix:`, `docs:`, `chore:`) are
appreciated but not required. Once a branch is merged, its history is not
rewritten — fix-ups go in follow-up commits.

## Reporting bugs and proposing features

Use the issue forms (Issues → New issue). For security problems, do **not** open
a public issue — see [SECURITY.md](./SECURITY.md).

## License

By contributing, you agree that your contributions are licensed under the
project's dual [MIT](./LICENSE-MIT) OR [Apache-2.0](./LICENSE-APACHE) terms.
