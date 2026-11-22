# Code standards

These are the rules every change to skyscreens follows. Each rule comes from a published industry source, such as a language's own guidelines, a widely used lint set, or a security baseline. Each one names the tool and setting that enforces it. `scripts/check.sh` runs all of them, and CI runs the same script. A rule marked *review* has no tool yet, so reviewers and agents check it by hand.

The last section lists recommendations we looked at and decided not to adopt, with the reason. Revisit those when the code changes.

Sources:
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) and the [Clippy lint list](https://rust-lang.github.io/rust-clippy/)
- Embark Studios' [shared Rust lint set](https://github.com/EmbarkStudios/rust-ecosystem/blob/main/lints.toml)
- The [Swift API Design Guidelines](https://www.swift.org/documentation/api-design-guidelines/) and [Swift 6 data-race safety](https://www.swift.org/migration/documentation/migrationguide/)
- [SwiftLint](https://realm.github.io/SwiftLint/rule-directory.html) and [swift-format](https://github.com/swiftlang/swift-format)
- [Ruff](https://docs.astral.sh/ruff/rules/) and [mypy strict mode](https://mypy.readthedocs.io/en/stable/command_line.html#cmdoption-mypy-strict)
- The [Google Shell Style Guide](https://google.github.io/styleguide/shellguide.html) and [ShellCheck](https://www.shellcheck.net)
- The [OpenSSF Scorecard](https://github.com/ossf/scorecard) checks and [zizmor](https://docs.zizmor.sh) for GitHub Actions
- [Conventional Commits](https://www.conventionalcommits.org)

## Size and shape (all languages)

| Rule | Limit | Enforced by |
|---|---|---|
| Lines per file | 400 (research scripts in `bench/`: 500) | `scripts/limits.py` |
| Lines per shell script | 150; past that, use Python | `scripts/limits.py` (Google Shell Style Guide) |
| Line length | 120 | rustfmt, swift-format, ruff format |
| Lines per function | 50 | clippy `too_many_lines`, SwiftLint `function_body_length`, ruff `PLR0915` (statements) |
| Parameters per function | 6 (C ABI functions exempt: the C signature fixes them) | clippy `too_many_arguments`, SwiftLint `function_parameter_count`, ruff `PLR0913` |
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
| Names say what a thing is. No single-letter names in Swift except `i`, `j`, `n`, `x`, `y`, `z`, `w`, `h` | SwiftLint `identifier_name` |
| No magic numbers in logic. Name constants next to their use, or in `link` for shared limits | *review* |
| Silence a lint only with a stated reason: `#[expect(lint, reason = "…")]` in Rust; a config entry with its reason in this file for the other tools | clippy `allow_attributes`, `allow_attributes_without_reason` |

## Rust

Edition 2024. The toolchain is pinned in `rust-toolchain.toml`, and `rust-version` matches it. Lints are set once in `[workspace.lints]` in `Cargo.toml`. Warnings are errors.

**Correctness and robustness**

| Rule | Lint |
|---|---|
| No `unwrap` or `expect` outside tests. Return an error, or recover (`PoisonError::into_inner`) | `clippy::unwrap_used`, `clippy::expect_used` |
| No `panic!`, `todo!`, `unimplemented!`, `dbg!` or `process::exit` in library code | `clippy::panic`, `todo`, `unimplemented`, `dbg_macro`, `exit` |
| No printing from libraries. Only binaries print, and they say so with `#![expect]` | `clippy::print_stdout`, `print_stderr` |
| Lossy numeric casts are explicit (`try_from`, `from`); `as _` is banned | clippy pedantic `cast_*`, `clippy::as_underscore`, `trivial_casts`, `trivial_numeric_casts` |
| Match every enum variant by name, so a new variant can't fall into `_` unnoticed | `clippy::wildcard_enum_match_arm` |
| No shadowing a name with an unrelated value | `clippy::shadow_unrelated` |
| Lock guards drop as soon as possible, and never live across a `match` on the locked value | `clippy::significant_drop_tightening`, `significant_drop_in_scrutinee` |
| Shared pointers are cloned visibly: `Arc::clone(&x)` | `clippy::clone_on_ref_ptr` |
| Assertions say what failed | `clippy::missing_assert_message` |
| Floats are not compared exactly against constants; float literals are exact | `clippy::float_cmp_const`, `lossy_float_literal` |
| No `mem::forget`, `Rc<Mutex>`, or mutexes around a single integer (use atomics) | `clippy::mem_forget`, `rc_mutex`, `mutex_integer` |
| Errors are not discarded by `map_err(|_| …)` | `clippy::map_err_ignore` |

**`unsafe`**

| Rule | Lint |
|---|---|
| `unsafe` is denied by default. A module that needs it opts in at the top with `#![expect(unsafe_code, reason = "…")]`. Today: `link::ffi`, and in `xr`: `android`, `runtime`, `app`, `gpu`, `decoder` | `unsafe_code` |
| One unsafe operation per `unsafe` block, so each one is reviewable on its own | `clippy::multiple_unsafe_ops_per_block` |
| `unsafe` functions wrap each unsafe call in its own block | `unsafe_op_in_unsafe_fn` |
| C ABI functions check pointers for null before use | *review* (`ffi.rs` pattern: `handle()`, `slice()`) |

**API and style**

| Rule | Lint |
|---|---|
| Items are public only if reachable from outside the crate; otherwise `pub(crate)` | `unreachable_pub` |
| Inside an `impl`, the type is `Self` | `clippy::use_self` |
| Every public type implements `Debug` | `missing_debug_implementations` |
| Rust 2018+ idioms: elided lifetimes in paths are written `'_` | `rust_2018_idioms` |
| No unused lifetimes, redundant lifetimes or unit bindings | `unused_lifetimes`, `redundant_lifetimes`, `unit_bindings` |
| ASCII identifiers only | `non_ascii_idents` |
| Lint names in the config exist in the pinned toolchain | `unknown_lints`, `renamed_and_removed_lints` |
| All of clippy `pedantic`: lossy casts, needless borrows, `must_use` results and more | `clippy::pedantic` |

**Dependencies and tests**

| Rule | Enforced by |
|---|---|
| Allowed licenses, no known advisories, crates.io only, no wildcard versions | `cargo deny` (`deny.toml`) |
| No unused dependencies | `cargo shear` |
| `rust-version` matches the pinned toolchain | `rust-toolchain.toml`, `Cargo.toml` |
| Host Rust (`link`, `fake-headset`) keeps at least 85% line coverage | `cargo llvm-cov --fail-under-lines 85` |
| Tests are named after the behavior they prove, and are deterministic: fixed seeds, port 0, waits with deadlines | *review* |

## Swift

Swift 6 language mode, so data-race safety is checked at compile time. The `ExistentialAny` and `MemberImportVisibility` upcoming features are on. Warnings are errors.

| Rule | Enforced by |
|---|---|
| Format: 4 spaces, 120 columns, ordered imports, no semicolons | swift-format, strict mode (`.swift-format`) |
| No force unwraps, `try!`, `as!` or implicitly unwrapped optionals | swift-format `NeverForceUnwrap`, `NeverUseForceTry`, `NeverUseImplicitlyUnwrappedOptionals`; SwiftLint `force_cast`, `force_try`, `force_unwrapping`, `implicitly_unwrapped_optional` |
| Capture `self` weakly in escaping closures; never `unowned` | SwiftLint `unowned_variable_capture` |
| Errors from `Task { try … }` are handled, not dropped | SwiftLint `unhandled_throwing_task` |
| UI code is `@MainActor`. Work on capture and encoder threads lives in separate types whose state sits behind a lock. Timers and signal handlers reach the main actor through `MainActor.assumeIsolated` | Swift 6 compiler |
| `@unchecked Sendable` only for a type whose mutable state is behind a lock, or that wraps a thread-safe system object. Today: `LinkServer` (the Rust server locks internally), `VideoEncoder` (VTCompressionSession), `ScreenCapture`, `FramePipeline` and `ProbeRecorder` (NSLock) | *review* |
| `catch` blocks name the error type they expect, or use the implicit `error` | SwiftLint `untyped_error_in_catch` |
| Existential types are written `any P` | `ExistentialAny` upcoming feature |
| Each file imports what it uses | `MemberImportVisibility` upcoming feature |
| Modern Swift over Objective-C idioms: `isMultiple(of:)`, Swift types over `NSNumber`/`NSDictionary`, `first(where:)`, `isEmpty`, `contains(where:)`, `sorted().first` → `min()` | SwiftLint `legacy_multiple`, `legacy_objc_type`, `first_where`, `empty_count`, `contains_over_*`, `sorted_first_last` |
| Multi-line closures name their parameters instead of using `$0` | SwiftLint `anonymous_argument_in_multiline_closure` |
| Unused parameters are removed or named `_` | SwiftLint `unused_parameter` |
| A file is named after the main type it declares | SwiftLint `file_name` |
| No optional `Bool` or optional collections; no `fatalError` without a message | SwiftLint `discouraged_optional_boolean`, `discouraged_optional_collection`, `fatal_error_message` |
| Metrics: function body 50 lines, type body 250, file 400, cyclomatic complexity 10, 6 parameters, tuples of 2 | SwiftLint metrics (`.swiftlint.yml`) |
| The rest of SwiftLint's defaults, plus the opt-in rules listed in `.swiftlint.yml` | SwiftLint |

## Python

`scripts/` and `docs/images/` are tooling the gate depends on, so they are fully typed. `bench/` is research code: it gets the same lint rules, except type annotations and printing.

| Rule | Enforced by |
|---|---|
| Every ruff rule is on (`select = ["ALL"]`), except the few listed with reasons in `ruff.toml` | ruff |
| Format | ruff format |
| Fully typed: annotations everywhere, no implicit `Any`, strict optional checks | mypy `--strict` on `scripts/` and `docs/images/` |
| Use `pathlib` over `os.path` and `open()`; no shell strings in `subprocess` | ruff `PTH`, `S` |
| Logic in shell scripts that grows conditions moves to Python with unit tests (for example the eval judge, `scripts/eval_judge.py`, and the limits check, `scripts/limits.py`) | *review*, plus the 150-line shell limit |

## Shell

| Rule | Enforced by |
|---|---|
| bash only, starting with `set -euo pipefail` | `scripts/limits.py` |
| Every ShellCheck check, including the optional ones, except variable braces (SC2250) and masked command-substitution status (SC2312) | ShellCheck (`.shellcheckrc`) |
| Functions are never called inside `&&` or `\|\|` chains, where `set -e` is off and failures go unnoticed | ShellCheck SC2310 |
| Google style formatting: 2-space indent, indented `case` labels, `&&` and `\|` start continuation lines | shfmt (settings in `.editorconfig`) |
| At most 150 lines per script | `scripts/limits.py` |

## CI and supply chain

| Rule | Enforced by |
|---|---|
| Third-party actions are pinned to a full commit SHA, and Dependabot updates them weekly | zizmor `unpinned-uses`, `.github/dependabot.yml` |
| Workflows grant the least permissions, set per job | zizmor `excessive-permissions` |
| `actions/checkout` does not persist credentials unless a job pushes | zizmor `artipacked` |
| No template injection from event data into `run:` steps; pass values through `env:` | zizmor `template-injection`, actionlint |
| Downloaded build inputs (OpenXR loader, actionlint, SwiftLint) are verified against a pinned SHA-256; every other tool is installed at a pinned version | `xr/build.sh`, `ci.yml`, `scripts/check.sh` |
| Commit subjects follow Conventional Commits, 100 characters at most | `scripts/commit_msg.py` (`commit-msg` hook, CI) |
| The C header is generated, never edited | `cbindgen` drift check in `scripts/check.sh rust` |

Accepted exceptions:
- zizmor `dangerous-triggers` is ignored for `claude.yml` (`.github/zizmor.yml`). The `issue_comment` trigger is how `@claude` mentions work. The action only runs for users with write access, and the workflow never checks out pull request code.

## Considered, not adopted

| Recommendation | Why not |
|---|---|
| `clippy::undocumented_unsafe_blocks` (`// SAFETY:` comments) | It conflicts with the no-comments rule. We get the same reviewability from `unsafe_code` scoping, one operation per block, and safe wrappers named after their contract |
| `clippy::min_ident_chars` (Rust) | It's in clippy's restriction group, not idiomatic Rust: short closure and pattern names (`\|e\|`, `s`) are normal and scoped. Swift adopts its equivalent, because SwiftLint enables it by default |
| `clippy::indexing_slicing`, `arithmetic_side_effects` | Too noisy for parsing and pixel code that checks bounds first. Seeded property tests instead prove that arbitrary input never panics the decoder or reassembler |
| `clippy::as_conversions` (all `as`) | Lossy casts are already caught by pedantic `cast_*`. What remains is lossless enum-to-integer and raw-handle casts that FFI needs |
| `clippy::let_underscore_must_use`, `let_underscore_drop` | The remaining `let _ =` lines discard errors that cannot be acted on (a socket option, a joined thread at shutdown). Review them case by case |
| `clippy::implicit_return`, `question_mark_used`, `std_instead_of_core`, `missing_inline_in_public_items`, `exhaustive_*`, `single_call_fn`, `absolute_paths` | Restriction lints that contradict idiomatic Rust or only matter for `no_std` or published crates |
| Swift `explicit_type_interface`, `explicit_acl`, `no_magic_numbers`, `missing_docs`, `contrasted_opening_brace`, `type_contents_order` | They contradict swift-format, the no-comments rule, or Swift's type inference. Magic numbers are a *review* rule |
| `InternalImportsByDefault` (Swift) | It would require `public import` on every framework the public API mentions. Revisit when `SkyscreensKit` gets other clients |
| Coverage gate for Swift | Most of `SkyscreensKit` drives hardware and permissions. `scripts/eval.sh mac` covers it end to end instead |
| Mutation testing (`cargo mutants`), fuzzing with `cargo fuzz`, Miri | Too slow for every commit; fuzzing and Miri need nightly. Candidates for a scheduled job once the protocol stabilizes |
| CodeQL, OpenSSF Scorecard workflow | Worth adding as scheduled jobs. Not linter rules, so they are left for a separate change |
| Markdown and TOML linters | Docs are reviewed by hand for now. `typos` catches spelling |
