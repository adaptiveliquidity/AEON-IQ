# Longitudinal Memory Benchmark — Results (smoke-scale + scaled N=40, 2026-07-04)

> **▶ Scaled publication run (N=40, seed 42, untruncated `longmemeval_s`, RMK ON)
> completed 2026-07-04 — see [§8](#8-scaled-publication-run-n40-2026-07-04).**
> Headline confirmed at scale: **AMP retains ~0.88–0.91 of gold under pressure vs
> ~0.27–0.33 for LRU/random (~2.7–3.0×)**. The stress test also **confirmed the
> §4.5 decay-filter bug is severe** (configured decay collapses recall as memory
> ages: r1 ~0.95 → r5 ~0.13). Those decay numbers are the **pre-fix** product; a
> standalone §4.5 fix + re-run of the three decay conditions is in progress, after
> which §8 will carry the corrected curve. §§1–7 below are the earlier smoke record.

**Status: smoke-scale (n = 3) in §§1–7; scaled N=40 in §8.** The smoke numbers are a
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
differentiators are actually exercised. At smoke scale we find:

- **(a) Ranking layer — provisional, n = 3, noisy:** over an aged corpus, decay /
  importance / graph show **no *measurable* lift** at this scale, but the
  differences are single-query noise — a **provisional observation, not a
  conclusion.**
- **(b) RMK — NOT TESTED:** RMK's learning loop was **off** for this run (background
  workers disabled under `MEMORYOS_ROLE=proxy`, see §6/A5). RMK is an **open
  question**, *not* a negative finding. Do not read "no RMK lift" anywhere in this
  doc.
- **(c) Pressure layer — the one robust result (still smoke-scale):** under memory
  pressure, **AMP eviction retains ~81–95 % of gold vs ~38–50 % for LRU and ~39–47 %
  for random** — a consistent ~2× advantage across **every** agent. Robust in the
  sense that it holds on all agents with a large margin, but still n = 3 and
  **pending the publication-scale run.**

Getting to these honest numbers required fixing four harness bugs and one metric
artifact (§4), one of which had made AMP look like a *perfect* 100 %.

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
| A5 | Kernel run with `MEMORYOS_ROLE=proxy` during the **smoke** run | Serve the forced-sweep endpoint but disable background workers, so forced sweeps are deterministic and the background pressure-sweep job doesn't deadlock against the harness's SQL. **Consequence:** background RMK policy-learning and co-access decay are **not** exercised (see §6). **SUPERSEDED for the scaled run by A9.** | full run |
| A6 | P0 seeding parallelized (16 threads, env `LONGITUDINAL_SEED_CONCURRENCY`) | Serial seeding is ~3/s (synchronous embedding round-trips) → ~10 h for the 5-condition untruncated run. **Perf-only:** creates are order-independent and aging is applied post-seed by memory id (gold-blind), so seed order cannot affect stored state. Integrity-verified: serial == concurrent, byte-identical stored state (mapping keys, per-turn content, importance); 1:1 (100/100). | scaled-run prep |
| A7 | Importance **SQL-injected** post-seed for the importance variant: **gold = 0.95, non-gold = 0.5** (fixed, pre-registered before the run) | The create API silently drops `importance` (`CreateMemoryBody` = {content, memory_type}) and hardcodes `importance_score = 1.0`, so the importance-weighting term is inert. Exactly as the **timeline** is SQL-injected to test decay (the API has no timestamp field either), importance is SQL-injected to test the weighting math. Non-gold **must** be < gold (retrieval penalty = 1 − score). See §4.6. | scaled-run prep |
| A8 | Dedup **disabled** during seeding (`DEDUP_THRESHOLD=0`) | Shipped default `0.05` skips an insert within cosine-distance 0.05 of an existing live memory and returns the **existing** id, silently collapsing near-duplicate turns (N turns → < N memories) and potentially merging a gold turn away. Off ⇒ faithful **1 turn = 1 distinct retrievable memory**. Analogous to the existing `ARCHIVAL_INTERVAL_HOURS=0`. Verified 1:1 (100/100). The smoke run did **not** disable it — see §4.7. | scaled-run prep |
| A9 | Scaled run uses `MEMORYOS_ROLE=all` with **RMK ON** (reverses A5) | The smoke's A5 proxy-role dodge disabled RMK. The deadlock is now fixed properly (per-agent advisory lock serializing the AMP sweep vs harness SQL; §4.8), so the scaled run exercises RMK's learning loop for real. Probe: 112 forced sweeps + 110 aging txns + a real background sweep → zero new deadlocks. | scaled-run prep |
| A10 | **Rounds are a repeated measure**: re-score the SAME unit(s) every round after each clock advance (was: partition an agent's units across rounds) | With ~1 question per LongMemEval record the partition design leaves rounds 2..R **empty at any N** (not a truncation artifact) → no aging curve. Re-scoring the same question at seed-age + (r−1)·6 d yields a genuine **recall-vs-age curve**. **These are repeated measures on a corpus aging underneath the query, NOT independent per-round samples** — any trend analysis must treat them as such (the same unit recurs each round). | scaled-run prep |

**Scaled run (2026-07-04):** untruncated `longmemeval_s` (drops A2), **N = 40**
conversations, 5 conditions (baseline / decay-only / amp-only / aeon-full /
aeon-full-importance), seed 42, corrected `gold_retention` (A4), **RMK ON** (A9),
**rounds = repeated measure** (A10). Amendments A6–A10 and the §4.6–§4.8 findings
were committed **before** this run (code `2352c3b` + this doc) for the
anti-cherry-pick trail.

Decay stays at the pre-registered **configured** value `MEMORY_DECAY_RATE=0.03`,
`IMPORTANCE_BOOST_FACTOR=0.5` (ships as 0.0 — write-ups must say "configured
decay," not a shipped default). AMP uses shipped defaults (target ≈ 1000 hardcoded;
no tuning PR).

### 2.1 Pre-result expectation: ranking/aging is near-ceiling (logged before results)

Recorded **before** the scaled run completes, from condition 1 (`cosine-baseline`,
the first to finish): at N=40 on untruncated `longmemeval_s`, the **baseline is
already near the recall ceiling** — recall@10 = 1.00, recall@1 = 0.90, recall@5 =
1.00, with a **flat round_trend** (rounds 1–5 identical). Consequences we expect,
stated now so they are not post-hoc excuses:

- **The rounds-ranking comparison will likely show little separation** between
  conditions on recall, because there is almost no recall headroom for decay /
  importance / graph to recover — gold is nearly always already in the top-k.
  This is **dataset difficulty (haystack ≈ 490 turns/agent), not a sample-size
  limitation** — more agents cannot manufacture headroom. MRR/nDCG (baseline
  0.936 / 0.824) retain some room; watch those, not recall@k, for any ranking
  signal.
- **A flat recall-vs-age curve (A10) is the mathematically-expected default**:
  `advance_clock` ages every live memory uniformly, and a uniform age shift scales
  every candidate's decay factor by the *same* constant → ranking is invariant.
  Decay can only move ranking when memories age at *different* rates (e.g. access
  refreshes `last_accessed_at` for retrieved ones) or when the §4.5 filter pushes
  a memory past the distance threshold. So "the aging curve is flat / aeon-full
  tracks baseline" is a **predicted honest outcome**, not a failure to hide.

**Where the real statistical value is:** the **pressure phase (AMP vs LRU vs
random)** — its own SQL-seeded distractor corpus, independent of haystack size,
with 40 per-agent `gold_retention` samples — and the **A10 recall-vs-age curve**
as an honest test of aging even if it lands flat. Lead with AMP-under-pressure;
report ranking/aging/RMK/importance straight, "near-ceiling, little separation"
included where true.

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
condition shows a *measurable* ranking lift or drag over baseline at this scale —
a provisional smoke observation, not a conclusion.** These rows also **do not test
RMK**: its learning loop was off (A5), so its column here is RMK-*absent*, not
RMK-negative.

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

### 4.5 Kernel: decay filters below threshold instead of only re-ranking (CONFIRMED SEVERE at scale; fix in progress)
`src/memory/store.rs` applies decay to the distance that is then compared to the
threshold (`WHERE cosine_dist·exp(rate·days_stale)·(1+…) < threshold`), so a
sufficiently-stale memory is **removed** from results, not just down-ranked — even if
highly relevant. The documented intent and the only decay test
(`decay_reorders_stale_memories`, which passes `threshold = 5.0` to avoid the filter)
say decay should **re-rank**. So this is a bug, not a feature.

**Smoke decision was "do not fix now" on the belief the shipped effect was a *mild
ranking drag with zero recall loss.* The scaled run (A10 repeated-measure aging, §8.2)
overturns that belief.** The smoke only ever scored round 1, so aging never
accumulated and the removal never triggered. At N=40 with the corpus aging under
repeated rounds, the filter fires hard: every condition with configured decay
(`0.03`) collapses from recall@10 ≈ 0.90–0.98 at round 1 to **≈ 0.13–0.15 at round 5**,
while the two decay-off conditions (baseline, amp-only) stay pinned at **1.0** across
all rounds. That `recall@5 == recall@10` in every collapsed row (§8.2) is the
signature of **removal, not demotion** — aged gold crosses the distance ceiling and
is dropped from the candidate set entirely. So the effect is not a mild drag: **as
shipped-configured, decay actively destroys recall as memory ages.**

**Decision (updated 2026-07-04): fix it.** Now warranted by hard evidence, being
implemented as a standalone product change — the threshold gates **raw** cosine
relevance, decay only reorders candidates already inside the ceiling (matching the
`decay_reorders_stale_memories` intent) — on its own commit with a survival test
proving aged gold is retained. The §8.2 decay curve is therefore the **pre-fix**
product; the three decay conditions (decay-only, aeon-full, aeon-full-importance) are
being re-run after the fix, and the corrected curve will replace it in §8.

### 4.6 Kernel: create API silently ignores importance (DEFERRED known-issue)
`CreateMemoryBody` (`src/api.rs`) accepts only `{content, memory_type}`; the
`importance` field the harness sends is dropped by serde, and `create_memory`
hardcodes stored `importance_score = 1.0`. Because the retrieval formula's
importance factor is `(1.0 + importance_boost·(1.0 − importance_score))`, a uniform
`1.0` makes that factor identically `1.0` — **the importance-weighting mechanism is
inert in production via this API, for any `IMPORTANCE_BOOST_FACTOR`.** This also
means the smoke run's "no importance lift" was **structurally untestable**, not
disproven (same status as RMK under A5). **Decision (owner): do not change shipped
behavior mid-benchmark** (same treatment as §4.5). The scaled run tests the
*weighting math* by SQL-injecting importance (A7), exactly as it tests decay by
SQL-injecting the timeline; the paper reports both the math result **and** this
product gap ("importance is inert in production until the API accepts the field").
Surfaced 2026-07-04 by the A6 integrity check.

### 4.7 Kernel: insert-time dedup silently collapsed the corpus (fixed via A8)
`store_memory` (`src/memory/store.rs`) skips an insert when the nearest live memory
is within `DEDUP_THRESHOLD` (cosine distance, **default 0.05**) and returns the
**existing** id. Seeding N similar turns therefore yields **< N** distinct memories
(measured: 100 near-identical turns → 87 rows serial / 78 concurrent), and a gold
turn can be merged into another memory and become unretrievable under its own id.
The smoke run ran with the default `0.05` **on**, so its corpus was subject to this
collapse (an unquantified confound on the smoke numbers). Fixed for the scaled run
by A8 (`DEDUP_THRESHOLD=0`), verified 1:1 (100/100). Surfaced 2026-07-04 while
validating A6.

### 4.8 Kernel: AMP sweep vs harness SQL deadlock (fixed — enables RMK / A9)
The background AMP pressure sweep and the harness's row mutations (aging, forced
sweeps) both `UPDATE memories` and acquired row locks in opposing orders →
Postgres deadlock. The smoke run dodged it by disabling background workers (A5),
which also disabled RMK. Fixed properly: `run_pressure_sweep_for_agent` now holds a
per-agent advisory lock (`pg_advisory_xact_lock(AMP_SWEEP_LOCK_CLASSIFIER,
hashtext(agent_id))`) across its read→decide→write, and the harness takes the same
lock around its mutations — a total lock order, deadlock-free by construction.
Probe (330 s, dedup/RMK on, `role=all`): 112 forced sweeps + 110 aging txns + one
real autonomous background sweep → **zero** new `pg_stat_database.deadlocks`, zero
errors. This is what lets A9 run RMK for real.

---

## 5. What this means for AEON-IQ's claims

- **The one robust result (still smoke-scale):** *Under memory pressure, AMP's
  adaptive eviction preserves substantially more task-relevant memory than LRU or
  random* (~1.7–2.2×, consistent across every agent). This is the headline worth
  pursuing — but it is **still n = 3 and must be confirmed by the publication-scale
  run** before it is stated as a result.
- **Provisional only (n = 3, noisy):** no *measurable* ranking lift from decay /
  importance / graph. This is a **provisional observation, not a conclusion** — at
  n = 3 a single query moves recall@1 by 0.333. In the pressure phase decay /
  importance mildly *reduce* gold survival; that too is provisional at this scale.
- **UNTESTED — RMK:** RMK's learning loop did not run (proxy role, §6/A5). We have
  **no data** on whether RMK helps or hurts. This is an **open question**, *not* a
  negative finding. Do not write "no RMK lift."
- **Do not claim:** AMP "retains 100 % of gold" (artifact), any RMK effect
  (untested), a ranking *conclusion* (provisional, n = 3), PI-controller
  "convergence" (didn't), or any longitudinal round-over-round trend (LongMemEval
  has one question per record, so only round 1 is scored — see §6).

---

## 6. Caveats — what was and wasn't exercised

- **n = 3 conversations.** Rounds metrics are underpowered; treat §3.1 as noise. The
  pressure result (§3.2) is more robust only because it holds on every agent with a
  large margin — still n = 3, still smoke.
- **Truncated haystack** (A2): ~139 turns/agent, not the full ~492. Relative
  comparisons hold; absolute difficulty is easier than a full run.
- **RMK was NOT tested** (`MEMORYOS_ROLE=proxy`, A5): **RMK policy learning and
  co-access edge decay did not run.** `aeon-full`/`aeon-full-importance` here reflect
  the retrieval-time RMK adapter with a *default* (unlearned) policy. We have **zero
  data** on RMK's effect. This is an **open question**, and nothing in this doc may
  be read as a "no RMK lift" / negative RMK finding.
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

## 8. Scaled publication run (N=40, 2026-07-04)

Untruncated `longmemeval_s` (drops A2), **N = 40** conversations, 5 conditions, seed
42, corrected `gold_retention` (A4), **RMK ON** / `MEMORYOS_ROLE=all` (A9), **dedup
off** (A8), importance **SQL-injected** gold=0.95 / non-gold=0.5 for the variant (A7),
**rounds = repeated measure** (A10). ~19,605 memories seeded per condition (40 agents
× ~490 turns). All amendments + the §4.6–§4.8 findings were committed **before** this
run (anti-cherry-pick trail). All 5 conditions completed `status = ok`.

**This section reports the run straight, exactly as it landed. Nothing was tuned. The
decay result below is a *negative* finding for the mechanism as shipped-configured —
the opposite of a flattering number — which is the whole point of the stress test.
The §8.2 decay curve is the PRE-FIX product (see §4.5); it is being re-run after the
decay fix and will be replaced by the corrected curve.**

### 8.1 Memory pressure (eviction) — the robust result, confirmed at scale

Primary metric `gold_retention` (fraction of gold still retrievable after each policy
soft-evicts the same settled K to target ≈ 1000), 40 per-agent samples per condition:

| condition | **AMP** | LRU | random | AMP advantage | AMP post-evict recall@10 / nDCG@10 |
|-----------|:------:|:---:|:------:|:-------------:|:----------------------------------:|
| amp-only | **0.876** | 0.272 | 0.307 | **~3.0×** | 1.000 / 0.851 |
| aeon-full | **0.888** | 0.300 | 0.289 | **~3.0×** | 0.950 / 0.580 |
| aeon-full-importance | **0.911** | 0.334 | 0.328 | **~2.7×** | 0.925 / 0.722 |

**AMP retains ~0.88–0.91 of gold under pressure vs ~0.27–0.33 for LRU/random —
consistent ~2.7–3.0× across every pressure condition.** This is **stronger** than the
smoke (there AMP 0.81–0.95 vs comparators 0.38–0.50 ≈ 2×): at scale the gold-blind
comparators fall to ~0.27–0.33, so AMP's relative advantage *widens*. This is the
headline, now at N=40. It is unaffected by the §4.5 decay bug (pressure is measured on
its own SQL-seeded distractor corpus, not the aging haystack).

### 8.2 Recall-vs-age curve (A10) — decay actively destroys recall (PRE-FIX; §4.5)

recall@10 by round (corpus ages ~6 d per round underneath a re-scored query):

| condition | r1 | r2 | r3 | r4 | r5 |
|-----------|:--:|:--:|:--:|:--:|:--:|
| cosine-baseline (decay 0) | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| amp-only (decay 0) | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| **decay-only** (decay 0.03) | 0.975 | 0.825 | 0.650 | 0.325 | **0.150** |
| **aeon-full** (decay 0.03) | 0.950 | 0.875 | 0.650 | 0.375 | **0.125** |
| **aeon-full-importance** (decay 0.03) | 0.900 | 0.850 | 0.725 | 0.300 | **0.125** |

nDCG@10 tells the same story (decay-only 0.557→0.035; aeon-full 0.580→0.043;
importance 0.714→0.044). Aggregate over all 200 round-queries: baseline recall@10=1.000
nDCG=0.824; amp-only 1.000 / 0.823; decay-only 0.585 / 0.286; aeon-full 0.595 / 0.298;
importance 0.580 / 0.331.

**Every decay-on condition collapses to ~13–15 % recall by round 5; every decay-off
condition holds at 1.0.** In every collapsed row `recall@5 == recall@10` — the gold is
**removed** from the candidate set (§4.5 filter), not merely demoted. This is the
§4.5 bug manifesting at realistic accumulated aging, and it **overturns the smoke's
"mild drag, zero recall loss"** read (the smoke only scored round 1, so aging never
accumulated). Honest verdict: **shipped-configured decay is harmful, not neutral,
under aging.** Fix + re-run in progress (§4.5).

### 8.3 Importance (A7) — a small but real ranking gain

With importance actually varying (gold 0.95 / non-gold 0.5, SQL-injected per A7 since
the create API drops the field — §4.6), the importance variant pins
`mean_first_gold_rank = 1.000` in **every** round (gold is always rank-1 when present,
vs aeon-full's 1.13–1.29), and shows higher pressure-phase nDCG (**0.722 vs aeon-full
0.580**) and retention (0.911 vs 0.888). So the **weighting math does help ranking
quality when scores differ** — a modest, genuine positive. Recall still collapses
across rounds with the others because decay filters the same aged gold out regardless
of its importance. The §4.6 product gap stands: importance is inert in production until
the create API accepts the field.

### 8.4 RMK — exercised for real, no measurable lift (likely under-exercised)

RMK genuinely ran this time (`MEMORYOS_ROLE=all`, background workers on, advisory-lock
fix from §4.8 held — zero deadlocks), so this is a real "ran, no measurable lift,"
**not** the smoke's "untested." Evidence: aeon-full (RMK on) AMP retention **0.888** ≈
amp-only (RMK off) **0.876** — within noise; and aeon-full's rounds are dominated by
the decay collapse, not RMK. **Caveat (honest, not spin):** RMK's policy-update cooldown
is ~1 h, but each condition's harness wall-clock was tens of minutes (baseline 16 min),
so its learning loop had time for at most ~1 update cycle — plausibly too few episodes
to show learning. Verdict: **no measurable RMK benefit here, likely under-exercised;** a
longer-runtime RMK re-run is under consideration to turn this into a firm verdict.

### 8.5 Run health

| condition | seeded | errors | error_rate | latency p50 / mean / p99 (ms) |
|-----------|:------:|:------:|:----------:|:-----------------------------:|
| cosine-baseline | 19,605 | 2 / 30,727 | 0.0001 | 271 / 363 / 2249 |
| decay-only | 19,605 | 2 / 30,307 | 0.0001 | 260 / 313 / 1231 |
| amp-only | 19,605 | 2 / 31,647 | 0.0001 | 270 / 420 / 2390 |
| aeon-full | 19,605 | 2 / 31,225 | 0.0001 | 270 / 401 / 3300 |
| aeon-full-importance | 19,605 | 2 / 30,731 | 0.0001 | 262 / 350 / 1571 |

Negligible error rate (2 per ~30 k requests), healthy latency. Seeded corpus identical
across conditions (same seed 42).

### 8.6 What this changes

1. **Headline holds and strengthens:** AMP ~2.7–3.0× under pressure at N=40 (§8.1).
2. **§4.5 decay bug promoted latent → confirmed-severe and slated for a fix** (§4.5),
   with the three decay conditions re-running post-fix; §8.2 is the pre-fix baseline
   the corrected curve will be measured against.
3. **Importance:** small real ranking gain when scores vary (§8.3); production gap
   (§4.6) unchanged.
4. **RMK:** ran for real, no measurable lift, likely under-exercised (§8.4); optional
   longer re-run pending.

---

*Generated from `benchmarks/scripts/run_longitudinal.py` runs on 2026-07-04 (smoke §§1–7)
and `run_pubscale.py` (scaled §8). Raw per-condition `longitudinal_quality.json`
artifacts were produced with real OpenAI embeddings; re-run with the committed SQL +
seed 42 to reproduce the aging.*
