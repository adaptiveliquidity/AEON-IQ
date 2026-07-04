# Longitudinal Memory Benchmark — Results (smoke-scale, 2026-07-04)

**Status: preliminary / smoke-scale (n = 3 conversations).** These numbers are a
go/no-go and mechanism-sanity pass, **not** publication-grade. See
[LONGITUDINAL_DESIGN.md](LONGITUDINAL_DESIGN.md) for the pre-registered protocol
and [§6 Caveats](#6-caveats--what-was-and-wasnt-exercised) below for exactly what
was and was not exercised. Do not quote a single number from this file without its
caveat.

Authority: the pre-registration in `LONGITUDINAL_DESIGN.md` is frozen. This file
records the run, every amendment made to the frozen protocol (with reasons,
recorded before the numbers), and the honest findings — including a real
retrieval bug and a benchmark-metric artifact we found in our *own* harness.

---

## 1. One-paragraph summary

The original seed-then-query benchmark showed `aeon-full == cosine-baseline`
because it never aged memories or applied pressure. This harness manufactures
history (SQL-injected timelines, rounds, feedback, memory pressure) so the
differentiators are actually exercised. At smoke scale we find: **(a)** on
retrieval *ranking* over an aged corpus, decay / importance / graph / RMK show **no
statistically meaningful lift** at n = 3 (differences are single-query noise); and
**(b)** under memory pressure, **AMP eviction retains ~81–95 % of gold vs ~38–50 %
for LRU and ~39–47 % for random** — a real, consistent ~2× advantage across every
agent. Getting to those honest numbers required fixing four harness bugs and one
metric artifact (§4), one of which had made AMP look like a *perfect* 100 %.

---

## 2. Pre-registered amendments (recorded before results)

Each of these departs from the frozen `LONGITUDINAL_DESIGN.md`; all were forced by
findings at the smoke stage and are logged here for the anti-cherry-pick trail.

| # | Amendment | Reason | When found |
|---|-----------|--------|-----------|
| A1 | Dataset `longmemeval_oracle` → `longmemeval_s` | Oracle is ~all-gold (12–43 gold turns per query over ~23 memories/agent): baseline ceilings at recall 1.0 with **no headroom** for any condition to show a lift. `_s` has a real distractor haystack. | smoke |
| A2 | For the smoke, `_s` haystack **truncated** to gold sessions + 12 distractor sessions/record (~139 turns/agent vs ~492) | Full `_s` = ~492 turns/agent; seeding 3 agents × 2 conditions blows the wall-clock budget. Truncation hits both baseline and every condition **equally**, so relative comparisons are unaffected. The full run must use untruncated `_s`. | smoke |
| A3 | `SIM_EPOCH` anchored to **run-time now** (was fixed `2026-01-01`) | Harness bug — see §4.1. Not a tuning choice; a correctness fix. | smoke |
| A4 | `gold_retention` counts `soft_evicted` as evicted | Metric artifact — see §4.2. Correctness fix. | full run |
| A5 | Kernel run with `MEMORYOS_ROLE=proxy` during the run | Serve the forced-sweep endpoint but disable background workers, so forced sweeps are deterministic and the background pressure-sweep job doesn't deadlock against the harness's SQL. **Consequence:** background RMK policy-learning and co-access decay are **not** exercised (see §6). | full run |

Decay stays at the pre-registered **configured** value `MEMORY_DECAY_RATE=0.03`,
`IMPORTANCE_BOOST_FACTOR=0.5` (ships as 0.0 — write-ups must say "configured
decay," not a shipped default). AMP uses shipped defaults (target ≈ 1000 hardcoded;
no tuning PR).

---

## 3. Results

Run: `longmemeval_s` (truncated, A2), seed 42, 3 conversations, 5 rounds,
`AEON_SEMANTIC_THRESHOLD=0.95` (a cosine-*distance* ceiling — permissive),
real OpenAI `text-embedding-3-small`, kernel built from this branch.

### 3.1 Retrieval ranking (rounds phase) — n = 3 queries, HIGH NOISE

"recall@k" here is hit@k (≥1 gold in top-k), the QA convention.

| condition | recall@1 | recall@10 | MRR@10 | nDCG@10 |
|-----------|:-------:|:--------:|:------:|:-------:|
| cosine-baseline | 0.667 | 1.000 | 0.722 | 0.693 |
| decay-only | 1.000 | 1.000 | 1.000 | 0.777 |
| amp-only | 0.667 | 1.000 | 0.722 | 0.693 |
| aeon-full | 0.667 | 1.000 | 0.714 | 0.670 |
| aeon-full-importance | 0.667 | 1.000 | 0.704 | 0.668 |

**Read this as noise, not signal.** At n = 3, one query flipping changes recall@1
by 0.333. `decay-only` scoring 1.000 and `aeon-full` scoring 0.667 differ by a
single query. `amp-only == baseline` exactly (expected: with decay/importance/graph
off, AMP does nothing at retrieval time — it only acts under pressure). **No
condition shows a defensible ranking lift or drag over baseline at this scale.**
The `aeon-full` numbers here exclude live RMK learning (A5).

### 3.2 Memory pressure (eviction) — the robust result

Per agent: ~140 real memories + 2000 SQL-injected far-embedding distractors (§4.4)
→ ~2140 active vs target 1000. The forced-sweep endpoint drives AMP to soft-evict
**K ≈ 1150**; LRU and random comparators evict the **same K** over fresh copies of
the identical aged corpus. Primary metric is `gold_retention` (fraction of gold
still retrievable), corrected per A4.

| condition | **AMP** | LRU | random |
|-----------|:------:|:---:|:------:|
| amp-only | **0.947** | 0.504 | 0.436 |
| aeon-full | **0.855** | 0.445 | 0.467 |
| aeon-full-importance | **0.808** | 0.383 | 0.390 |

Per-agent AMP retention (kept/total): amp-only (23/24, 38/43, 12/12); aeon-full
(22/24, 35/43, 10/12); importance (19/24, 38/43, 9/12). **AMP beats both
comparators on every single agent.** This is the strongest, most consistent finding
in the run — AMP's utility/pheromone pressure preferentially evicts the
never-accessed distractors and largely spares the gold the rounds phase reinforced,
while gold-blind LRU/random destroy roughly half the gold.

**Honest sub-findings:**
- **AMP is good, not perfect.** It soft-evicted real gold on most agents (retention
  0.81–0.95, not 1.0). The perfect 1.0 we first saw was a metric artifact (§4.2).
- **Decay/importance slightly *hurt* retention under pressure**
  (amp-only 0.947 > aeon-full 0.855 > importance 0.808). AMP's pressure term grows
  with `days_stale`, so decay-aged gold gets more eviction pressure; and
  `importance_score` does **not** protect a memory from AMP eviction (AMP's pressure
  ignores importance). Seeding gold at high importance therefore does *not* improve —
  and slightly worsens — pressure survival.
- **The PI controller did not converge.** It hit the 20-sweep cap oscillating
  active ≈ 880–984 around target 1000 (restoring soft-evicted memories when active
  dipped below target), never settling within the deadband for 2 consecutive sweeps.
  The eviction comparison is still valid (all arms evict the same settled K), but
  "convergence" should not be claimed.

---

## 4. Bugs and artifacts found (in our own harness/kernel)

### 4.1 Harness: fixed-past `SIM_EPOCH` over-aged everything (fixed)
`SIM_EPOCH` was hardcoded `2026-01-01`, but the kernel measures decay staleness
from real `NOW()` (`src/memory/store.rs`, `days_stale = EXTRACT(EPOCH FROM (NOW() -
last_accessed_at))/86400`). Run on 2026-07-03 this injected ~183 phantom days;
`exp(0.03·183) ≈ 242×` inflated every distance past the retrieval threshold and
collapsed `aeon-full` recall to **0.0**. Anchored to run-time now; relative aging
stays deterministic (seed-hash).

### 4.2 Harness: `gold_retention` ignored `soft_evicted` (fixed)
`gold_retention` counted only `archived_at IS NULL`, but AMP evicts via
`soft_evicted = TRUE` (never `archived_at`), while LRU/random set `archived_at`.
AMP's evictions were therefore **invisible** to the metric → a guaranteed 1.0 on
every agent regardless of what AMP evicted (empirically: AMP had soft-evicted 25
real memories on one agent while the metric reported 1.0). Fixed to match the
kernel's own retrievability filter (`archived_at IS NULL AND soft_evicted = FALSE`).
Corrected AMP retention: 0.81–0.95 (§3.2).

### 4.3 Harness: two more (fixed)
- `run_rounds` passed raw search-result dicts to the feedback rule (expected string
  ids) → `TypeError`.
- `execute_file` ran multi-statement `.sql` via one psycopg `execute()` → *"cannot
  insert multiple commands into a prepared statement."* Added a string/comment-aware
  statement splitter.

### 4.4 Harness: distractor isolation (design step 1, done)
Pressure distractors were originally seeded through the API with **real** embeddings
into the scored agent, so they could surface in top-k and pollute recall. Replaced
with a direct SQL insert of a fixed **far** (one-hot) embedding: distractors count
toward AMP `active_count` (pressure) but sit at cosine distance ≈ 0.98 (> the 0.95
ceiling) so they are never retrieval candidates. `gold_retention` is the primary
pressure metric; post-eviction recall is over real memories only.

### 4.5 Kernel: decay filters below threshold instead of only re-ranking (LATENT; not fixed)
`src/memory/store.rs` applies decay to the distance that is then compared to the
threshold (`WHERE cosine_dist·exp(rate·days_stale)·(1+…) < threshold`), so a
sufficiently-stale memory is **removed** from results, not just down-ranked — even if
highly relevant. The documented intent and the only decay test
(`decay_reorders_stale_memories`, which passes `threshold = 5.0` to avoid the filter)
say decay should **re-rank**. So this is a latent bug, not a feature. **Decision
(owner): do not fix now** — at correct aging (0–30 d) the shipped effect is a *mild
ranking drag with zero recall loss* (baseline nDCG 0.693 → aeon-full 0.627 at smoke;
recall unchanged; the removal never triggered on these queries). Revisit only if
removal manifests at larger staleness/scale.

---

## 5. What this means for AEON-IQ's claims

- **Defensible now:** *Under memory pressure, AMP's adaptive eviction preserves
  substantially more task-relevant memory than LRU or random* (~1.7–2.2× at smoke
  scale, consistent across agents). This is the headline worth pursuing.
- **Not supported (yet):** any claim that decay / importance / graph / RMK improves
  retrieval *ranking*. At n = 3 they are within noise, and in the pressure phase
  decay/importance mildly *reduce* gold survival.
- **Do not claim:** AMP "retains 100 % of gold" (artifact), PI-controller
  "convergence" (didn't), or any longitudinal round-over-round trend (LongMemEval
  has one question per record, so only round 1 is scored — see §6).

---

## 6. Caveats — what was and wasn't exercised

- **n = 3 conversations.** Rounds metrics are underpowered; treat §3.1 as noise. The
  pressure result (§3.2) is more robust only because it holds on every agent with a
  large margin — still n = 3, still smoke.
- **Truncated haystack** (A2): ~139 turns/agent, not the full ~492. Relative
  comparisons hold; absolute difficulty is easier than a full run.
- **Background workers off** (`MEMORYOS_ROLE=proxy`, A5): **RMK policy learning and
  co-access edge decay did not run.** `aeon-full`/`aeon-full-importance` here reflect
  the retrieval-time RMK adapter with a *default* policy — not a learned one. The RMK
  mechanism is essentially untested for lift.
- **One question per record:** rounds advance the simulated clock but only round 1
  scores a query per agent, so the intended round-over-round *trend* is not measured;
  what §3.1 measures is retrieval over a 0–30-day-aged corpus, scored once.
- **PI controller did not converge** (§3.2).
- **Decay = "configured" (0.03), not shipped** (0.0). Any write-up must say so.

---

## 7. Recommended next steps (for the full/publication run)

1. **Scale up** to ≥ 30–50 conversations for statistical power on the rounds phase;
   the pressure phase especially deserves n large enough for a confidence interval on
   the AMP vs LRU/random gap.
2. **Use untruncated `longmemeval_s`** (drop A2), budget the wall-clock accordingly.
3. **Exercise RMK properly:** run with background workers (`MEMORYOS_ROLE=all`) but
   serialize the forced sweeps against the background job (or add a bench-only lock)
   to avoid the deadlock that forced A5.
4. **Decide the kernel decay-filter question** (§4.5) with real aged-corpus numbers
   at scale before publishing any decay claim.
5. Lead the write-up with the **AMP-under-pressure** result; report the rounds phase
   as "no measurable ranking effect at this scale," honestly.

---

*Generated from `benchmarks/scripts/run_longitudinal.py` runs on 2026-07-04.
Raw per-condition `longitudinal_quality.json` artifacts were produced with real
OpenAI embeddings; re-run with the committed SQL + seed 42 to reproduce the aging.*
