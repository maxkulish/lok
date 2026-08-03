# Spec: Fix slice panics on CI log truncation and out-of-range file:line references

**Created**: 2026-08-03
**Revised**: 2026-08-03 (second review pass - proof plan reworked, empty-container behavior added, seventh site found)
**Linear**: [CLO-633](https://linear.app/cloud-ai/issue/CLO-633)
**Estimated scope**: S/M (7 files, 4 sub-tasks)

## 1. Problem Statement

Seven slicing sites panic on ordinary inputs. None needs an attacker, and several are reachable from
input the tool reads by default. All seven were re-read in the working tree on 2026-08-03 and
confirmed. Linear names four; the other three were found by review of this spec and each was
verified by reading before being added here.

### Defect 1: byte-offset truncation is not on a character boundary (2 sites)

`src/tasks/ci.rs:198` (GitHub, `gh run view --log-failed`) and `src/tasks/ci.rs:276` (GitLab,
`glab ci trace`) both truncate a log to its last 15000 bytes:

```rust
let truncated = if log_text.len() > 15000 {
    format!("{}...\n[truncated, showing last 15000 chars]", &log_text[log_text.len() - 15000..])
} else {
    log_text
};
```

`log_text.len()` is a byte count, not a character count. When a multi-byte character straddles
`len() - 15000`, the index is not a `char` boundary and `&str` indexing panics, aborting `lok ci`.
CI logs carry box-drawing characters, emoji and non-ASCII test names as a matter of course, so the
offset landing mid-character is a question of log content, not of adversarial input. The
`[truncated, showing last 15000 chars]` label is also inaccurate - the budget is bytes.

### Defect 2: inverted range when the referenced line exceeds the file length (2 sites)

`src/tasks/context.rs:329-333` (inside `read_file_around_line`) and `src/tasks/fix.rs:189-198`
(an inline copy of the same logic, with `context_lines` hardcoded to 10) both compute:

```rust
let start = line.saturating_sub(context_lines);
let end = (line + context_lines).min(lines.len());
for (i, l) in lines[start..end].iter().enumerate() {
```

`start` is clamped only at zero and `end` only at the file length, so nothing keeps `start <= end`.
A reference to line 99999 in a 200-line file yields `start = 99989`, `end = 200`, and
`lines[99989..200]` panics with `slice index starts at 99989 but ends at 200`.

Both call sites are fed by `extract_file_references`, which harvests `file.rs:NNN` matches out of
GitHub issue and PR text with `([a-zA-Z0-9_/.-]+\.(rs|rb|py|...)):(\d+)` and parses the number with
`parse::<usize>().unwrap_or(0)`. So a single stale `src/main.rs:99999` in an issue body crashes
`lok context --issue N` (`src/tasks/context.rs:119`) and `lok fix --issue N`
(`src/tasks/fix.rs:182`). Nobody has to be hostile; the reference only has to be out of date.

`line + context_lines` is a second, independent defect on the same line: on a 64-bit target
`parse::<usize>()` accepts `18446744073709551615`, so `file.rs:18446744073709551615` overflows the
addition - a panic in debug builds, a wrap to a small `end` in release builds, which then panics on
the inverted slice anyway.

### Defect 3: two more byte-offset slices, truncating from the head (2 sites)

`src/main.rs:1902`, in the PR-review path:

```rust
let max_diff_chars = 50000;
let diff_for_review = if diff.len() > max_diff_chars {
    // ... "Note: Diff truncated from {} to {} chars"
    &diff[..max_diff_chars]
} else { &diff };
```

A diff larger than 50000 bytes whose byte 50000 falls mid-character panics. Diffs contain whatever
the source files contain, so any repository with non-ASCII content can trigger it. The "chars"
wording is wrong here too.

`src/tasks/implement.rs:583`, inside `clean_code_output`, sniffing a backend response for preamble
instead of code:

```rust
let first_150 = &code[..code.len().min(150)].to_lowercase();
```

The `.min(150)` bounds the index but does not put it on a character boundary. Any model response
longer than 150 bytes with a multi-byte character straddling byte 150 panics - and this input is
model output, so it is the least predictable of the seven.

Both want the head of the string, which is exactly what the existing `crate::utils::truncate_utf8`
(`src/utils.rs:235`) already does correctly. Neither needs a new helper.

### Defect 4: unchecked length slice on a git SHA (1 site)

`src/tasks/implement.rs:545`:

```rust
Ok(sha) => println!("      {} Committed {}", "●".green(), &sha[..8]),
```

This is a length bound rather than a UTF-8 boundary - a SHA is ASCII hex - but it panics just the
same when the string is shorter than 8 bytes, and it can be. `commit_file`
(`src/tasks/implement.rs:761-767`) builds its return value from `git rev-parse HEAD`:

```rust
let sha = AsyncCommand::new("git").args(["rev-parse", "HEAD"]).current_dir(dir).output().await?;
Ok(String::from_utf8_lossy(&sha.stdout).trim().to_string())
```

`head.status` is never checked. In a repository with no commits yet, `git rev-parse HEAD` fails,
stdout is empty, and `sha` is `""` - so `&sha[..8]` panics. `lok implement` against a freshly
initialized repository reaches this. Review raised the site as a scope question; the unborn-HEAD
path is what makes it belong here rather than in a follow-up.

### Defect 5: an empty window suppresses the keyword fallback and leaves a bare heading

This is the consequence of fixing Defect 2, and it has to be specified alongside it or the panic fix
trades a crash for silently degraded output.

Both callers push their parent heading *before* the loop that may produce nothing:

- `src/tasks/context.rs:117` pushes `## Referenced Files\n\n` whenever `file_refs` is non-empty.
- `src/tasks/fix.rs:180` pushes `## Referenced files from issue:\n\n` on the same condition.

In `fix.rs` this is load-bearing. The keyword-search fallback at `src/tasks/fix.rs:214` runs only
when nothing else was gathered:

```rust
if !keywords.is_empty() && context.is_empty() {
```

So an issue whose every file reference is stale, out of range, or names a file that does not exist
leaves `context` holding just the heading - non-empty - and the fallback never runs. `lok fix` then
sends the model a prompt containing one empty section and no code at all. This already happens today
for references to files that do not exist (the `if let Ok(content)` at `src/tasks/fix.rs:185` simply
skips), and clamping the window to empty would extend it to out-of-range references. `context.rs`
does not gate its keyword search this way and only suffers the cosmetic empty heading.

### Why the duplication matters

`src/tasks/fix.rs` does not call `read_file_around_line`; it inlines the same start/end computation
and the same marker-rendering loop. The two rendering loops are identical, character for character,
apart from variable names. That is the actual hazard here: a fix applied to one copy leaves the
other one panicking, which is how a single defect became two.

## 2. Acceptance Criteria

Every criterion below must be provable by a test that **fails against the current `main`**. A test
that passes before the change proves nothing about it; see the note on discriminating tests in
section 5.

- [ ] AC1: A log larger than 15000 bytes whose last-15000-bytes offset falls strictly inside a
      multi-byte character truncates without panicking, and the result is valid UTF-8 of at most
      15000 bytes (excluding the appended marker).
- [ ] AC2: Both `src/tasks/ci.rs` truncation sites call `utils::tail_utf8`. No `&log_text[..]` byte
      indexing remains in `src/tasks/ci.rs`.
- [ ] AC3: `line_window` always returns `start <= end <= total_lines`, for every combination of
      `line` and `context_lines` including `usize::MAX` for either, in both debug and release.
- [ ] AC4: A `file.rs:99999` reference against a 200-line file produces an empty window rather than
      panicking, and no section is emitted for it - no heading, no empty fenced block.
- [ ] AC5: A `file.rs:18446744073709551615` reference does not overflow in debug or release and
      produces an empty window. (64-bit targets; see AC5a.)
- [ ] AC5a: On a target where `usize` is 32 bits, `18446744073709551615` fails to parse and
      `extract_file_references`'s existing `unwrap_or(0)` yields line 0, which renders the head of
      the file with no marker. This is the pre-existing behavior and must be preserved, not
      "fixed".
- [ ] AC6: `src/tasks/context.rs` and `src/tasks/fix.rs` both render their numbered-line body
      through one shared helper. Neither file computes its own `start`/`end` any more.
- [ ] AC7: For every input where the current code produces a **non-empty** body, the rendered
      output is byte-identical to what it produces today. Proven by differential comparison against
      a test-local copy of the current renderer over a bounded sweep including a 200-line file, not
      by spot examples.
- [ ] AC7a: Where the current code produces an **empty** body without panicking - an existing but
      empty file, or a line far enough past EOF that `start == end` - the new behavior deliberately
      differs: today both callers still emit a `### path` heading and an empty fenced block, and
      after this change they emit nothing at all. This is the carve-out that makes AC4 and AC8
      possible; AC7 is scoped around it rather than contradicting it.
- [ ] AC8: When no file section renders, the parent heading is not emitted either, and in
      `src/tasks/fix.rs` the keyword-search fallback runs as if no references had been found.
      Covered for three cases: all references invalid, a mix of valid and invalid, and a reference
      to a file that does not exist.
- [ ] AC9: `src/main.rs`'s PR-diff truncation and `clean_code_output`
      (`src/tasks/implement.rs:565`) both truncate on a character boundary. A 60000-byte diff and a
      200-byte model response, each with a multi-byte character straddling the cut, are handled
      without panicking.
- [ ] AC10: `&sha[..8]` (`src/tasks/implement.rs:545`) does not panic when the SHA is shorter than
      8 bytes, including the empty string. Proven by a direct test of the extracted `short_sha`.
      The `println!` call site itself is covered by the row-24 grep, not by a unit test: a test can
      pin the behavior of the function that holds the slice, but it cannot assert that a given line
      calls that function. Extracting a further display-path wrapper only moves the same gap one
      level up, so the slice is confined to one tested function and the wiring is checked by grep.
- [ ] AC11: None of the seven defect sites performs a raw byte-offset slice of a `String`/`&str`
      any more, and each one positively calls its intended helper. Scoped to exactly those seven
      sites - this is not a repository-wide prohibition, and other byte slices elsewhere are out of
      scope.
- [ ] AC12: `cargo test` passes, `cargo clippy --all-targets -- -D warnings` is clean,
      `cargo fmt --check` is clean.

**Verification method**: AC1, AC3, AC5, AC7 and AC10 are proven by unit tests on pure functions in
`src/utils.rs`'s existing `#[cfg(test)] mod tests`. AC4 and AC8 are proven by unit tests on the
extracted, filesystem-free section builders described in sub-task 3. AC9 is proven by a direct test
of `clean_code_output` and of the extracted `truncate_diff_for_review`. AC2, AC6 and AC11 are proven
by the grep assertions in section 5, which include positive call-site assertions and not only
negative ones. AC12 is the existing CI Gate.

## 3. Constraints

**Must**:
- Put the new general-purpose helpers in `src/utils.rs`, beside the existing `truncate_utf8`
  (`src/utils.rs:235`), and test them in that file's existing `#[cfg(test)] mod tests`. The
  codebase already keeps exactly this kind of helper there.
- Keep the rendered body byte-identical wherever the current code produces a non-empty body (AC7,
  and see the AC7a carve-out for empty ones). This is a panic fix, not a formatting change; the
  off-by-one asymmetry in the existing window (`start` is `line - context_lines` as a 0-based
  index, so the window opens at displayed line `line - context_lines + 1`) is behavior to preserve,
  not a bug to correct in this task.
- Use `str::is_char_boundary` for the boundary walk. Stable since Rust 1.9, so it works regardless
  of what MSRV this crate settles on.
- Use `saturating_add` for the `line + context_lines` arithmetic.
- Make every new test discriminating: it must fail if applied to the current `main`. Where the
  natural unit under test is already correct (`truncate_utf8`), test the *caller* instead, or
  extract the caller's logic until it is testable.

**Must-not**:
- Do not use `str::floor_char_boundary` / `ceil_char_boundary`. Both carry
  `#[stable(feature = "round_char_boundary", since = "1.91.0")]` - verified in this machine's
  sysroot at `core/src/str/mod.rs:423` - and `Cargo.toml:5` declares `rust-version = "1.80"`.
  Nothing in `.github/workflows/ci.yml` builds against the declared MSRV and the oldest toolchain
  installed here is 1.94, so a violation would be caught neither locally nor in CI; it would
  surface as a build failure for a crates.io consumer after Phase 13 publishes. A three-line
  `is_char_boundary` walk costs nothing and moots the question. (One AI reviewer asserted 1.77.0
  for this stabilization and concluded the constraint was unnecessary. The version is wrong; the
  constraint stands.)
- Do not raise `rust-version` in `Cargo.toml` as part of this task, and do not add an MSRV CI job
  here. Whether the crate as a whole still builds on 1.80 is unverified and unrelated to this fix;
  the constraint above is only about not making it worse. Filed as a follow-up.
- Do not change the `FILE_REF_RE` regex or `extract_file_references`. Rejecting implausible line
  numbers at parse time is a different fix; the slicing sites have to be safe for any `usize`
  regardless of what the parser lets through. This is also what keeps AC5a true.
- Do not fold the duplicated `extract_file_references` / `FILE_REF_RE` pair
  (`src/tasks/context.rs:16,256` and `src/tasks/fix.rs:12,243`) into `src/utils.rs` in this task.
  Review raised it and it is a genuine instance of the same drift hazard, but it is a parser, not a
  panic, and consolidating it widens the diff of a fix that should stay reviewable. Filed as a
  follow-up.
- Do not fix `commit_file`'s unchecked `head.status` (`src/tasks/implement.rs:761-767`). AC10 makes
  the *slice* safe, which is this task's remit. That `commit_file` reports an unborn HEAD as a
  successful empty SHA is a separate error-handling defect. Filed as a follow-up.
- Do not change `truncate_utf8`'s behavior. It truncates from the head and has its own callers; the
  CI sites need the tail, which is why `tail_utf8` is a sibling rather than a parameter on it.

**Prefer**:
- Pure, filesystem-free functions wherever a panic is being fixed, so the fix is testable without
  fixtures. This is what makes AC4, AC8 and AC9 provable at all.
- Correcting the two `chars`-should-be-`bytes` labels (`src/tasks/ci.rs`, `src/main.rs:1896`) while
  those lines are being edited.

**Escalate when**:
- Preserving byte-identical output (AC7) conflicts with the shared-helper extraction - that would
  mean the two copies are not as identical as they read, and the divergence needs a decision rather
  than a silent pick.
- Any existing test depends on the panicking behavior, on the empty parent heading, or on the
  fallback being suppressed.

## 4. Decomposition

1. **`tail_utf8` helper + CI truncation sites**: add
   `pub fn tail_utf8(s: &str, max_bytes: usize) -> &str` to `src/utils.rs` next to `truncate_utf8`.
   Return `s` unchanged when `s.len() <= max_bytes`; otherwise start at `s.len() - max_bytes` and
   walk forward while `!s.is_char_boundary(start)`, then slice. The walk terminates because
   `is_char_boundary(s.len())` is always true, and advances at most 3 bytes. Replace both
   `&log_text[log_text.len() - 15000..]` expressions in `src/tasks/ci.rs` (lines 198 and 276) with
   the helper, and fix the "chars" label to "bytes".
   Files: `src/utils.rs`, `src/tasks/ci.rs`.

2. **`line_window` helper**: add
   `pub fn line_window(total_lines: usize, line: usize, context_lines: usize) -> (usize, usize)`
   to `src/utils.rs`, computing
   `end = line.saturating_add(context_lines).min(total_lines)` then
   `start = line.saturating_sub(context_lines).min(end)`. Clamping `start` against `end` rather than
   against `total_lines` is what makes the range non-inverted in every case.
   Files: `src/utils.rs`.

3. **`render_line_window` + section builders + both call sites**: add
   `pub fn render_line_window(lines: &[&str], line: usize, context_lines: usize) -> Option<String>`
   to `src/utils.rs`, built on `line_window`, returning `None` when the window is empty and
   otherwise emitting exactly `format!("{} {:4}: {}\n", marker, line_num, l)` with
   `line_num = start + i + 1` and `marker` of `">>>"` when `line_num == line` and `"   "` otherwise
   - the current format, unchanged. Returning `Option` rather than a possibly-empty `String` is what
   makes "no empty fenced block" a property of the type rather than a rule each caller has to
   remember.

   Then, in each caller, extract the whole parent block into a pure, filesystem-free function that
   takes already-read file contents and returns the assembled section text, empty when nothing
   rendered:
   - `src/tasks/context.rs`: `fn render_referenced_files(files: &[(&str, usize, &str)]) -> String`,
     emitting `## Referenced Files\n\n` plus each `### {path} (line {n})` fenced block, and `""`
     when no file yields a window. `read_file_around_line` becomes a thin wrapper over
     `render_line_window`.
   - `src/tasks/fix.rs`: `fn render_issue_file_sections(files: &[(&str, usize, &str)]) -> String`,
     emitting `## Referenced files from issue:\n\n` plus each `### {path}` heading (with
     ` (around line {n})` only when `n > 0`) and fenced block, and `""` when nothing renders.

   The async callers keep the I/O: read each referenced file, collect the `(path, line, content)`
   tuples that read successfully, hand the slice to the builder, and push the result. Because the
   builder returns `""` when nothing rendered, `fix.rs`'s existing
   `if !keywords.is_empty() && context.is_empty()` at line 214 starts working correctly with no
   change to that line - the heading is no longer there to make `context` spuriously non-empty.
   Files: `src/utils.rs`, `src/tasks/context.rs`, `src/tasks/fix.rs`.

4. **Head-truncation sites and the SHA slice**:
   - `src/main.rs`: extract `fn truncate_diff_for_review(diff: &str, max_bytes: usize) -> &str`
     (a one-line wrapper over `utils::truncate_utf8`) so the PR-review path has something testable,
     call it at line 1902, and correct the "chars" wording at line 1896. `src/main.rs` already has a
     `#[cfg(test)] mod tests` at line 2267 for the test to live in.
   - `src/tasks/implement.rs:583`: replace `&code[..code.len().min(150)]` with
     `crate::utils::truncate_utf8(code, 150)`. `clean_code_output` is already a pure
     `fn(&str) -> Option<String>`, so it is directly testable as-is.
   - `src/tasks/implement.rs:545`: replace `&sha[..8]` with a length-safe short SHA -
     `utils::truncate_utf8(&sha, 8)` reuses the existing helper and is correct for ASCII hex and for
     the empty string alike.
   Files: `src/main.rs`, `src/tasks/implement.rs`.

**Dependency order**: sub-task 3 depends on sub-task 2 (`render_line_window` calls `line_window`).
Sub-tasks 1 and 4 are independent of everything, and of each other.

## 5. Evaluation

**Discriminating-test rule**: before marking a test done, confirm it fails on the pre-change code.
`git stash` the source change, leave the test, run it, and see it panic or mismatch. Rows below
marked **(D)** are the ones that carry the actual proof; rows marked (R) are regression guards that
may pass either way and are there to catch drift, not to demonstrate the fix.

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | (R) `tail_utf8` on a string shorter than `max_bytes` | returns input unchanged | `cargo test tail_utf8` |
| 2 | **(D)** `tail_utf8("😀".repeat(5000) + "x", 15000)` - length 20001, so the split at byte 5001 falls strictly inside an emoji | no panic; valid UTF-8; `len() <= 15000`; result does not begin with a partial char | `cargo test tail_utf8` |
| 3 | (R) `tail_utf8("😀".repeat(5000), 15000)` - split at byte 5000, already a boundary | no panic; `len() == 15000`. Kept only to pin the aligned case; it does **not** exercise the walk, which is why row 2 exists | `cargo test tail_utf8` |
| 4 | (R) `tail_utf8(s, 0)` and `tail_utf8("", 15000)` | `""` and `""`, no panic | `cargo test tail_utf8` |
| 5 | **(D)** `line_window(200, 99999, 10)` | `(200, 200)` - empty, not inverted | `cargo test line_window` |
| 6 | **(D)** `line_window(200, usize::MAX, 10)` | `(200, 200)`, no overflow | `cargo test line_window` |
| 7 | **(D)** `line_window(200, 10, usize::MAX)` | `(0, 200)`, no overflow | `cargo test line_window` |
| 8 | (R) `line_window(0, 1, 15)` on an empty file | `(0, 0)` | `cargo test line_window` |
| 9 | **(D)** invariant sweep: `start <= end <= total_lines` over the cross product of `total_lines`, `line`, `context_lines` in `{0,1,2,10,15,199,200,usize::MAX-1,usize::MAX}` | holds for all combinations | `cargo test line_window` |
| 10 | **(D)** AC7 differential sweep: a test-local copy of the current renderer (`start = line.saturating_sub(c)`, `end = (line+c).min(len)`, same format string) compared against `render_line_window` over every combination that does not panic under the legacy version - `total_lines` in `{0,1,5,200}`, `line` in `{0,1,2,5,199,200,201}`, `context_lines` in `{10,15}` (both production sizes) | byte-identical output wherever legacy does not panic; `None` wherever it does | `cargo test render_line_window` |
| 11 | (R) `render_line_window(&["a","b","c","d","e"], 3, 1)` | `Some(">>>    3: c\n       4: d\n")` - `line_window(5,3,1)` gives `(2,4)`, and a non-marker prefix is `"   "` + space + `{:4}`, so seven leading spaces | `cargo test render_line_window` |
| 12 | **(D)** `render_line_window` with `line = 99999` against 5 lines | `None`, no panic | `cargo test render_line_window` |
| 13 | (R) `render_line_window(&["a","b","c"], 0, 1)` | `Some("       1: a\n")` - no `>>>` anywhere; preserves the `unwrap_or(0)` path (AC5a) | `cargo test render_line_window` |
| 14 | **(D)** `render_issue_file_sections` with every reference out of range | `""` - no heading, no fences | `cargo test render_issue_file_sections` |
| 15 | **(D)** `render_issue_file_sections` with one valid and two out-of-range references | heading present exactly once, exactly one `###` section, no empty fences | `cargo test render_issue_file_sections` |
| 16 | **(D)** `render_issue_file_sections` with an empty input slice (stands in for every referenced file failing to read) | `""` | `cargo test render_issue_file_sections` |
| 17 | **(D)** `render_referenced_files` - same three cases as rows 14-16 against `context.rs`'s framing | `""`, one section, `""` respectively | `cargo test render_referenced_files` |
| 18 | **(D)** AC8 end to end: `context.is_empty()` is true after `render_issue_file_sections` returns `""`, so the keyword fallback branch is taken | fallback branch reached | `cargo test` (assert on the builder's output feeding the same condition) |
| 19 | **(D)** `clean_code_output` on a 200-byte response with a multi-byte char straddling byte 150 | returns without panicking | `cargo test clean_code_output` |
| 20 | **(D)** `truncate_diff_for_review` on a 60000-byte diff with a multi-byte char at byte 50000 | no panic; valid UTF-8; `len() <= 50000` | `cargo test truncate_diff_for_review` |
| 21 | **(D)** short-SHA rendering for `""`, `"abc"`, and a full 40-char SHA | `""`, `"abc"`, first 8 chars; no panic | `cargo test` |
| 22 | (R) No byte indexing left at the defect sites | no output | `rg -n 'log_text\[' src/tasks/ci.rs` and `rg -n 'diff\[\.\.\|code\[\.\.\|sha\[\.\.' src/main.rs src/tasks/implement.rs` |
| 23 | (R) Neither file caller computes its own window | no output | `rg -n 'saturating_sub\(context_lines\)\|saturating_sub\(10\)' src/tasks/context.rs src/tasks/fix.rs` |
| 24 | (R) **Positive** wiring assertions - each defect site calls its helper. These must match *call sites*, not helper bodies or tests. After sub-tasks 1 and 4 extracted `truncate_log` and `truncate_diff_for_review`, counting `tail_utf8` in `ci.rs` proves nothing about the two call sites, since the only remaining hit is inside the helper; and a bare `truncate_log(` also matches the `fn` definition and the test. Hence the argument-anchored patterns below | 2, 1, 1, 1, 1, 1 respectively | `rg -c 'truncate_log\(log_text,' src/tasks/ci.rs; rg -c 'render_line_window' src/tasks/context.rs; rg -c 'render_line_window' src/tasks/fix.rs; rg -c 'truncate_diff_for_review\(&diff, max_diff_bytes\)' src/main.rs; rg -c 'short_sha\(&sha\)' src/tasks/implement.rs; rg -c 'truncate_utf8\(code, 150\)' src/tasks/implement.rs` |
| 25 | (R) Full suite, lints, formatting | all pass | `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` |
| 26 | **(D)** Overflow absent in release, not merely masked | rows 6, 7 and 9 pass under release | `cargo test --release line_window` |

**Edge cases to verify**:
- `max_bytes` larger than the string, equal to the string length, and zero.
- A string that is entirely one multi-byte character truncated to fewer bytes than that character
  occupies - `tail_utf8` must return `""`, not a broken slice.
- `line = 0`, which `extract_file_references` produces via `unwrap_or(0)` whenever the digits do not
  fit a `usize`. It must keep rendering the head of the file with no `>>>` marker (AC5a).
- `line` exactly equal to `lines.len()`, and `line = lines.len() + 1` - the boundary either side of
  the last real line.
- An empty file (`lines.len() == 0`) with any `line`.
- A referenced file that exists but is empty, versus one that does not exist at all - both must
  contribute no section (AC8).
- Debug and release builds both, since integer overflow panics only in debug and the release path
  fails differently.

## 6. Follow-ups (file at completion, do not do here)

1. `commit_file` (`src/tasks/implement.rs:761-767`) ignores `git rev-parse HEAD`'s exit status and
   returns an empty string as a successful SHA on an unborn HEAD. AC10 makes the slice safe; the
   error handling is still wrong.
2. `extract_file_references` and `FILE_REF_RE` are duplicated verbatim across
   `src/tasks/context.rs:16,256` and `src/tasks/fix.rs:12,243` - the same drift hazard that turned
   Defect 2 into two sites.
3. No CI job builds against the declared `rust-version = "1.80"`, and the oldest toolchain installed
   locally is 1.94, so the MSRV in `Cargo.toml` is an unverified claim. It matters more once Phase 13
   publishes to crates.io.
4. `/pr:review` Step 9.5's re-review poll cannot succeed against Qodo. It waits for a *review*
   object whose `commit_id` equals the new head, but Qodo edits its existing review comment in
   place and submits no new review - the same document already says so two sections earlier, then
   gives a poll that contradicts it. Observed on PR #80: `/agentic_review` at 13:52:13Z produced a
   comment update at 13:54:52Z with the findings recount, while `pulls/80/reviews` still held only
   the original 13:46:39Z review against the superseded SHA. The poll ran to its full timeout on a
   re-review that had already succeeded. The correct signal is `issues/comments/{id}.updated_at`
   moving past the request timestamp on the existing review comment. Same untested-shell-in-
   markdown family as CLO-623 and CLO-624.
