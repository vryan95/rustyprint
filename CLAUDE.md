# rustyprint

An HTML and CSS to PDF engine written in Rust, with its own layout engine. It ships as a CLI, an HTTP service, a Ruby gem and a NuGet package built on one core crate.

## Where the work stands

- [plan.md](plan.md) is the plan. Its **Milestones** table (section 8) has a **Status** column, `todo`, `partial` or `done`, which is the record of where work stands. The next milestone is the first one that is not `done`.
- [progress.html](progress.html) shows the same statuses with the tracker counts, and names the next milestone at the top.
- The tracker files (`html-elements.md`, `css-*.md`) track every HTML element and CSS feature, each with its milestone and status.

When you start a milestone, set its status to `partial`. When you finish it, set it to `done`. Either way, regenerate the page with `cargo run -p rustyprint-tests --bin progress`. The fixture suite fails if the page is out of date, or if a status disagrees with the trackers.

## Commands

CI (`.github/workflows/ci.yml`) runs these on every push to `master` and every pull request. `master` is protected: work goes on a branch and arrives through a pull request, which merges only when the `CI result` check passes. Run these before pushing a branch:

- `cargo test --workspace` runs every test, including the fixture suite: one HTML fixture in `tests/fixtures/` per tracker row. CI runs it on Linux, Windows and macOS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all --check`
- `cargo deny check`, with its configuration in `deny.toml`. Install it with `cargo install cargo-deny`.

`rust-toolchain.toml` pins the Rust version to the workspace's `rust-version`, and the workflows' `RUST_TOOLCHAIN` names it too. Change all three together; CI fails if they differ.

Nightly (`.github/workflows/nightly.yml`):

- `cargo bench -p rustyprint --bench render`
- `cargo +nightly fuzz run <html|css|render>` in `fuzz/`, which is a separate workspace. Fuzzing works on Linux and macOS only; on Windows, use WSL.

## Layout

- `crates/rustyprint-dom`, `-css`, `-layout` and `-pdf` are the engine stages. `crates/rustyprint` is the public API, `render(html, &RenderOptions)`.
- `render` runs each render on its own thread with a 256 MB stack, within the limits in `RenderOptions::limits`. The checks live in `rustyprint_dom::budget`.
- `schema/render-options.schema.json` describes the JSON render options. A test keeps it in step with `RenderOptions::from_json`.
- Workspace lints forbid `unsafe` code.

## Releasing

Every crate shares the workspace version in `Cargo.toml`. To release, set `version` under `[workspace.package]` in a pull request, and once it has merged, tag the merge on `master` `v<version>` and push the tag. `.github/workflows/release.yml` checks the tag against the version, runs CI, builds the CLI for every platform and publishes a GitHub Release. Run it by hand from the Actions tab for a dry run, which builds the archives without a release.

## Working agreements

- Write a milestone's tests first, then implement until they pass.
- Only stage, commit, push or open a pull request when asked.
