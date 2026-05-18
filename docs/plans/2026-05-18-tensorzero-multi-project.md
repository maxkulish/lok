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
- Full multi-tenant RBAC (per-key scopes, role hierarchies, audit log of permission changes). Phase 3a.i installs Postgres + TensorZero's built-in auth, which makes per-project API keys cheap to add later — but RBAC beyond "this key works / does not work" is out of scope for this plan.
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
| 1 | **Centralize config + deploy** to `orchestrator/tensorzero/` (loopback-bound) | Unblocks every later phase. Cross-repo copy + delete with explicit cutover window. Zero behavior change. | Yes — copy back, restore old compose |
| 2 | **Naming convention + tags + shared model blocks + CI semantic check** | Lock down naming and tag schema before adding more projects. Rename Loker's model blocks to shared names while we are touching the file. | Yes — functions are config |
| 3 | **Production hardening** — 7 subtasks (3a auth gateway+UI, 3b TLS, 3c backups + restore drill, 3d alerting, 3e resource limits, 3f data privacy + retention, 3g cost control) | 3a must ship before exposing beyond loopback. 3f must ship before any non-operator gets UI access. Each runbook piece ships next to its mechanism, not at the end. | Yes per subtask — feature flags + revert compose |
| 4 | **Fallback policy + fallback observability in client code** | Each consumer project must answer: hard-dep or best-effort? Fallbacks MUST be counted and alerted on — silent fallback defeats the entire observability layer. | Yes — code change in each repo |
| 5 | **Onboard second project** (recommended: agtx or ralph-tui — confirm at execution time which actually makes LLM calls) | Proves the multi-tenant abstraction works end-to-end. | Yes — remove functions, revert client code |
| 6 | **Consolidated docs + observability cheatsheet** | Assembles the runbook pieces written in Phase 3 into a single operator entry-point. Phase 6 no longer carries first-time runbook content. | N/A — docs only |

Phases 3 and 4 can run in parallel after Phase 2 lands, with the caveat that Phase 4's fallback observability tests want 3d's alerting destination wired up — implement 3d before merging Phase 4 if possible.

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

**Scope**: gateway auth (gateway + UI are two separate problems), TLS, ClickHouse backups, healthcheck alerting, resource limits, data privacy, cost control. Each is an independent subtask; ship in any order **except 3a must ship before exposing the stack beyond loopback** (see Phase 1 verification).

### 3a. Gateway and UI auth — the two-layer problem

**Why**: today `127.0.0.1:3000` (gateway) and `127.0.0.1:4000` (UI) are wide open to anything on the host. Loopback binding (Phase 1) buys us local-only protection. Once we expose either port to LAN or beyond — even for a second project on a different host — anyone reachable can either spend the API keys (gateway) or read every prompt and response (UI).

**Critical correction from earlier draft**: TensorZero auth is **not** a simple `TENSORZERO_API_KEY` env var. Per the [Setting up auth for TensorZero](https://www.tensorzero.com/docs/operations/set-up-auth-for-tensorzero) docs:

1. Auth is enabled in `tensorzero.toml`, not via env var
2. The gateway requires **Postgres** when auth is enabled (it stores users, keys, scopes there)
3. `/health` and `/status` are **always unauthenticated by design** — they exist for orchestrators (k8s liveness, Docker healthchecks) to probe without credentials
4. The UI has **no built-in auth** — it must be fronted by a reverse proxy (Tailscale, OAuth2-Proxy, Nginx + basic auth, Caddy with `basic_auth`) or it stays loopback-only forever
5. Auth must be verified against `/inference` or `/openai/v1/chat/completions`, not `/health`

#### 3a.i — Gateway auth via tensorzero.toml + Postgres

**Compose change** — add a Postgres service and wire it to the gateway:

```yaml
postgres:
  image: postgres:16
  environment:
    POSTGRES_USER: tensorzero
    POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:?POSTGRES_PASSWORD must be set}
    POSTGRES_DB: tensorzero
  volumes:
    - postgres-data:/var/lib/postgresql/data
  healthcheck:
    test: ["CMD-SHELL", "pg_isready -U tensorzero"]
    interval: 5s
    timeout: 3s
    retries: 5

gateway:
  environment:
    TENSORZERO_POSTGRES_URL: postgres://tensorzero:${POSTGRES_PASSWORD}@postgres:5432/tensorzero
  depends_on:
    postgres:
      condition: service_healthy
    clickhouse:
      condition: service_healthy
```

**Config change** — add to `tensorzero.toml`:

```toml
[gateway]
bind_address = "0.0.0.0:3000"
observability.enabled = true

[gateway.auth]
enabled = true
```

After first startup, create at least one API key via the documented admin flow (gateway exposes admin endpoints once Postgres is wired; exact endpoint shape depends on the pinned version — verify against `tensorzero/gateway:2026.4.1` docs at execution time).

**Client update**: clients send `Authorization: Bearer <key>`. Loker's HTTP client must include the header.

**Decision**: single shared key for Phase 3a.i. Per-project keys (one row per project in Postgres) are cheap to add later because the storage already exists — promote to per-project keys the first time we have two projects on the same gateway.

#### 3a.ii — UI auth (separate problem)

**Why**: the UI is read-only but exposes every prompt and response. With auth disabled on the UI port, anyone with network reach to `:4000` can read all customer prompts, internal tool prompts, etc. This is a privacy exposure even on a single-tenant dev box if the dev box is on a shared network.

**Options, ranked**:

| # | Approach | Cost | When to pick |
|---|---|---|---|
| 1 | **Stay loopback-only forever** (`127.0.0.1:4000`) plus SSH tunnel for remote inspection (`ssh -L 4000:127.0.0.1:4000 host`) | Zero | Default. Until a non-operator needs UI access |
| 2 | Front UI with **Tailscale** (`tailscale serve` exposes container to tailnet only) | Low (Tailscale already in use? assume not — verify) | Multi-operator, all on the same tailnet |
| 3 | **OAuth2-Proxy** sidecar against Google / GitHub | Medium | Need named-user auth + audit log |
| 4 | **Nginx + basic auth** sidecar | Low but coarse | Quick gate, single shared password, no audit trail |

Recommendation: **(1)** until there is a second consumer of the UI. Document the SSH-tunnel one-liner in the runbook.

**Compose change for option (1)**: confirm the loopback binding from Phase 1 stays in place. No additional change.

**Compose change for option (4)** (when needed):

```yaml
nginx-ui:
  image: nginx:1
  ports:
    - "4001:443"
  volumes:
    - ./nginx/ui.conf:/etc/nginx/conf.d/default.conf:ro
    - ./nginx/htpasswd:/etc/nginx/htpasswd:ro
    - ./nginx/certs:/etc/nginx/certs:ro
  depends_on:
    ui:
      condition: service_started
```

Then the UI container drops its host-side port binding entirely and is reached only via the nginx sidecar.

#### Verification — 3a

- Gateway: `curl http://127.0.0.1:3000/health` returns **200 without bearer** (unauthenticated by design).
- Gateway: `curl http://127.0.0.1:3000/inference -X POST -d '{...}'` returns **401 without bearer**; **200 with valid bearer**.
- Gateway: Loker integration test passes after the client adds the bearer header.
- UI: under option (1), `curl http://127.0.0.1:4000` from another machine on the LAN returns **connection refused**.
- Postgres: `docker compose exec postgres psql -U tensorzero -c '\dt'` lists the tables TensorZero created on startup.

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

### 3c. ClickHouse backups + restore drill

**Why**: ClickHouse holds every inference trace. Losing the `clickhouse-data` volume = losing observability history. A backup is only useful if you have rehearsed restoring it.

**Approach**: scheduled `clickhouse-backup` container, write to local disk or S3-compatible bucket. One cron entry on the host triggers `docker exec` against a small backup container.

**Files** (this is the **runbook-forward** placement that the earlier draft put in Phase 6 — operators need this on day one, not after onboarding):
- `orchestrator/tensorzero/ops/backup.sh` — calls `docker compose exec clickhouse clickhouse-backup create` then `upload` if a remote target is configured
- `orchestrator/tensorzero/ops/restore.sh` — wraps `clickhouse-backup restore <name>`
- `orchestrator/tensorzero/ops/README.md` — documents cron entry, restore procedure, and **quarterly restore-drill checklist**

**Retention**: 7 daily local snapshots, 30 daily on remote.

**Restore drill** (quarterly): stand up a scratch ClickHouse container, restore the most recent backup into it, run `SELECT COUNT(*) FROM ChatInference WHERE timestamp > now() - INTERVAL 1 DAY`, confirm non-zero. Tear down scratch container. If the drill fails, ship is on fire — fix before the next backup window.

**Encryption at rest** (cross-reference 3f): backups must be encrypted before leaving the host. Either rely on the S3 bucket's SSE-S3/KMS settings, or encrypt locally with `age` before upload. Local-disk-only backups inherit the disk's encryption (assume FileVault / LUKS on the host).

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

postgres:
  deploy:
    resources:
      limits:
        memory: 512M
        cpus: '0.5'
```

Prevents a runaway query from taking down the host.

### 3f. Data privacy and retention

**Why**: ClickHouse stores the **full input and output** of every inference (per the [TensorZero data-model docs](https://www.tensorzero.com/docs/gateway/data-model) — `ChatInference.input`, `ChatInference.output`, plus `ModelInference.raw_request` and `raw_response` with the unredacted upstream provider payloads). The UI surfaces all of this verbatim. Two consequences: (a) anyone with UI access can read every prompt across every project; (b) old data accumulates indefinitely unless we set a retention policy.

This is the highest-impact subtask that the earlier draft missed entirely.

#### What is captured

| Table | Sensitive content | Notes |
|---|---|---|
| `ChatInference` | `input` (full conversation), `output` (model response), `tags` | One row per gateway call |
| `ModelInference` | `raw_request`, `raw_response` — the literal HTTP payloads to/from the upstream provider | One row per upstream call (≥1 per inference if fallback triggered) |
| `JsonInference` | Same as `ChatInference` but for JSON-mode functions | — |

If a project sends PII, customer data, secrets, or proprietary prompts, **all of it lives in ClickHouse** until we explicitly delete it.

#### Subtasks

1. **Data-classification rule** — document in `CONVENTIONS.md`. Examples of what is OK to send through TensorZero by default:
   - Operator-internal prompts (e.g. Loker's spec drafting)
   - Synthetic test data
   - Public documents
   What is **not** OK without an explicit privacy review:
   - Customer PII, PHI, payment card data
   - Internal secrets / credentials embedded in prompts
   - Documents under NDA with third parties
   Projects that need to send any of the second category must (a) get sign-off, (b) consider a dedicated TensorZero instance for that project rather than the shared one.

2. **Per-project redaction at the client layer** — the Phase 4 client crate exposes a `redact_fn: Option<fn(&str) -> Cow<str>>` hook applied before the request hits the wire. Default is `None`. Projects that need redaction supply one (e.g. credit-card regex, email masker). Document this in CONVENTIONS.md and link from each project's adoption guide.

3. **Retention policy** — TTL on ClickHouse tables. Sensible default:
   ```sql
   ALTER TABLE ChatInference MODIFY TTL timestamp + INTERVAL 90 DAY;
   ALTER TABLE ModelInference MODIFY TTL timestamp + INTERVAL 90 DAY;
   ALTER TABLE JsonInference MODIFY TTL timestamp + INTERVAL 90 DAY;
   ```
   Tune per project at the application layer if a project legally must keep traces longer (compliance) or shorter (privacy minimum). Document the TTL value in `ops/README.md`.

4. **UI access control** — links to 3a.ii. The UI is the primary leakage surface. If 3a.ii is deferred (UI stays loopback-only with SSH-tunnel access), **explicitly document that broadcasting UI access requires 3a.ii first**.

5. **Backup encryption** — covered in 3c. Cross-reference here.

6. **Deletion-on-request workflow** — if a project needs to satisfy a deletion request (GDPR Article 17, customer ask, "we mis-sent something"), document the SQL:
   ```sql
   ALTER TABLE ChatInference DELETE WHERE inference_id = '...'
                                    OR JSONExtractString(tags, 'request_id') = '...';
   ```
   ClickHouse `DELETE` is async — the row may persist briefly but the data is gone within the merge window. Document the SLA the operator commits to.

#### Verification — 3f

- `SHOW CREATE TABLE tensorzero.ChatInference` after Phase 3f shows a `TTL timestamp + INTERVAL 90 DAY` clause.
- CONVENTIONS.md has a "What data is safe to send" section.
- Backup files at rest on disk fail `file` inspection as plain JSON/SQL (i.e. they are encrypted or live on an encrypted volume).

### 3g. Cost control

**Why**: TensorZero exposes spending in ClickHouse but does not cap it. A bad prompt loop in any one project can spend the shared API keys faster than any human notices.

#### Subtasks

1. **Mandatory project tags** — already in Phase 2 (`project`, `env`, `owner`, `workflow`). The Phase 4 client crate rejects requests missing required tags. Without `project`, cost attribution is impossible.

2. **Provider-side budget caps** — set monthly hard caps on each provider's dashboard:
   - Anthropic console → org budget
   - OpenAI dashboard → usage limits
   - Google AI Studio → budget alerts (Google's API does not hard-cap; rely on alerting)

   Set the cap **below the level at which a runaway loop would be catastrophic**. Document the values in `ops/README.md`. These caps protect against the worst case (gateway compromise, runaway client) — they are the last line of defense, not the first.

3. **Spend-spike alerting** — ClickHouse query run hourly via cron, fires an alert if any project's spend over the last hour exceeds 3× its trailing-7-day hourly average:
   ```sql
   SELECT JSONExtractString(tags, 'project') AS project,
          SUM(input_tokens + output_tokens) AS recent_tokens
   FROM tensorzero.ChatInference
   WHERE timestamp > now() - INTERVAL 1 HOUR
   GROUP BY project
   HAVING recent_tokens > 3 * (
       SELECT AVG(hourly_tokens) FROM (
           SELECT toStartOfHour(timestamp) AS hr,
                  SUM(input_tokens + output_tokens) AS hourly_tokens
           FROM tensorzero.ChatInference
           WHERE timestamp BETWEEN now() - INTERVAL 7 DAY AND now() - INTERVAL 1 HOUR
             AND JSONExtractString(tags, 'project') = project
           GROUP BY hr
       )
   );
   ```
   Wire this to whatever alerting channel 3d picks. Tune the threshold once we have a real baseline.

4. **Per-project kill switch** — a TensorZero variant-level config flag, or a runtime gateway-config reload, that drops traffic for `project=<name>` while leaving others unaffected. Cheap implementation: add an `[disabled_projects]` list to a sidecar file the gateway reads on SIGHUP, and have the Phase 4 client crate check it locally before sending. Heavier implementation: use TensorZero's per-function disable feature once we are on a version that exposes it. Start with the sidecar approach; revisit if the gateway native feature is simpler.

#### Verification — 3g

- A test request without the `project` tag is rejected by the client crate (Phase 4 test).
- Manually flooding test traffic for one project triggers the spend-spike alert within 1× the cron interval.
- Adding a project to the kill switch causes new requests to fail-closed; existing in-flight requests complete.

### Verification (per subtask) — summary

- 3a: see 3a verification block above.
- 3b: `curl https://tensorzero.internal/health` returns 200 with valid cert.
- 3c: `./ops/backup.sh` produces a non-empty backup; `./ops/restore.sh` into a scratch ClickHouse yields identical row counts. Quarterly restore drill on the calendar.
- 3d: Manually stop the gateway; alerting channel fires within 2× the probe interval.
- 3e: `docker stats` shows the configured limits.
- 3f: see 3f verification block.
- 3g: see 3g verification block.

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

### Fallback observability — do not let fallbacks hide

A fallback that silently succeeds is *worse* than a hard failure: the gateway is broken, every call is bypassing observability, and nobody knows. Fallbacks must be counted and alerted on.

**At every fallback invocation, the client crate MUST**:

1. **Increment a counter** — local Prometheus counter, or simple atomic `AtomicU64` exposed at a `/metrics` endpoint per project, or (if the project has no metrics layer yet) a `tracing::warn!` log with structured fields:
   ```rust
   tracing::warn!(
       project = %project,
       family = %family,
       function = %function_name,
       gateway_status = %status,
       "tensorzero_fallback_invoked"
   );
   ```
2. **Tag the direct provider call** — even though it bypasses TensorZero, log it locally with `fallback_reason` so the operator can correlate after the fact.
3. **Surface in the runbook** — `ops/README.md` Phase 3 entry documents how to grep logs for `tensorzero_fallback_invoked`.

**Alerting threshold**: fallback rate >1% over a 5-minute window for any project triggers an alert. (Adjust once we have a baseline.) Implementation depends on what 3d picks — at minimum, a logwatch rule on the host that mails when more than N occurrences of `tensorzero_fallback_invoked` show up per minute.

The point: silent fallback is the *exact* failure mode this whole project is designed to prevent. Make it loud.

### Tests

Per project:
- `test_fallback_on_gateway_502`: gateway returns 502 → with `best-effort`, direct call succeeds **and the fallback counter increments by 1**; with `hard`, error propagates.
- `test_no_fallback_on_400`: gateway returns 400 → error propagates regardless of policy; counter does **not** increment (4xx is a caller bug, not a gateway failure).
- `test_fallback_logs_structured_event`: assert the `tensorzero_fallback_invoked` event appears in captured logs with the expected fields.

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

1. Add to `orchestrator/tensorzero/config/tensorzero.toml`. Note: per Phase 2, model blocks are **shared, not project-prefixed** — agtx references the same `openai_gpt5mini` model block that Loker uses.
   ```toml
   [functions.agtx_<purpose>_openai]
   type = "chat"

   [functions.agtx_<purpose>_openai.variants.mini_v1]
   type = "chat_completion"
   model = "openai_gpt5mini"  # shared model block from Phase 2 rename
   ```
2. Restart gateway (`docker compose up -d` from `orchestrator/tensorzero/deploy/`)
3. In `agtx/`, replace direct OpenAI SDK call with HTTP call to `http://localhost:3000/inference` carrying function name and bearer token
4. Implement fallback per Phase 4
5. Run agtx's test suite. Verify ClickHouse shows traffic for the new project — use the **`project` tag** (Phase 2), not the function name prefix:
   ```sql
   SELECT function_name, COUNT(*)
   FROM tensorzero.ChatInference
   WHERE JSONExtractString(tags, 'project') = 'agtx'
   GROUP BY function_name
   ```

### Verification

- ClickHouse query above returns non-zero count for agtx requests
- Loker's traffic is still visible and not affected (filter on `project = 'loker'`, count unchanged from baseline)
- UI at `http://127.0.0.1:4000` shows both projects' inferences in the trace list (filter by tag)
- **Fallback observability check** (per Phase 4): the agtx test run does not emit unexpected `tensorzero_fallback_invoked` events

---

## Phase 6: Consolidated docs and observability cheatsheet

**Scope**: documentation only. The earlier draft of Phase 6 bundled "runbook" content (backup/restore, key rotation, upgrade procedure) into the *last* phase — which meant operators would have nothing to lean on for the first several phases of production use. That content is **moved forward**:

- **Backup + restore + restore drill** → Phase 3c (ships with the backup container itself)
- **Provider key rotation** → Phase 3a.i (ships with the auth subtask, since key plumbing is the auth change)
- **TensorZero version upgrade procedure** → Phase 3a.i (ships with the first compose-version pin)
- **Per-project kill switch** → Phase 3g (ships with cost control)
- **Data-deletion-on-request** → Phase 3f

Phase 6 is what's left: the **entry-point doc** (`README.md`), the **operator quick-reference** that links to the per-subtask runbooks already written, and the **observability cheatsheet**.

### Phase 6 deliverables

1. `orchestrator/tensorzero/README.md` — onboarding entry-point. Three sections:
   - **What this is** — one paragraph.
   - **For operators** — links to `ops/README.md` (the assembled runbook) and the cheatsheet below.
   - **For project authors** — links to `CONVENTIONS.md`, the Phase 4 client crate (if it exists), and the data-classification rule from 3f.

2. `orchestrator/tensorzero/ops/README.md` — assembled runbook. Sections, with most content already written during earlier phases:
   - Start / stop / restart (this phase — trivial)
   - Adding a new project (this phase — 3-step recipe linking to CONVENTIONS.md)
   - Rotating provider API keys (from 3a.i)
   - Rotating the gateway bearer key (from 3a.i)
   - Restoring from backup + quarterly drill checklist (from 3c)
   - Upgrading TensorZero version (from 3a.i)
   - Per-project kill switch (from 3g)
   - Data-deletion-on-request (from 3f)
   - SSH-tunnel one-liner for UI access (from 3a.ii)

3. `orchestrator/tensorzero/CHEATSHEET.md` — observability cheatsheet, below.

### Trivial bits this phase actually writes

**Start / stop / restart**:
```sh
cd orchestrator/tensorzero/deploy
docker compose up -d              # start
docker compose restart gateway    # restart just gateway
docker compose down               # stop (volumes preserved)
docker compose down -v            # stop + delete ALL data (destructive — ClickHouse + Postgres)
```

**Adding a new project**:
1. Open PR against `orchestrator/tensorzero/config/tensorzero.toml` adding the function block(s) per CONVENTIONS.md
2. Wait for CI semantic smoke check (Phase 2) to pass; merge
3. `docker compose up -d` to pick up the new config (gateway reloads on container restart)
4. In the consuming project: integrate via the client pattern (Phase 4) with the `project` tag set

### Observability cheatsheet — top queries

These use **tag-based filtering** (per Phase 2), not function-name prefix matching.

```sql
-- Top functions by call volume (last 24h), grouped by project
SELECT JSONExtractString(tags, 'project') AS project,
       function_name,
       COUNT(*) AS calls
FROM tensorzero.ChatInference
WHERE timestamp > now() - INTERVAL 1 DAY
GROUP BY project, function_name
ORDER BY calls DESC;

-- p50/p95/p99 latency by function (last 7d)
SELECT function_name,
       quantile(0.5)(processing_time_ms) AS p50,
       quantile(0.95)(processing_time_ms) AS p95,
       quantile(0.99)(processing_time_ms) AS p99
FROM tensorzero.ChatInference
WHERE timestamp > now() - INTERVAL 7 DAY
GROUP BY function_name;

-- Token spend by project (last 24h) — uses tag, not function_name prefix
SELECT JSONExtractString(tags, 'project') AS project,
       SUM(input_tokens + output_tokens) AS total_tokens
FROM tensorzero.ChatInference
WHERE timestamp > now() - INTERVAL 1 DAY
GROUP BY project;

-- Failures by provider model
SELECT model_name, COUNT(*) AS fails
FROM tensorzero.ModelInference
WHERE timestamp > now() - INTERVAL 1 DAY AND error IS NOT NULL
GROUP BY model_name;

-- Fallback rate by project (last 1h) — pairs with Phase 4 logs
-- This query alone is partial: silent fallbacks at the client layer never reach
-- ClickHouse. Always combine with `grep tensorzero_fallback_invoked` over project logs.
SELECT JSONExtractString(tags, 'project') AS project,
       countIf(JSONExtractString(tags, 'fallback_policy') = 'best-effort') AS best_effort_calls
FROM tensorzero.ChatInference
WHERE timestamp > now() - INTERVAL 1 HOUR
GROUP BY project;

-- Episode tracing — all inferences for one conversation
SELECT inference_id, function_name, processing_time_ms
FROM tensorzero.ChatInference
WHERE episode_id = '<uuid>'
ORDER BY timestamp;
```

Verify table/column names against the actual ClickHouse schema at execution time — the TensorZero schema evolves with versions. Run `SHOW TABLES IN tensorzero` and `DESCRIBE tensorzero.ChatInference` first.

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

`.env` files contain real provider keys and, after Phase 3a.i, the `POSTGRES_PASSWORD` and any TensorZero API keys minted via the admin flow. Do not commit any of them. The `.env.example` template is committed; the real `.env` is gitignored. Verify `.gitignore` in the new repo lists `.env` before first commit. Treat the Postgres password and any minted gateway keys as keys of equivalent sensitivity to the provider keys — anyone with the gateway key can spend the provider keys.

### Existing Loker functions stay valid; model blocks rename

`loker_d1_anthropic`, `loker_d1_openai`, `loker_d1_google` already match the function-naming convention (Phase 2). The **model blocks** they reference rename in Phase 2 (`loker_anthropic_haiku` → `anthropic_haiku45`, etc.) — the function-block update happens in the same PR, so ClickHouse history continues uninterrupted (function name unchanged; only the internal `model = "..."` reference changes).

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

The new `orchestrator/tensorzero` repo needs CI that catches semantic, not just syntactic, breakage. Per Phase 2:
1. **Naming-convention check** (`ci/validate_names.py`) — regex over function and model block names.
2. **`docker compose config` lint** — catches YAML errors in the compose file.
3. **Semantic smoke check** — start the pinned gateway image against the actual TOML with dummy provider keys and assert `/health` returns 200. This catches bad model references, unknown family suffixes, and TOML/schema drift before they break the shared gateway at runtime.

No application tests beyond that — the gateway is upstream.

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
| ClickHouse schema breaks on TZ upgrade | Medium | High (observability blackout) | Pin gateway+UI to the same version tag; test upgrades in dev first; backup before upgrade (3c); restore-drill quarterly |
| Postgres-or-ClickHouse-or-gateway version skew after Phase 3a | Medium | High (gateway refuses to start) | Pin all three images in compose; upgrade procedure (3a.i) requires bumping in lockstep + restore drill |
| Gateway bearer key or Postgres password leaks | Low | High (provider key abuse) | Phase 3a.i documents quarterly rotation; per-project keys cheap to add once Postgres is in place |
| UI exposed without auth | Medium until 3a.ii ships | High (every prompt/response readable) | Loopback binding from Phase 1 holds the line; 3a.ii required before broadcasting UI access; data-classification rule (3f) limits what is OK to send through the shared instance |
| Runaway loop in one project spends shared keys | Medium | High | Provider-side budget caps (3g); spend-spike alerting (3g); per-project kill switch (3g) |
| Silent fallbacks hide gateway outage | Medium | High (we lose the thing we built this for) | Phase 4 fallback observability — counter + structured log + alert on fallback rate >1% |
| Moving the volume loses history | Low if explicit-rename procedure followed | Low (dev data) | Document the choice in the Loker PR; default to accepting loss |
| New project adoption stalls because of the auth + tags + fallback plumbing | Medium | Low | Provide a tiny `tensorzero-client` helper crate (Rust) for orchestrator projects — one-call interface that handles auth, required tags, redaction hook, and fallback. Build only if Phase 5 reveals friction |
| `orchestrator/tensorzero` becomes unowned | Medium | Medium | Single-operator setup is fine for now; revisit if the team grows |

---

## References

- TensorZero docs (root): https://www.tensorzero.com/docs (verify version compatibility with 2026.4.1 at execution time)
- TensorZero auth (basis for Phase 3a rewrite): https://www.tensorzero.com/docs/operations/set-up-auth-for-tensorzero
- TensorZero data model (basis for Phase 3f data-privacy phase): https://www.tensorzero.com/docs/gateway/data-model
- Loker round-trip spike: `loker/docs/spikes/2026-04-25-tensorzero-roundtrip.md`
- Loker family-naming verdict (CLO-247): comment block at `loker/tensorzero/config/tensorzero.toml:36-43`
- `family_of(backend_id)` derivation (CLO-251 / FR-13): in Loker source
- Existing deploy README: `loker/deploy/tensorzero/README.md`
- Lok prior-art reference: `lok/docs/design-docs/clo-212-role-routing-config.md`

---

## Changelog

- **rev 1** (2026-05-18): initial draft, 6 phases.
- **rev 2** (2026-05-18): incorporated review feedback.
  - **#1 Auth**: rewrote 3a. Auth lives in `tensorzero.toml` (`[gateway.auth] enabled = true`), requires Postgres (new service in compose), `/health` and `/status` stay unauthenticated, UI has no built-in auth (new 3a.ii subtask for Tailscale/OAuth2-Proxy/Nginx/SSH-tunnel options), verification moved to `/inference`.
  - **#2 Volume + cutover**: Phase 1 reframed as copy-into-new-repo + delete-from-Loker with explicit cutover window. Volume rename via Docker Compose project prefix called out; explicit-rename procedure documented in Cross-Cutting Concerns.
  - **#3 Port binding**: Phase 1 docker-compose.yml now binds gateway/UI/ClickHouse to `127.0.0.1` until 3a + 3b ship.
  - **#4 Blast radius**: CI smoke check in Phase 2 starts the actual pinned gateway against the TOML, catching semantic breakage before merge (not just YAML lint).
  - **#5 Data privacy**: new Phase 3f covers data classification, per-project redaction hook, ClickHouse TTL, UI access control (links to 3a.ii), backup encryption (links to 3c), deletion-on-request workflow.
  - **#6 Cost control**: new Phase 3g covers mandatory `project` tag, provider-side budget caps, ClickHouse spend-spike alert, per-project kill switch.
  - **#7 Naming**: Phase 2 reworked to split metadata across `function_name` (small) and **tags** (rich). Required tags: `project`, `env`, `workflow`, `owner`, `fallback_policy`. Cheatsheet queries use tag-based filtering.
  - **#8 Model ownership**: shared, not project-prefixed. Renaming `loker_*` model blocks to `<provider>_<modelshortname>` is part of Phase 2. Phase 5 example updated.
  - **#9 Runbook timing**: backup/restore moved into 3c (with quarterly restore drill); key rotation moved into 3a.i; upgrade procedure moved into 3a.i. Phase 6 reduced to entry-point doc + cheatsheet, with explicit list of which prior-phase docs it assembles.
  - **#10 Fallback observability**: Phase 4 now mandates counter + structured log + alert at fallback rate >1%; tests assert counter increments.
