# Code standards

These are the rules every change to skyscreens follows, whether a person or an AI agent writes it. We rely on checks over trust: every rule a tool can enforce fails the build, and every way around a rule is counted, frozen, and needs the maintainer's approval to grow.

Each rule names its source and the tool and setting that enforce it. `scripts/check.sh` runs all of them, and CI runs the same script. A rule marked *review* has no tool yet, so reviewers check it by hand. The last section lists recommendations we decided not to adopt, with the reason.

Sources:
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/), the [Clippy lint list](https://rust-lang.github.io/rust-clippy/) and Embark Studios' [shared Rust lint set](https://github.com/EmbarkStudios/rust-ecosystem/blob/main/lints.toml)
- The [Swift API Design Guidelines](https://www.swift.org/documentation/api-design-guidelines/), [Swift 6 data-race safety](https://www.swift.org/migration/documentation/migrationguide/) and [strict memory safety (SE-0458)](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0458-strict-memory-safety.md)
- [SwiftLint](https://realm.github.io/SwiftLint/rule-directory.html) and [swift-format](https://github.com/swiftlang/swift-format)
- [Ruff](https://docs.astral.sh/ruff/rules/) and [mypy strict mode](https://mypy.readthedocs.io/en/stable/command_line.html#cmdoption-mypy-strict)
- The [Google Shell Style Guide](https://google.github.io/styleguide/shellguide.html) and [ShellCheck](https://www.shellcheck.net)
- The [OpenSSF Scorecard](https://github.com/ossf/scorecard) checks and [zizmor](https://docs.zizmor.sh) for GitHub Actions
- [cargo-mutants](https://mutants.rs) for test quality, and [Conventional Commits](https://www.conventionalcommits.org)

## Guardrails for AI-assisted development

| Guardrail | How it works |
|---|---|
| **Escape-hatch ledger** | `scripts/escape_hatches.py` counts every way around a rule, per file: `unsafe` code, lint `expect`s, `@unchecked Sendable`, `assumeIsolated`, `try?`, discarded results, `\|\| true`, silenced stderr, `Any` in Python. It also fingerprints every rule file and gate script. The check fails on any difference from the committed `escape-hatches.json`. Only a human regenerates it, with `scripts/escape_hatches.py --update` |
| **Code owners** | `.github/CODEOWNERS` covers the ledger, every lint config, the gate scripts, the workflows and the files that hold `unsafe`. With the branch ruleset "Require review from Code Owners", a pull request touching them needs the maintainer's approval |
| **Agent guard** | `.claude/settings.json` asks the human before any Claude Code edit to a protected file. Its hook, `scripts/agent_guard.py`, blocks shell commands that write those files or regenerate the ledger. It is a speed bump for agents; CI and code owners are the gate |
| **Mutation testing** | `scripts/check.sh mutants` changes the protocol code one mutation at a time (`cargo mutants`). The run fails if a mutant survives, because a surviving mutant means a test that checks nothing. A mutant that makes a test hang counts as caught |
| **Coverage floors** | Host Rust keeps at least 85% line coverage. Every Swift file of pure logic keeps at least 90%. Files that drive hardware or processes are listed in `scripts/swift_coverage.py`; everything else must be tested |
| **Definition of done** | [AGENTS.md](../AGENTS.md#definition-of-done): the checks pass, a test proves each behavior change, and the evals pass for what the change touches |

## No unsafe code where we can avoid it

| Rule | Enforced by |
|---|---|
| The Mac side has no `unsafe` Rust. `link`, `link-helper` and `fake-headset` declare `#![forbid(unsafe_code)]`, which no attribute can override | rustc `unsafe_code` |
| Swift talks to Rust through a helper process (`skyscreens-link`) and a pipe, not through a C bridge. There are no linker flags and no generated headers | architecture |
| Swift code is compiled with strict memory safety (SE-0458): any unsafe pointer, `Unmanaged` or C pointer call must be marked `unsafe`, or the build fails. Pointer work lives in `PixelMemory.swift`; the remaining marked calls are CoreGraphics display configuration, encoder creation and one CoreFoundation property | Swift compiler, ledger |
| The headset still needs `unsafe` for OpenXR, Vulkan, JNI and the Android NDK, which only have unsafe entry points. It is confined to modules that opt in with `#![expect(unsafe_code, reason = "…")]`, one operation per block. The ledger freezes the current count; the reduction plan runs after the first device test | rustc `unsafe_code`, `clippy::multiple_unsafe_ops_per_block`, ledger |
| `@unchecked Sendable` only for a type whose state sits behind a lock or wraps a thread-safe system object: `VideoEncoder`, `ScreenCapture`, `FramePipeline`, `ProbeRecorder` | ledger, *review* |

## Size and shape (all languages)

| Rule | Limit | Enforced by |
|---|---|---|
| Lines per file | 400 (research scripts in `bench/`: 500) | `scripts/limits.py` |
| Lines per shell script | 150; past that, use Python | `scripts/limits.py` (Google Shell Style Guide) |
| Line length | 120 | rustfmt, swift-format, ruff format |
| Lines per function | 50 | clippy `too_many_lines`, SwiftLint `function_body_length`, ruff `PLR0915` (statements) |
| Parameters per function | 6 | clippy `too_many_arguments`, SwiftLint `function_parameter_count`, ruff `PLR0913` |
| Cyclomatic complexity | 10 | SwiftLint `cyclomatic_complexity`, ruff `C901`. Rust: clippy `cognitive_complexity` at 15 |
| Nesting depth | 5 blocks; types nest at most 1 level | clippy `excessive_nesting`, SwiftLint `nesting`; Python through ruff `C901` |
| Tuple size | 2 named elements; use a struct beyond that | SwiftLint `large_tuple`, clippy `type_complexity` |
| Boolean parameters | At most 3 per function; prefer an enum or options struct | clippy `fn_params_excessive_bools`, ruff `FBT` |
| Whitespace | No tabs or trailing whitespace; files end with one newline | `scripts/limits.py`, `.editorconfig` |
| Spelling | Code and docs are spell-checked | `typos` (`_typos.toml`) |

## Comments and naming

| Rule | Enforced by |
|---|---|
| No comments or doc comments in code. Intent lives in names, types and tests; design reasons live in `docs/` | `scripts/comments.py` |
| No single-letter names except `i`, `j`, `n`, `x`, `y`, `z`, `w`, `h` | clippy `min_ident_chars`, SwiftLint `identifier_name` |
| No magic numbers in logic. Name constants next to their use, or in `link` for shared limits | *review* |
| Silence a lint only with a stated reason: `#[expect(lint, reason = "…")]` in Rust; `#[allow]` is rejected. Every `expect` is counted in the ledger | clippy `allow_attributes`, `allow_attributes_without_reason`, ledger |

## Rust

Edition 2024. The toolchain is pinned in `rust-toolchain.toml`, and `rust-version` matches it. Lints are set once in `[workspace.lints]` in `Cargo.toml`. Warnings are errors.

**Correctness and robustness**

| Rule | Lint |
|---|---|
| No `unwrap` or `expect` outside tests. Return an error, or recover (`PoisonError::into_inner`) | `clippy::unwrap_used`, `expect_used` |
| No `panic!`, `todo!`, `unimplemented!`, `dbg!` or `process::exit` in library code | `clippy::panic`, `todo`, `unimplemented`, `dbg_macro`, `exit` |
| Results are not silently dropped. Handle the error or log it through the `log` facade. A deliberate discard is a named binding, such as `let _receiver_gone = …`, and the ledger counts it | `clippy::let_underscore_must_use`, `unused_result_ok`, ledger |
| No `as` casts. Use `From`, `TryFrom` and, for raw handles, `expose_provenance` and `with_exposed_provenance` | `clippy::as_conversions`, `as_underscore`, pedantic `cast_*`, `trivial_casts`, `trivial_numeric_casts` |
| Code that parses bytes from the network or the Mac never indexes, slices or does unchecked arithmetic. It uses `split_first_chunk`, `get` and saturating or checked math, so hostile input cannot panic it: `link::message`, `link::receiver`, `link::annexb`, `link_helper::bridge` | `clippy::indexing_slicing`, `arithmetic_side_effects` (module level) |
| Match every enum variant by name, so a new variant can't fall into `_` unnoticed | `clippy::wildcard_enum_match_arm` |
| No shadowing a name with an unrelated value | `clippy::shadow_unrelated` |
| Lock guards drop as soon as possible, and never live across a `match` on the locked value | `clippy::significant_drop_tightening`, `significant_drop_in_scrutinee` |
| Shared pointers are cloned visibly: `Arc::clone(&x)` | `clippy::clone_on_ref_ptr` |
| Assertions say what failed | `clippy::missing_assert_message` |
| Floats are not compared exactly against constants; float literals are exact | `clippy::float_cmp_const`, `lossy_float_literal` |
| No `mem::forget`, `Rc<Mutex>`, or mutexes around a single integer (use atomics) | `clippy::mem_forget`, `rc_mutex`, `mutex_integer` |
| Errors are not discarded by `map_err(\|_\| …)` | `clippy::map_err_ignore` |

**API and style**

| Rule | Lint |
|---|---|
| Items are public only if reachable from outside the crate; otherwise `pub(crate)` | `unreachable_pub` |
| Inside an `impl`, the type is `Self` | `clippy::use_self` |
| Every public type implements `Debug` | `missing_debug_implementations` |
| Rust 2018+ idioms: elided lifetimes in paths are written `'_` | `rust_2018_idioms` |
| No unused lifetimes, redundant lifetimes or unit bindings; ASCII identifiers only | `unused_lifetimes`, `redundant_lifetimes`, `unit_bindings`, `non_ascii_idents` |
| Lint names in the config exist in the pinned toolchain | `unknown_lints`, `renamed_and_removed_lints` |
| All of clippy `pedantic` | `clippy::pedantic` |

**Dependencies and tests**

| Rule | Enforced by |
|---|---|
| Allowed licenses, no known advisories, crates.io only, no wildcard versions | `cargo deny` (`deny.toml`) |
| No unused dependencies | `cargo shear` |
| Host Rust (`link`, `link-helper`, `fake-headset`) keeps at least 85% line coverage | `cargo llvm-cov --fail-under-lines 85` |
| Every mutant of the protocol crates is caught by a test | `cargo mutants` (`scripts/check.sh mutants`, CI) |
| Wire formats shared across languages are pinned by golden vectors that both sides test: `crates/link-helper/tests/vectors.json` | Rust and Swift tests |
| Tests are named after the behavior they prove, and are deterministic: fixed seeds, port 0, waits with deadlines | *review* |

## Swift

Swift 6 language mode with strict memory safety, `ExistentialAny` and `MemberImportVisibility`. Warnings are errors.

| Rule | Enforced by |
|---|---|
| Format: 4 spaces, 120 columns, ordered imports, no semicolons | swift-format, strict mode (`.swift-format`) |
| No force unwraps, `try!`, `as!` or implicitly unwrapped optionals | swift-format `NeverForceUnwrap`, `NeverUseForceTry`, `NeverUseImplicitlyUnwrappedOptionals`; SwiftLint `force_cast`, `force_try`, `force_unwrapping`, `implicitly_unwrapped_optional` |
| UI code is `@MainActor`. Work on capture and encoder threads lives in separate types whose state sits behind a lock. Timers and signal handlers reach the main actor through `MainActor.assumeIsolated` | Swift 6 compiler, ledger |
| Capture `self` weakly in escaping closures; never `unowned` | SwiftLint `unowned_variable_capture` |
| Errors from `Task { try … }` are handled; `try?` is counted | SwiftLint `unhandled_throwing_task`, ledger |
| `catch` blocks name the error type they expect, or use the implicit `error` | SwiftLint `untyped_error_in_catch` |
| No `String(format:)`; fixed-point output goes through `FixedFormat`, which is locale-independent and memory-safe | strict memory safety |
| Modern Swift over Objective-C idioms: `isMultiple(of:)`, Swift types over `NSNumber`/`NSDictionary`, `first(where:)`, `isEmpty`, `contains(where:)` | SwiftLint `legacy_multiple`, `legacy_objc_type`, `first_where`, `empty_count`, `contains_over_*` |
| Multi-line closures name their parameters; unused parameters are `_`; a file is named after its main type | SwiftLint `anonymous_argument_in_multiline_closure`, `unused_parameter`, `file_name` |
| Metrics: function body 50 lines, type body 250, file 400, cyclomatic complexity 10, 6 parameters, tuples of 2 | SwiftLint metrics (`.swiftlint.yml`) |
| Pure logic keeps at least 90% line coverage | `scripts/swift_coverage.py` |

## Python

`scripts/` and `docs/images/` are tooling the gate depends on, so they are fully typed. `bench/` is research code: it gets the same lint rules, except type annotations and printing.

| Rule | Enforced by |
|---|---|
| Every ruff rule is on (`select = ["ALL"]`), except the few listed in `ruff.toml` | ruff |
| Fully typed, no implicit `Any`; explicit `Any` is counted | mypy `--strict` on `scripts/` and `docs/images/`, ledger |
| Logic that grows conditions moves out of shell into Python with unit tests: `eval_judge.py`, `limits.py`, `escape_hatches.py`, `agent_guard.py` | *review*, plus the 150-line shell limit |

## Shell

| Rule | Enforced by |
|---|---|
| bash only; executable scripts start with `set -euo pipefail` | `scripts/limits.py` |
| Every ShellCheck check, including the optional ones, except variable braces (SC2250) and masked command-substitution status (SC2312) | ShellCheck (`.shellcheckrc`) |
| Functions are never called inside `&&` or `\|\|` chains, where `set -e` is off and failures go unnoticed | ShellCheck SC2310 |
| Google style formatting: 2-space indent, indented `case` labels, `&&` and `\|` start continuation lines | shfmt (settings in `.editorconfig`) |
| `\|\| true` and `2> /dev/null` are counted | ledger |

## CI and supply chain

| Rule | Enforced by |
|---|---|
| Third-party actions are pinned to a full commit SHA, and Dependabot updates them weekly | zizmor `unpinned-uses`, `.github/dependabot.yml` |
| Workflows grant the least permissions, set per job; `actions/checkout` does not persist credentials | zizmor `excessive-permissions`, `artipacked` |
| No template injection from event data into `run:` steps; pass values through `env:` | zizmor `template-injection`, actionlint |
| Downloaded build inputs (OpenXR loader, actionlint, SwiftLint) are verified against a pinned SHA-256; every other tool is installed at a pinned version | `xr/build.sh`, `ci.yml`, `scripts/check.sh` |
| Commit subjects follow Conventional Commits, 100 characters at most | `scripts/commit_msg.py` (`commit-msg` hook, CI) |

Accepted exceptions:
- zizmor `dangerous-triggers` is ignored for `claude.yml` (`.github/zizmor.yml`). The `issue_comment` trigger is how `@claude` mentions work. The action only runs for users with write access, and the workflow never checks out pull request code.

## Considered, not adopted

| Recommendation | Why not |
|---|---|
| `clippy::undocumented_unsafe_blocks` (`// SAFETY:` comments) | A comment is trust, not a check, and it conflicts with the no-comments rule. Instead, `unsafe` is forbidden on the Mac side, confined to opted-in headset modules, one operation per block, counted in the ledger, and owned by the maintainer |
| `clippy::indexing_slicing` and `arithmetic_side_effects` outside the parsers | In the sender's enum-sized queue arrays and in pixel math they only add noise. Parsers, which see hostile input, do enforce them |
| `clippy::implicit_return`, `question_mark_used`, `std_instead_of_core`, `missing_inline_in_public_items`, `exhaustive_*`, `single_call_fn`, `absolute_paths` | Restriction lints that contradict idiomatic Rust or only matter for `no_std` or published crates |
| Swift `explicit_type_interface`, `explicit_acl`, `no_magic_numbers`, `missing_docs`, `contrasted_opening_brace`, `type_contents_order` | They contradict swift-format, the no-comments rule, or Swift's type inference. Magic numbers are a *review* rule |
| `InternalImportsByDefault` (Swift) | It would require `public import` on every framework the public API mentions. Revisit when `SkyscreensKit` gets other clients |
| Fuzzing with `cargo fuzz`, Miri | They need nightly Rust. The seeded property tests and mutation testing cover the parsers today; add a scheduled nightly job when the protocol grows |
| CodeQL, OpenSSF Scorecard workflow | Worth adding as scheduled jobs; they report rather than gate, so they are left for a separate change |
| Markdown and TOML linters | Docs are reviewed by hand for now. `typos` catches spelling |
