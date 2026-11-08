---
status: done
branch: fix/gh-api-downloads
repos: [adr-lint, attest, decisions, fleet-lsp, cluster-vision, duro-operator, duro-design-system, duro-app, homelab-preview-operator, authelia-oidc-operator, ddns-updater-operator, aval, amont-agent, amont, relais, charts, activity-vision, application-landscape, customer-vision, kb-vision, social-planner, sre-agent, trade-agents, website-builder, homelab]
adrs: []
---
# CI fetches GitHub release assets through the API, fleet-wide

## Review panel
📄 Full reviews: [2026-11-08-ci-fetches-release-assets-api.reviews.md](2026-11-08-ci-fetches-release-assets-api.reviews.md)
👉 **Decide:** none. Approve if `gh release download` on the API route, with retries, should replace every anonymous web-route curl in 25 repositories.
📍 25 repos · planned · next: the adr-lint pilot PR. Panel: backend (opus-5-5), platform (opus-5-5), codex.
**Changed by review:** the homelab `scripts/ci` helpers are now in scope; each download needs a `gh` line in the job log, not only green; the old gh 2.62.0 on the runners is tested.
**Verdicts:** backend, platform and codex approve-with-changes. You chose to reuse GH_BRIDGE_TOKEN. 4 findings are carried into implementation, plus 1 low one: cross-reference the CI guard in Phase 4.

## Picture
none: one recipe swapped per call site

## Context

On 2026-11-08 GitHub's anonymous web route, `github.com/<o>/<r>/releases/download/…`,
was throttled and answered 504 for hours, while the authenticated API asset route
served the same files. relais's CI moved its adr-lint download to
`gh release download` with `GH_TOKEN` (branch `fix/plugin-spawn-cwd`, not merged yet).
grh's CLI and its `deny` job got the same fix (grh#25, grh#27). Every other repository
still curls the web route anonymously. This plan gives each of them the same fix.

Inventory (2026-11-08, live checkouts; each worktree re-greps `origin/main`):
- 16 GitHub-hosted repositories, mostly the copied "Ensure aval" block in `adr.yaml`.
  No generator emits that block (grep across aval, adr-lint, amont-agent, amont,
  grh and skills found none), so every copy is edited by hand.
- 8 Forgejo app repositories (aval, adr-lint and ast-grep in `adr.yaml`/`ci.yaml`)
  and homelab (about 12 tools).
- Forgejo facts: every runner label is `<label>:host` on the `forgejo-runner` image,
  which bakes `gh` and `jq` (`homelab/services/forgejo-runner/Dockerfile:182`).
  `secrets.GITHUB_TOKEN` there is a Forgejo token, so api.github.com answers 401
  (`trade-agents/.forgejo/workflows/ci.yml:464`). `GH_BRIDGE_TOKEN` is a Forgejo
  **org** secret, a GitHub PAT that homelab already uses (`mirror-discover.yaml`).
  homelab's `dl()` sends it as a bearer on the web route, but curl drops it at the
  redirect to the asset host (its own comment, `argo-lint.yaml:43`).
- Checked locally with gh 2.103.3: `gh release download -O -` streams into `tar`,
  and a tag containing a slash (`kustomize/v5.5.0`) works.

## Recipe

**A, GitHub Actions.** The step gets `GH_TOKEN: ${{ github.token }}` in its `env:`.
The curl becomes:

```sh
f="$RUNNER_TEMP/<asset>"
for attempt in 1 2 3; do
  gh release download "<tag>" --repo <owner>/<repo> --pattern "<asset>" -O "$f" --clobber && break
  [ "$attempt" = 3 ] && { echo "::error::could not download <asset> from <owner>/<repo> <tag>"; exit 1; }
  sleep 5
done
```

Then the existing `sha256sum -c` (where there is one) and `tar … -f "$f"`. The loop
keeps the 3 tries the old `curl --retry 3` had. Downloading to a file instead of
piping means a retry never feeds half an archive to `tar`. A comment above the step
names the reason in one line: the web route throttles anonymous downloads and
returned 504 on 2026-11-08, while the API served the files.

**B, Forgejo.** Same loop. The person chose on 2026-11-08 to reuse `GH_BRIDGE_TOKEN` rather than mint a read-only token: it is already an org secret any org workflow can name, so the extra exposure is small. The env is `GH_TOKEN: ${{ secrets.GH_BRIDGE_TOKEN }}` and
`GH_HOST: github.com`, because origin is git.daddyshome.fr and `gh` must not infer its
host from it. If `GH_TOKEN` is empty, the step fails at once with
`::error::GH_BRIDGE_TOKEN is empty, and api.github.com cannot be reached without it`.
There is no anonymous API fallback: all the runners share one NAT IP, and the anonymous
API limit is 60 requests an hour per IP. Authenticated: about 2 requests per asset (release lookup + asset), so about 24 per homelab run (12 tools) and at most 6 per app repository run, against 5,000 an hour per token: more than 200 homelab runs an hour.

**C, the other GitHub reads.**
- `raw.githubusercontent.com` becomes
  `gh api repos/<o>/<r>/contents/<path>?ref=main -H 'Accept: application/vnd.github.raw'`.
- An anonymous `api.github.com/…/releases/latest` call becomes
  `gh release view --repo <o>/<r> --json tagName -q .tagName`.
- In PowerShell, the step gets `GH_TOKEN` in its env and the call gets `Authorization = "Bearer $env:GH_TOKEN"`. Its `catch` skips only on a 404 and rethrows any other status, so a 401 or 403 fails the step instead of reading as 'no published release yet' (`amont/.github/workflows/ci.yaml:571-578`).

## Phases

One worktree, branch and PR per repository (`worktree-task`). The edits are made by
hand, since they are mechanical one-block swaps (memory:
small-fixes-by-hand-not-relais-rounds). Each PR is merged on green with
`merge-when-green` (Forgejo: `forgejo-recipes`). Every commit subject is
`ci: fetch release assets via the GitHub API`.

### Phase 1: GitHub repositories with a single aval call site (recipe A)
`adr-lint/.github/workflows/adr.yaml`, `attest/.github/workflows/adr.yaml`,
`decisions/.github/workflows/ci.yaml`, `fleet-lsp/.github/workflows/adr.yaml`,
`cluster-vision/.github/workflows/adr.yaml`, `duro-operator/.github/workflows/adr.yaml`,
`duro-design-system/.github/workflows/adr.yaml`, `duro-app/.github/workflows/adr.yml`,
`homelab-preview-operator/.github/workflows/adr.yaml`,
`authelia-oidc-operator/.github/workflows/adr.yaml`,
`ddns-updater-operator/.github/workflows/adr.yaml`.
The PR's own CI runs the aval step, which proves the change. Start with `adr-lint` as
the pilot. Once it is green, the other 10 follow with the identical block. A repository listed under `## Code review` as `none` keeps that only if its hunk, diffed against adr-lint#53's hunk, differs in nothing but the `AVAL_VERSION` line; otherwise it goes back to `backend, platform`.

### Phase 2: GitHub repositories with more than one call site (recipes A and C)
- `aval/.github/workflows/adr.yaml`, `aval/.github/workflows/docs.yaml` (mdBook).
- `amont-agent/.github/workflows/adr.yaml`, `amont-agent/.github/workflows/ci.yaml`
  (adr-lint), `amont-agent/.github/workflows/docs.yaml` (mdBook).
- `amont/.github/workflows/ci.yaml`: aval, hadolint, yq, kube-linter, kubeconform and
  kustomize (A); the raw MANIFEST fetch at line 68 (C); the PowerShell `releases/latest`
  call at line 573 (C).
- `amont/.github/workflows/docs.yaml` (mdBook) and
  `amont/.github/workflows/release.yaml:301` (attest).
- `relais/.github/workflows/release.yaml:283` (attest). relais's `ci.yaml` stays with
  `fix/plugin-spawn-cwd`.
- `charts/.github/workflows/helm-deps-update.yaml:62` (C).
Check each edited job's `permissions:` block: `contents: read` is enough for public
releases, but an empty `permissions: {}` block would starve the token.

### Phase 3: Forgejo app repositories (recipe B)
`activity-vision/.forgejo/workflows/adr.yaml`,
`application-landscape/.forgejo/workflows/adr.yaml`,
`customer-vision/.forgejo/workflows/adr.yaml` (aval, adr-lint, ast-grep),
`kb-vision/.forgejo/workflows/adr.yaml`, `social-planner/.forgejo/workflows/adr.yaml`,
`sre-agent/.forgejo/workflows/ci.yml`,
`trade-agents/.forgejo/workflows/adr.yml` (aval, adr-lint, ast-grep),
`website-builder/.forgejo/workflows/ci.yaml`.
`kb-vision` is the pilot: its PR run shows that a same-repo PR receives the org secret.
The other 7 follow once it is green. application-landscape, social-planner, sre-agent and website-builder may drop to implementation-review alone (the person's choice, 2026-11-08) only after the kb-vision PR has merged green with aval fetched through `gh` in its log, and only if their hunk, diffed against kb-vision's, differs in nothing but the version line; that is a further `## Code review` delta.

### Phase 4: homelab (recipe B)
`homelab/.forgejo/workflows/ci-validate.yaml`: both `dl()` helpers become one
`gh release download` per tool, and the kube-linter, pluto, aval, shellcheck, gitleaks
and trivy curls are converted. Also `homelab/.forgejo/workflows/argo-lint.yaml` and
`homelab/.forgejo/workflows/publish-docs.yaml`. `homelab-wt-thanos-skeleton` edits
`ci-validate.yaml` in other hunks (detect-changes, a new job), so whichever of us lands
second rebases. The comments about the bearer token that curl drops at the redirect
are removed along with the curls. The scripts ci-validate calls are converted too: `scripts/ci/install-yq.sh:64`, `scripts/ci/manifest-tools.sh:72-96`, `scripts/ci/go-toolchain.sh:89` and `scripts/ci/test-litellm-bootstrap.sh:18`. They also run on a laptop, so each one uses `gh release download` when `GH_TOKEN` is set and keeps its curl otherwise.

## Verification
- Before each push, every converted download is run locally with
  `GH_TOKEN=$(gh auth token)`, by the exact new command, followed by its
  `sha256sum -c`, its `tar` extraction or the binary's `--version`. Each Forgejo download is also run once with gh 2.62.0, the version the runner image bakes (`homelab/services/forgejo-runner/Dockerfile:181`), including `-O` and the slash tag `kustomize/v5.5.0`, because the local check used 2.103.3.
- After each edit, `command grep -nE 'github\.com/[^ ]+/releases/download|raw\.githubusercontent'`
  across the whole repository (workflows and the scripts they call) prints nothing outside comments, the non-goals and the laptop-only curl fallback in the 4 homelab `scripts/ci/*` helpers.
- The empty-token guard: the recipe B step's script, run locally with `GH_TOKEN=''`, exits 1 and prints the `::error::… is empty` line.
- PR CI: the adr and aval steps run on every PR (phases 1 and 3, and the PR jobs of
  phases 2 and 4). Read each run's `conclusion` explicitly. A green conclusion is not
  evidence on its own: a path filter can skip the job (homelab `ci-validate.yaml:143`, the
  `argo-lint.yaml` paths), and a baked binary at the pinned version returns before the
  download. For each converted download, the PR's job log must show the `gh release
  download` line. If it doesn't, the step is run on the target runner with the download
  forced, through `workflow_dispatch` or a commit that touches the filtered path.
- Some jobs a PR does not run. `docs.yaml` gets `workflow_dispatch` on the merged main
  where it has the trigger; otherwise its next main push is the observation.
  `release.yaml` is covered by the local run, and its next tag confirms it.
  `helm-deps-update` is dispatched once after the merge.
- At the end, the fleet re-grep from Context, run against each `origin/main`, finds
  only the non-goals below.

## Carried into implementation
- homelab `scripts/ci/*`: when `CI` is set and `GH_TOKEN` is empty, exit 1 instead of falling back to curl; each calling step sets `GH_TOKEN` and `GH_HOST: github.com`.
- `scripts/ci/test-litellm-bootstrap.sh`: the asset name is decoded from `%2B` to `+` for `--pattern`.
- Each `gh release download` attempt keeps a per-attempt timeout like the old curl `--max-time`.

## Non-goals
- Product installers (`install.sh`, `install.ps1`, grh) keep the web route: users have
  no token, and grh already falls back to the API (grh#25).
- Forks and other people's repositories: chezmoi (origin is twpayne), garage-operator
  and happier (their lines are upstream's).
- `homelab/*-operator/.github/workflows/*`: nested copies that no CI runs.
- Dockerfile build-time downloads, such as the runner image's own `gh`: a build needs a
  secret mount. Offered as a follow-up.
- A lint that refuses the web route in new CI. Offered as a follow-up amont check.
- `iroh` (cargo-deny-action): not one of the person's repositories.
- The operators' Makefile `golangci-lint/install.sh` (authelia-oidc, ddns-updater, duro and homelab-preview operators): a third-party installer that fetches from the web route itself. Offered as a follow-up (`go install` or `gh release download`).

## Code review
- adr-lint: backend, platform
- attest: none — a copy of the recipe A block that backend and platform approved in adr-lint#53, differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- decisions: none — a copy of the recipe A block that backend and platform approved in adr-lint#53, differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- fleet-lsp: none — a copy of the recipe A block that backend and platform approved in adr-lint#53, differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- cluster-vision: none — a copy of the recipe A block that backend and platform approved in adr-lint#53, differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- duro-operator: backend, platform
- duro-design-system: none — a copy of the recipe A block that backend and platform approved in adr-lint#53, differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- duro-app: none — a copy of the recipe A block that backend and platform approved in adr-lint#53, differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- homelab-preview-operator: none — a copy of the recipe A block that backend and platform approved in adr-lint#53, differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- authelia-oidc-operator: none — a copy of the recipe A block that backend and platform approved in adr-lint#53, differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- ddns-updater-operator: none — a copy of the recipe A block that backend and platform approved in adr-lint#53, differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- aval: backend, platform
- amont-agent: backend, platform
- amont: backend, platform
- relais: backend, platform
- charts: backend, platform
- activity-vision: backend, platform
- application-landscape: none — a copy of the recipe B block that backend and platform approved in kb-vision#59 (merged; its log shows aval fetched through gh), differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- customer-vision: backend, platform
- kb-vision: backend, platform
- social-planner: none — a copy of the recipe B block that backend and platform approved in kb-vision#59 (merged; its log shows aval fetched through gh), differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- sre-agent: none — a copy of the recipe B block that backend and platform approved in kb-vision#59 (merged; its log shows aval fetched through gh), differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- trade-agents: backend, platform
- website-builder: none — a copy of the recipe B block that backend and platform approved in kb-vision#59 (merged; its log shows aval fetched through gh), differing only in the `AVAL_VERSION` pin (the person's choice, 2026-11-08); implementation-review alone
- homelab: backend, platform

## Decision log
- 2026-11-08: the per-attempt `timeout 120`, `sleep $((attempt * 5))` backoff and the exit code in the `::error::` line extend Recipe A (code-review backend and platform, adr-lint pilot). Every later copy is written by the same transformer, so the block stays identical fleet-wide.
- 2026-11-08: the 3-attempt retry is a permanent policy, not a stopgap; it has no `holds-until:` ceiling. 4xx is retried too: telling it from 5xx would mean parsing gh's stderr across gh 2.62.0 and 2.103.x, and a permanent error costs at most about 15 s.
- 2026-11-08: the person chose to review the copies with implementation-review alone (`## Code review`): the 9 Phase 1 aval-only repositories, whose hunk diffed against adr-lint#53 is identical (checked). The 4 Forgejo aval-only repositories wait for the kb-vision pilot. Delta reviewed by backend and platform (approve), plan sha e7c66558f95f.

- 2026-11-08: Phase 3's further `## Code review` delta: kb-vision#59 merged green, its ADR log showing `runner carries aval 0.3.1 … downloading it` then `aval 1.11.0` through gh with the org secret. The +/- lines of application-landscape, social-planner, sre-agent and website-builder are identical to kb-vision's (the `AVAL_VERSION` pin is unchanged context in each), so they get implementation-review alone. Delta reviewed by backend and platform (approve), plan sha 915bb42cda75.

## Outcome
Phase 1 pilot, adr-lint (`.github/workflows/adr.yaml`):
- input `AVAL_VERSION=1.11.0`, GH_TOKEN from `gh auth token`, the step script on macOS → expected: aval downloaded through gh and extracted to `$RUNNER_TEMP/bin`, GITHUB_PATH written → actual: `bin/aval` is an ELF x86-64 executable and GITHUB_PATH was written; only `aval --version` failed, because a Linux ELF cannot run on macOS.
- input `AVAL_VERSION=9.9.9` (no such release) → expected: 3 attempts, then `::error::…` and exit 1 → actual: "release not found" 3 times, then `::error::could not download … (exit 1; 124 = timed out after 120 s)`, exit 1.
- input: `timeout` absent from PATH → expected: the step fails, it does not pass → actual: 3 failed attempts, then `::error::`, exit 1.

Fleet outcome (all merged 2026-11-08):
- Phase 1 (GitHub, aval block): adr-lint#53, attest#41, decisions#170, fleet-lsp#33, cluster-vision#60, duro-operator#22, duro-design-system#175, duro-app#162, homelab-preview-operator#37, authelia-oidc-operator#19, ddns-updater-operator#19. Each PR log shows `downloading it` then the pinned `aval` version.
- Phase 2: aval#56, amont-agent#220, amont#440, relais#412 (its first-round review caught a step without GH_TOKEN, fixed), charts#531 (the anonymous lookup could write the tag "null"; it now warns and checks the tag shape).
- Phase 3 (Forgejo): kb-vision#59 (pilot: the ADR log shows aval fetched through gh with the org secret), activity-vision#16, customer-vision#37, trade-agents#418, application-landscape#340, social-planner#69, sre-agent#65, website-builder#229.
- Phase 4: homelab#1117 — one helper, `scripts/ci/gh-release-dl.sh`, for 11 workflow downloads and 4 scripts, plus the authelia template read; 8 of its jobs show the gh downloads in their logs.
- The fleet re-grep of every `origin/main` finds only amont's authenticated PowerShell `releases/latest` call and the helper's laptop fallback.
- The web route was down again during the work (a 60 s timeout, a 503 at 12:34Z) while every API fetch succeeded; it broke duro-operator's `setup-envtest` and kb-vision's `better-sqlite3` prebuild until it recovered (both re-run green).

Follow-ups, not done:
- Tool-internal web-route downloads the text grep cannot see: `setup-envtest` (Go operators), `prebuild-install` (npm native addons), attest's `fetch.sh` action (consumers fall back to verify.sh, after minutes).
- Dockerfile build-time downloads (sre-agent `docker/*.Dockerfile`, the runner image); the operators' Makefile golangci `install.sh`.
- homelab: the adr, argo-lint and publish-docs jobs were path-filtered out of #1117; their first run after a matching change is their CI observation. Test hardening from backend: a fake gh that writes a partial file, and a retry plus a guard test in `generate.mjs`.
- A lint that refuses the web route in new CI (an amont check).

## Implementation review
- adr-lint implementation-review: approve-with-changes, then approve-with-changes on the Delta, all low; deliberate: retry policy permanent, plan snippet superseded by Carried and Decision log; 48k+34k, 15+23 s (opus).
- adr-lint backend: approve; the backoff and exit code it asked for were fixed, 4xx retry deliberate (Decision log); 52k+23k, 41+9 s (opus).
- adr-lint platform: approve; the backoff was fixed in the transformer before any copy; 65k+24k, 38+14 s (opus).

<!-- panel: repos=adr-lint,attest,decisions,fleet-lsp,cluster-vision,duro-operator,duro-design-system,duro-app,homelab-preview-operator,authelia-oidc-operator,ddns-updater-operator,aval,amont-agent,amont,relais,charts,activity-vision,application-landscape,customer-vision,kb-vision,social-planner,sre-agent,trade-agents,website-builder,homelab reviewers=backend,platform,codex body-sha=915bb42cda75 -->
