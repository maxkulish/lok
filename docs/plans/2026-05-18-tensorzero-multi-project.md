# TensorZero Multi-Project Promotion Plan

**Date**: 2026-05-18
**Status**: Draft (rev 2 — auth model corrected, data-privacy phase added, port binding tightened)
**Scope**: Promote the TensorZero Tier-2 stack (currently embedded in Loker) into a shared inference + observability layer usable by any orchestrator project (Loker, Lok, agtx, ralph-tui, future).
**Driven by**: Cross-project observability gap. Today each project that calls Anthropic / OpenAI / Google must re-implement its own request logging, cost tracking, and fallback routing. Loker already runs a working TensorZero stack (gateway + ClickHouse + UI) but the config + deployment live inside the Loker repo at `loker/deploy/tensorzero/` and `loker/tensorzero/config/tensorzero.toml`, so other projects cannot consume it without copy-paste.
**Depends on**: Loker's existing Tier-2 deployment (CLO-243 spike + CLO-247 / CLO-251 family-naming work).

---

## Goals

1. One TensorZero instance serves N projects. Single ClickHouse, single UI, single SQL surface for cost + latency + failure analytics.
2. Configuration lives outside any single project repo. Adding a new function for a new project does not require editing Loker.
3. Production-grade enough that a project can rely on it for inference (TLS, auth, backups, alerting), but the door stays open for projects to bypass it on failure.
4. A naming convention prevents the ClickHouse view from becoming an unsearchable soup once 5+ projects are emitting traffic.

## Non-Goals

- Replacing direct provider SDK usage in every project today. Migration is opt-in per project.
- Building a TensorZero alternative or forking. We use upstream `tensorzero/gateway:2026.4.1` and `tensorzero/ui:2026.4.1`.
- Multi-tenant auth with per-project API keys + RBAC. Phase 1 uses a single shared gateway token; per-project auth is a future task if and when it matters.
- Horizontal scaling of the gateway. Single instance is sufficient for the foreseeable load.

---

## Current State

| Artifact | Location | Owner | Problem |
|---|---|---|---|
| Docker compose | `loker/deploy/tensorzero/docker-compose.yml` | Loker | Path mount `../../tensorzero/config` ties it to Loker layout |
| Gateway config | `loker/tensorzero/config/tensorzero.toml` | Loker | Functions all named `loker_*`; no other project can add functions here without crossing repo boundaries |
| `.env` template | `loker/deploy/tensorzero/.env.example` | Loker | Single-tenant — no auth token, only provider keys |
| Integration test | `loker/tests/tensorzero_integration` (gated by `LOKER_TZ_INTEGRATION=1`) | Loker | Will need path updates if config moves |
| README | `loker/deploy/tensorzero/README.md` | Loker | Documents dev-only Tier-2; no operator runbook |
| Lok references | `lok/docs/design-docs/clo-212-role-routing-config.md` (prior-art note only) | Lok | Lok does not yet call TZ |

**What TensorZero already gives us (kept as-is):**
- OpenAI-compatible HTTP API at `:3000`
- ClickHouse-backed observability (every inference persisted, queryable via SQL)
- Read-only UI at `:4000` with traces, episodes, variant comparison
- Function/variant abstraction in TOML — swap models without code changes
- Built-in fallback routing (`routing = ["primary", "fallback"]`)

---

## Recommended Implementation Order

Order by unblocking value, not dependency graph. Each phase is independently shippable and reversible.

| # | Phase | Why this order | Reversible? |
|---|---|---|---|
| 1 | **Centralize config + deploy** to `orchestrator/tensorzero/` | Unblocks every later phase. Pure file move plus path updates. Zero behavior change. | Yes — `git mv` back |
| 2 | **Naming convention + Loker rename** | Lock down `<project>_<purpose>_<family>` before adding more projects. Cheap to do while we are touching the file anyway. | Yes — functions are config |
| 3 | **Production hardening** (auth, TLS, backups, healthchecks) | Required before any non-Loker project depends on it. | Yes — feature flags + revert compose |
| 4 | **Fallback policy in client code** | Each consumer project must answer: hard-dep or best-effort? Codify per-project. | Yes — code change in each repo |
| 5 | **Onboard second project** (recommended: agtx or ralph-tui — confirm at execution time which actually makes LLM calls) | Proves the multi-tenant abstraction works end-to-end. | Yes — remove functions, revert client code |
| 6 | **Operator runbook + observability cheatsheet** | Locks in operational knowledge so this is not a one-person dependency. | N/A — docs only |

Phases 3 and 4 can run in parallel after Phase 2 lands.

---

## Phase 1: Centralize config and deployment

**Scope**: File moves plus path updates. No behavior changes. ~1 PR per repo.

### Files

**Loker repo** — moved out:
- `loker/deploy/tensorzero/docker-compose.yml` → `orchestrator/tensorzero/deploy/docker-compose.yml`
- `loker/deploy/tensorzero/README.md` → `orchestrator/tensorzero/deploy/README.md`
- `loker/deploy/tensorzero/.env.example` → `orchestrator/tensorzero/deploy/.env.example`
- `loker/tensorzero/config/tensorzero.toml` → `orchestrator/tensorzero/config/tensorzero.toml`

**Loker repo** — updated:
- `loker/tests/tensorzero_integration.rs` — any hardcoded path to config or compose file gets a relative path resolver or env var `TENSORZERO_CONFIG_DIR`
- `loker/README.md` — add a section "Inference layer: TensorZero is now in `orchestrator/tensorzero/`" with a link
- `loker/deploy/tensorzero/` — keep an empty `README.md` stub for 1 release with a redirect note, then delete

**New** (`orchestrator/tensorzero/` shared parent — sibling to `lok`, `loker`, `agtx`, etc.):
```
orchestrator/tensorzero/
├── README.md                 # Operator + onboarding entry point (Phase 6 fills this in)
├── CONVENTIONS.md            # Function-naming rules (Phase 2)
├── config/
│   └── tensorzero.toml       # All projects' functions
├── deploy/
│   ├── docker-compose.yml
│   ├── .env.example
│   └── README.md             # Local-dev quickstart (migrated)
└── ops/                      # Phase 3 lands runbook + backup scripts here
```

### docker-compose.yml changes

Two changes, both small.

**1. Mount path** — from `../../tensorzero/config` to `../config`:

```yaml
  gateway:
    image: tensorzero/gateway:2026.4.1
    volumes:
      - ../config:/app/config:ro
    command: --config-file /app/config/tensorzero.toml
```

**2. Loopback-only port bindings** — Docker Compose's `"3000:3000"` binds on `0.0.0.0` by default, which means the gateway and UI are reachable from any host on the LAN the moment the container is up. Until TLS and auth land (Phase 3), bind explicitly to loopback:

```yaml
  gateway:
    ports:
      - "127.0.0.1:3000:3000"
  ui:
    ports:
      - "127.0.0.1:4000:4000"
  clickhouse:
    ports:
      - "127.0.0.1:8123:8123"
```

Phase 3 lifts these back to `0.0.0.0` only behind a TLS-terminating reverse proxy (Caddy or similar) with auth in front.

Everything else (env vars, healthchecks) stays identical.

### Steps

This is a **copy-into-new-repo + delete-from-Loker** cutover, not an in-place `git mv`. `git mv` only preserves history within a single repo; copying across repo boundaries (Loker → new `tensorzero-infra` repo) means the file appears as a fresh add on the destination side. To preserve some history, use `git filter-repo` or `git format-patch` + `git am` on the destination if it matters; otherwise just `cp` and accept the clean start. Recommend the latter — the file is short, history is in `git log loker/deploy/tensorzero/` for archaeology.

1. Create new `orchestrator/tensorzero/` git repo (Phase 1.5 below), push it empty.
2. Copy the four files from Loker into the new repo at the layout shown above.
3. In the new compose: apply mount-path change *and* the loopback port bindings shown above.
4. Bring the stack down in Loker (`docker compose down` — keep volume).
5. Bring the stack up from the new location (`docker compose up -d`). See "Volume migration" in Cross-Cutting Concerns for the volume-name caveat — by default, Docker Compose prefixes volumes with the compose project name (derived from the directory), so `loker_clickhouse-data` becomes `tensorzero_clickhouse-data`. Either accept history loss or follow the explicit-rename procedure in Cross-Cutting.
6. Run `curl http://127.0.0.1:3000/health` and the gated `LOKER_TZ_INTEGRATION=1 cargo test`. Both must pass.
7. In Loker repo: update integration test to resolve config via env var (`TENSORZERO_CONFIG_DIR`) with a sensible default; update Loker README to point at the new repo; remove the now-orphaned `loker/deploy/tensorzero/` and `loker/tensorzero/` directories.
8. Open the Loker PR. Wait for green CI. Merge.
9. **Cutover window**: between step 5 (stack up in new location) and step 8 (Loker PR merged), the Loker integration test will fail if anyone reruns it from the old location. Coordinate timing or feature-gate the test until the PR lands.

### Decision: where does `orchestrator/tensorzero/` live as a repo?

Three options. Recommend (b).

| Option | Pros | Cons |
|---|---|---|
| (a) New dedicated repo `orchestrator/tensorzero-infra` | Clean separation, easy to grant access to ops people | One more repo to maintain; coordinating cross-repo PRs |
| **(b) Sibling untracked directory under `orchestrator/` parent, with its own git repo** | Lightest weight; matches existing layout where each project is its own repo at the same level | Discoverability — engineers may not know it exists. Mitigated by README links from each project. |
| (c) Monorepo-ize: pull tensorzero into one of the existing repos (e.g. `lok` since it is the operator) | Zero new repos | Couples it back to a project; same problem we are solving |

Pick (b). Initialize `orchestrator/tensorzero/` as its own git repo with `git init`, push to a new GitHub repo (e.g. `maxkulish/tensorzero-infra`). Reference it from every consuming project's README.

### Verification

- `docker compose up -d` from `orchestrator/tensorzero/deploy/` brings the stack up healthy
- `curl http://127.0.0.1:3000/health` returns 200 (note: `/health` and `/status` are unauthenticated by design — see TensorZero docs; auth on `/inference` is exercised in Phase 3a)
- `curl http://<lan-ip>:3000/health` from another machine on the LAN **must fail** (connection refused) — confirms loopback binding
- UI loads at `http://127.0.0.1:4000` and shows the existing Loker functions
- `cd loker && LOKER_TZ_INTEGRATION=1 cargo test --test tensorzero_integration` passes
- ClickHouse volume retains pre-move data **only if** the explicit-rename procedure in Cross-Cutting / Volume Migration is followed; otherwise expect a fresh DB

### Backward compatibility

- The old Loker compose file is removed; users must `cd orchestrator/tensorzero/deploy && docker compose up -d` instead of `cd loker/deploy/tensorzero`. Document this prominently in the Loker README and in commit message.
- Gateway port (`3000`) and UI port (`4000`) stay the same. No consumer code changes.

---

## Phase 2: Naming convention, tags, and model ownership

**Scope**: Document conventions, decide model-block ownership, and add a semantic CI check (not just YAML lint). No code changes; existing Loker functions stay valid.

### Don't overload `function_name` — use tags for metadata

The original draft tried to encode project, purpose, and model family into `function_name` alone. That breaks down: it forbids multi-word purposes, has no slot for environment or workflow or owner, and forces a rename if any of those change. Use TensorZero's tags (key-value map attachable per inference) as the metadata surface and keep `function_name` small.

**Function name** — short identifier of the logical call site:
```
<project>_<purpose>_<family>
```
- `<project>`: project key (lowercase, alphanumeric). Examples: `loker`, `lok`, `agtx`.
- `<purpose>`: short identifier (lowercase, alphanumeric, no underscores within). For multi-word purposes, compress (`prepr` for "pre-pr-validation") or pick a shorter handle.
- `<family>`: `anthropic` | `openai` | `google` | `zhipu`. Needed because Loker's `family_of(backend_id)` derives routing from this suffix; until that derivation moves to a tag, family stays in the name.

**Required tags on every inference** (validated at the client wrapper, not at the gateway — Phase 4 covers the client crate):

| Tag | Values | Purpose |
|---|---|---|
| `project` | `loker`, `lok`, `agtx`, … (one of `KNOWN_PROJECTS`) | Cost attribution, access filtering |
| `env` | `dev`, `staging`, `prod` | Filter dev noise out of prod analytics |
| `workflow` | free-form, scoped within project | Trace requests across a multi-step flow |
| `owner` | individual or team handle | On-call routing if something looks wrong |
| `fallback_policy` | `hard` | `best-effort` | Phase 4 — also recorded so ClickHouse queries can correlate fallbacks |

ClickHouse queries then filter on tags rather than parsing `function_name`. Phase 6 cheatsheet rewritten to use tag-based queries.

### Model-block ownership: shared, not project-prefixed

The original draft contradicted itself — Phase 2 said model blocks should be project-prefixed, Phase 5 reused `loker_openai_mini` from agtx. Pick the cleaner option:

**Model blocks describe upstream models. They are shared, not project-owned.** Naming:
```
<provider>_<model-shortname>
```
Examples: `openai_gpt5mini`, `anthropic_haiku45`, `google_gemini31flashlite`.

Functions reference shared model blocks. This means renaming Loker's existing model blocks is **part of Phase 2** (small, additive change — keep the old names as aliases for one release if you want zero risk):

| Old | New |
|---|---|
| `loker_anthropic_haiku` | `anthropic_haiku45` |
| `loker_openai_mini` | `openai_gpt5mini` |
| `loker_google_flashlite` | `google_gemini31flashlite` |

Variants still belong to functions (each function picks its own variant config — prompt template, temperature, etc.), so variant names like `haiku_v1`, `mini_v1` stay function-scoped.

### CONVENTIONS.md content

1. **Function name** rule + regex `^[a-z][a-z0-9]*_[a-z0-9]+_(anthropic|openai|google|zhipu)$`.
2. **Model block** rule + regex `^(anthropic|openai|google|zhipu)_[a-z0-9]+$`.
3. **Required tags** table (above) — clients without these tags get rejected by the client wrapper (Phase 4).
4. **`KNOWN_PROJECTS`** list — appended to via one-line PR. Catches typos.
5. **Variant naming**: `<model-shortname>_v<N>` (function-scoped).

### CI: semantic smoke check, not just YAML

`docker compose config` only checks YAML syntax. A bad TOML — missing model reference, unknown family suffix, duplicate function name — passes that check and breaks the gateway at startup. Because the config is now shared across projects, a bad function added for agtx can block Loker.

CI job in `tensorzero-infra` repo for every PR:

```yaml
- name: Start gateway against pinned image
  run: |
    docker run --rm -d --name tz-smoke \
      -v $PWD/config:/app/config:ro \
      -p 127.0.0.1:3001:3000 \
      -e OPENAI_API_KEY=dummy \
      -e ANTHROPIC_API_KEY=dummy \
      -e GOOGLE_AI_STUDIO_API_KEY=dummy \
      tensorzero/gateway:2026.4.1 \
      --config-file /app/config/tensorzero.toml
    sleep 5
    curl --fail http://127.0.0.1:3001/health

- name: Validate naming
  run: |
    python3 ci/validate_names.py

- name: Tear down
  if: always()
  run: docker rm -f tz-smoke || true
```

If gateway startup fails (bad TOML, missing model reference, schema mismatch with the pinned version), `curl /health` 404s or times out and CI fails. This catches semantic breakage before merge.

Dry-run inference per changed function is a stretch goal — the dummy provider keys mean a real inference fails. Either ship a mock provider in TZ's test mode (if available) or accept the startup-only smoke check.

### Backward compatibility

- Function names: zero migration (existing names already comply).
- Model blocks: renamed in this phase. Loker functions referencing them update in the same PR. Verified by the smoke check.
- Tags: enforced at the client layer in Phase 4, not retroactively on existing data.

---

## Phase 3: Production hardening

**Scope**: TLS, gateway auth, ClickHouse backups, healthcheck alerting, resource limits. Each is an independent subtask; ship in any order.

### 3a. Gateway auth (smallest, ship first)

**Why**: today `localhost:3000` is wide open. If we expose it to other machines on the LAN — even just for a second project on the same host — anyone on the network can spend the API keys.

**Approach**: TensorZero supports a bearer token. Set `TENSORZERO_API_KEY` in `.env`, send it as `Authorization: Bearer <token>` from clients. The existing `.env.example` already hints at this via the integration test's `TENSORZERO_API_KEY=any non-empty token` comment.

**Compose change**:

```yaml
gateway:
  environment:
    TENSORZERO_API_KEY: ${TENSORZERO_API_KEY:?Environment variable TENSORZERO_API_KEY must be set.}
```

**Client update**: Loker's HTTP client must include the header. Look for the existing `TENSORZERO_API_KEY` references in `loker/src/` — the integration test already reads it.

**Decision**: single shared token in Phase 3a. Per-project tokens are deferred (see Non-Goals).

### 3b. TLS termination

**Why**: needed before exposing beyond `127.0.0.1`. Until clients are remote, this is optional.

**Approach**: add a Caddy sidecar to the compose file with automatic Let's Encrypt. If running on a machine without a public DNS name, use self-signed and document the trust-store steps.

```yaml
caddy:
  image: caddy:2
  ports:
    - "443:443"
  volumes:
    - ./Caddyfile:/etc/caddy/Caddyfile:ro
    - caddy-data:/data
  depends_on:
    gateway:
      condition: service_healthy
```

Caddyfile (one block):

```
tensorzero.internal {
    reverse_proxy gateway:3000
}
```

Skip this subtask until we actually need remote access.

### 3c. ClickHouse backups

**Why**: ClickHouse holds every inference trace. Losing the `clickhouse-data` volume = losing observability history.

**Approach**: scheduled `clickhouse-backup` container, write to local disk or S3-compatible bucket. One cron entry in the host's crontab triggers `docker exec` against a small backup container.

**Files**:
- `orchestrator/tensorzero/ops/backup.sh` — calls `docker compose exec clickhouse clickhouse-backup create` then `upload` if a remote target is configured
- `orchestrator/tensorzero/ops/README.md` — documents the cron entry and restore procedure

**Retention**: keep 7 daily snapshots locally, 30 daily on remote. Negotiable.

### 3d. Healthcheck + alerting

**Why**: silent gateway failures mean projects are silently making un-logged direct provider calls (assuming we ship fallback in Phase 4) or breaking outright.

**Minimum viable**: a `curl http://localhost:3000/health` ping every 60s from a single uptime probe. Options:
- Use the user's existing monitoring (if any). Check before adding a new tool.
- Healthchecks.io free tier — one HTTP endpoint, email + Slack on failure.

Defer Prometheus/Grafana unless we already run them for something else.

### 3e. Resource limits

Add to compose:

```yaml
gateway:
  deploy:
    resources:
      limits:
        memory: 1G
        cpus: '1.0'

clickhouse:
  deploy:
    resources:
      limits:
        memory: 4G
        cpus: '2.0'
```

Prevents a runaway query from taking down the host.

### Verification (per subtask)

- 3a: `curl http://localhost:3000/health` returns 401 without bearer; 200 with. Loker integration test still passes after client update.
- 3b: `curl https://tensorzero.internal/health` returns 200 with valid cert.
- 3c: `./ops/backup.sh` produces a non-empty backup file; restore into a fresh ClickHouse container yields identical row counts.
- 3d: Manually stop the gateway; alerting channel fires within 2× the probe interval.
- 3e: `docker stats` shows the configured limits.

---

## Phase 4: Fallback policy per consuming project

**Scope**: Each project that adopts TensorZero must answer: "what happens when the gateway is down?". Codify the answer in code, not in a runbook.

### The two policies

| Policy | Behavior on gateway 5xx / timeout | When to pick |
|---|---|---|
| **Hard-dep** | Surface error, fail the operation | Project explicitly wants every call observed (e.g. for audit/compliance) |
| **Best-effort** | Log warning, fall back to direct provider SDK call, lose observability for that call | Default — most projects care more about availability than 100% observability |

### Implementation pattern (Rust, applicable to Loker / Lok)

```rust
async fn call_llm(req: InferenceRequest) -> Result<Response> {
    match tensorzero_client.infer(&req).await {
        Ok(r) => Ok(r),
        Err(e) if cfg.fallback_policy == FallbackPolicy::BestEffort && e.is_gateway_failure() => {
            tracing::warn!(?e, "TensorZero unavailable; falling back to direct provider");
            direct_provider_client(&req.family).infer(&req).await
        }
        Err(e) => Err(e),
    }
}
```

`e.is_gateway_failure()` covers: connection refused, 5xx, timeout. It does **not** cover 4xx (auth, malformed request) — those are caller bugs and should fail loudly regardless of policy.

### Loker integration

Loker is the first consumer. Codify its policy:
- Production runs: **best-effort** (do not block decision pipelines on observability infra)
- Integration test environment (`LOKER_TZ_INTEGRATION=1`): **hard-dep** (we want to know if the test path is broken)

Add a config field `tensorzero.fallback_policy: "hard" | "best-effort"` in Loker's existing config layer. Default `"best-effort"`.

### Tests

Per project:
- `test_fallback_on_gateway_502`: gateway returns 502 → with `best-effort`, direct call succeeds; with `hard`, error propagates
- `test_no_fallback_on_400`: gateway returns 400 → error propagates regardless of policy

Use a mock HTTP server (e.g. `wiremock` crate for Rust) instead of the real gateway.

---

## Phase 5: Onboard a second project

**Scope**: prove the abstraction. Pick the simplest LLM-calling project that is not Loker and add `<project>_*` functions plus client wiring.

### Selection criteria

- Already makes provider calls today (so the migration is "replace direct call with TZ call", not "add new feature")
- Maintained by the same operator (you) — avoids social coordination
- Small surface area — one or two call sites

Inspect `orchestrator/agtx/`, `orchestrator/ralph-tui/`, `orchestrator/gastown/` at execution time to confirm which qualify. If none, the Loker integration test alone validates the path and Phase 5 can be deferred until a real second consumer appears.

### Steps (assuming target = `agtx`)

1. Add to `orchestrator/tensorzero/config/tensorzero.toml`:
   ```toml
   [functions.agtx_<purpose>_openai]
   type = "chat"

   [functions.agtx_<purpose>_openai.variants.mini_v1]
   type = "chat_completion"
   model = "loker_openai_mini"  # reuse Loker's underlying model block — or add agtx_*
   ```
2. Restart gateway (`docker compose up -d` from `orchestrator/tensorzero/deploy/`)
3. In `agtx/`, replace direct OpenAI SDK call with HTTP call to `http://localhost:3000/inference` carrying function name and bearer token
4. Implement fallback per Phase 4
5. Run agtx's test suite. Verify ClickHouse shows traffic with `function_name LIKE 'agtx_%'`:
   ```sql
   SELECT function_name, COUNT(*)
   FROM tensorzero.ChatInference
   WHERE function_name LIKE 'agtx_%'
   GROUP BY function_name
   ```

### Verification

- ClickHouse query above returns non-zero count for agtx functions
- Loker's traffic is still visible and not affected (`function_name LIKE 'loker_%'` count unchanged from baseline)
- UI at `http://localhost:4000` shows both projects' inferences in the trace list

---

## Phase 6: Operator runbook and observability cheatsheet

**Scope**: documentation only. `orchestrator/tensorzero/ops/README.md` plus `orchestrator/tensorzero/CHEATSHEET.md`.

### Operator runbook sections

1. **Start / stop / restart**
   ```sh
   cd orchestrator/tensorzero/deploy
   docker compose up -d        # start
   docker compose restart gateway   # restart just gateway
   docker compose down         # stop (volumes preserved)
   docker compose down -v      # stop + delete ClickHouse data (destructive)
   ```
2. **Adding a new project** — link to CONVENTIONS.md plus a 3-step recipe: add function block, restart gateway, add client code with fallback
3. **Rotating provider API keys** — edit `.env`, `docker compose up -d` (re-reads env). No downtime for in-flight requests
4. **Restoring from backup** — exact `clickhouse-backup restore` command sequence
5. **Upgrading TensorZero version** — pin to `tensorzero/gateway:X.Y.Z` in compose; bump version; restart; verify health; rollback procedure if config schema breaks

### Observability cheatsheet — top 5 queries

```sql
-- Top functions by call volume (last 24h)
SELECT function_name, COUNT(*) AS calls
FROM tensorzero.ChatInference
WHERE timestamp > now() - INTERVAL 1 DAY
GROUP BY function_name ORDER BY calls DESC;

-- p50/p95/p99 latency by function (last 7d)
SELECT function_name,
       quantile(0.5)(processing_time_ms) AS p50,
       quantile(0.95)(processing_time_ms) AS p95,
       quantile(0.99)(processing_time_ms) AS p99
FROM tensorzero.ChatInference
WHERE timestamp > now() - INTERVAL 7 DAY
GROUP BY function_name;

-- Token spend by project (extracts project from function_name prefix)
SELECT splitByChar('_', function_name)[1] AS project,
       SUM(input_tokens + output_tokens) AS total_tokens
FROM tensorzero.ChatInference
WHERE timestamp > now() - INTERVAL 1 DAY
GROUP BY project;

-- Failures by provider model
SELECT model_name, COUNT(*) AS fails
FROM tensorzero.ModelInferenceCache  -- or ModelInference depending on schema version
WHERE timestamp > now() - INTERVAL 1 DAY AND error IS NOT NULL
GROUP BY model_name;

-- Episode tracing — all inferences for one conversation
SELECT inference_id, function_name, processing_time_ms
FROM tensorzero.ChatInference
WHERE episode_id = '<uuid>'
ORDER BY timestamp;
```

Verify the table/column names against the actual ClickHouse schema at execution time — the TensorZero schema evolves with versions. Run `SHOW TABLES IN tensorzero` and `DESCRIBE tensorzero.ChatInference` first.

---

## Cross-Cutting Concerns

### Repo coordination

This plan spans:
- `loker` (file moves, README updates, client integration tests)
- new `orchestrator/tensorzero/` (the destination)
- `lok` (this plan document only; Lok itself does not yet call TZ)
- A future second consumer in Phase 5

Open one PR per repo. Sequence: open `orchestrator/tensorzero` repo first (it must exist before Loker can move into it), then Loker PR, then per-project PRs in Phase 5.

### Secrets handling

`.env` files contain real provider keys and (after Phase 3a) the TENSORZERO_API_KEY. Do not commit them. The `.env.example` template is committed; the real `.env` is gitignored. Verify `.gitignore` in the new repo lists `.env` before first commit.

### Existing Loker functions stay valid

`loker_d1_anthropic`, `loker_d1_openai`, `loker_d1_google` already match the naming convention. Phase 2 does not require renaming them. ClickHouse history continues uninterrupted.

### Volume migration

The Docker volume `clickhouse-data` is named at the compose level. Moving the compose file to a new directory changes the volume's *project name* (Compose prefixes volumes with the directory name). Two options:

- (a) Accept the rename: start fresh ClickHouse in the new location, lose history. Acceptable if we treat current history as throwaway dev data.
- (b) Preserve history: rename the volume explicitly with `name:` in the new compose file:
  ```yaml
  volumes:
    clickhouse-data:
      name: tensorzero_clickhouse-data
      external: false
  ```
  Then `docker volume create tensorzero_clickhouse-data` followed by `docker run --rm -v <old>:/from -v <new>:/to alpine sh -c "cd /from && cp -a . /to"` to copy data.

Pick (a) unless current ClickHouse data has meaningful value. Document the choice in the Loker PR description.

### CI

The new `orchestrator/tensorzero` repo needs minimal CI: the naming-convention check from Phase 2, and a `docker compose config` lint step that catches syntax errors in the compose file. No tests beyond that — the gateway itself is upstream.

---

## Out of Scope

- Per-project API keys with quota enforcement. Defer until we have ≥3 projects and one of them is untrusted.
- Migrating Loker to call TensorZero in production today. CLO-243 spike was a round-trip POC; production cutover is a separate decision tracked in Loker, not in this plan.
- Replacing the in-Loker `llm-mux` layer with TensorZero. Different abstractions; coexistence is fine.
- Multi-region deployment. One host is plenty.
- Anything involving the TensorZero "training" / preference-learning features. We only use it as a gateway plus observability.

---

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| ClickHouse schema breaks on TZ upgrade | Medium | High (observability blackout) | Pin gateway+UI to the same version tag; test upgrades in dev first; backup before upgrade |
| Single shared gateway token leaks | Low | High (provider key abuse) | Rotate quarterly; phase 3a documents the rotation procedure; future per-project tokens |
| Moving the volume loses history | Low if (b) is picked | Low (dev data) | Document the choice in PR; default to (a) accepting loss |
| New project adoption stalls because of the bearer-token plumbing | Medium | Low | Provide a tiny `tensorzero-client` helper crate (Rust) for orchestrator projects — one-call interface that handles auth + fallback. Optional, build only if Phase 5 reveals friction |
| `orchestrator/tensorzero` becomes unowned | Medium | Medium | Single-operator setup is fine for now; revisit if the team grows |

---

## References

- TensorZero docs: https://www.tensorzero.com/docs (verify version compatibility with 2026.4.1 at execution time)
- Loker round-trip spike: `loker/docs/spikes/2026-04-25-tensorzero-roundtrip.md`
- Loker family-naming verdict (CLO-247): comment block at `loker/tensorzero/config/tensorzero.toml:36-43`
- `family_of(backend_id)` derivation (CLO-251 / FR-13): in Loker source
- Existing deploy README: `loker/deploy/tensorzero/README.md`
- Lok prior-art reference: `lok/docs/design-docs/clo-212-role-routing-config.md`
