# Analysis metrics: matching quality, AI calls and cost

A running log of measurements on real sessions, so later optimizations start from numbers. Add a dated section for every new measurement; keep old ones (they are the baseline). Decisions that came out of these numbers are in `docs/DECISIONS.md` (D48–D53).

## How to measure

- **Cost and calls of a run:** every AI call is logged in `<episode>/Logs/ai-calls.jsonl` (task, seconds, input size, and for the Claude Code provider tokens and API-equivalent cost). Summary per task:
  ```
  python3 scripts/ai_costs.py <episode folder> [--since 2026-11-02T10:00]
  ```
  Cached answers are not logged, so the summary counts real calls. To measure from scratch, delete `<episode>/Cache/ai` on a copy first.
- **Call count and request sizes without spending anything:** run the analysis on a copy with the mock provider. It makes the same calls with the same inputs and logs `input_chars`.
  ```
  CUTAWAY_AI_PROVIDER=mock CUTAWAY_NO_AUDIO=1 cutaway -y -p <copy> align
  ```
- **One character or one line only:** `cutaway -y -p <copy> align --character SARA` (or `--line <id>`).
- **Matching quality:** the dev panel's Learn tab scores settings variants against Rob's marks on reviewed scenes (found / wrong / missed). Results are written to `<episode>/Logs/eval-<time>.json`.
- **Always on a copy:** copy the `.cutaway` file with `sqlite3 <file> ".backup <copy>"`, copy `Cache/`, `Script/` and `Library/`, and symlink `Audio/`. Source audio is only read.
- **Cost figures** are what the CLI reports as `total_cost_usd`: API-equivalent. Rob's Claude Code runs on a subscription, so these count against the plan's usage limits rather than being billed.

## Test material: TARMAC Ep2 (Rob's session, 2026-10-31)

- 7 actors, each recorded separately: one session-mic file and one director-mic file per actor (14 WAVs, 48 kHz / 24-bit, dual mono), 5 min to 50 min each. 16 scenes, 108 dialogue lines.
- Output device during listening tests: Arturia Audiofuse Studio at 48 kHz.

## 2026-11-01/02 baseline and first optimizations

### Speech recognition (Parakeet TDT 0.6B v3 int8, VAD chunks)

| Metric | Value |
|---|---|
| Segments over all 14 files | 5,108 |
| Median words per segment | 2 |
| Segments with ≤ 2 words | 51 % |

Parakeet heard nearly every real take. An independent fuzzy search of the transcript found 417 occurrences of script lines on the actor mics. Misses were mostly in the matching after it. Fragmentation is the main noise: it breaks the short-line rule and gives the AI choppy context. Hence sentence merging (pause ≤ 0.7 s, D50).

### Matching

| Run | Takes | Notes |
|---|---|---|
| First app run (bleed cut at −38 dB) | 328 | 47 own-mic reads dropped as "bleed" (Sara 24, Colonel 17, Norbert 5); Nina 0 |
| Bleed relative to the mic (D48) | 372 | Sara 42 → 66 takes (17 → 20 lines), Colonel 63 → 77 (29 → 31), Norbert 60 → 65, Nina 0 → 1; bleed drops 4 |
| Plus rescue with the mock provider | 396 | Rules-only rescue (word overlap ≥ 0.5) |
| Sara only, Claude rescue | 72 (66 matcher + 6 rescued) | 5 clear rescues ("Kitty mean the time" → "Get him to the spring!"; 4 × "Paul!" heard as "How?"/"Hello!"), 1 doubtful ("I heard turkey, I'm good." → "That's all I needed, thank you.") |

Still missed: very short lines ("What?", "Ha!", "Paul!", "Mh-hm"). The transcript audit cannot tell those apart from chatter. Needs Rob's review in the Learn tab.

### Playback

The mixer found samples by converting time back into a sample index with `floor(t × rate)`. For clips starting on a whole sample, up to ~60 % of samples were repeated or skipped (simulation, clip at 100.5 s: 286,719 of 480,000 steps wrong). That was audible as grit, like a low-bitrate codec. Fixed in `328c3a3`, where a test checks every output sample against the source.

### AI calls per full analysis (68 calls)

| Task | Calls | Avg request (chars) | Max | Avg answer (chars) | Avg seconds |
|---|---|---|---|---|---|
| take_rescue (one per actor × scene) | 27 | 11,799 | 32,187 | 1,140 | 11 |
| room_classify, before the session fix | 11 | 196,325 | 341,647 | 21,089 | 87 (5 of 11 failed) |
| room_classify, after | 11 | 30,108 | 83,727 | — | ~17 |
| pick_detect | 11 | 8,438 | 22,663 | 1,661 | 9 |
| note_condense | 11 | 11,473 | 30,696 | 2,809 | 11 |
| line_match (unclear matches) | 8 | 4,130 | 7,246 | 218 | 5 |
| **Total request size** | | **2.73 MB → 0.90 MB** | | | |

- Request sizes come from the mock count on a copy.
- Answer sizes and seconds come from real Claude runs before the lean calls.
- Room classification was sending director speech from all seven sessions. Every session clock starts at zero, so a time window catches everyone (fixed, D53).

### Claude Code CLI overhead

| Call | Cost (API-equivalent) | Notes |
|---|---|---|
| Default `claude -p`, one-line answer | $0.044–0.053 | ~34k tokens of Claude Code's own system prompt and tools |
| Default `claude -p`, 15 kB synthetic input | $0.149 | several turns: the model used tools |
| Lean flags (D53), same 15 kB input, effort low | $0.043 | 1 turn, 8,037 input tokens, 1,030 output |
| Lean flags, same input again, effort medium | $0.018 | the input came from the cache |

- **Rates fitted to the two lean calls:** input ≈ $2 per million tokens, first send (1-hour cache write) ≈ $4/M, cache read ≈ $0.20/M, output ≈ $10/M. Both calls fit within 1 %.
- **Fixed overhead left after the lean flags:** about 4,500 input tokens per call (Sara run: 181k input tokens for 238 kB of our own text, ≈ 68k tokens). So even a tiny call costs about 2 cents.

### Measured: Sara re-run with lean calls (2026-11-02)

| Task | Calls | Seconds | Input kB | Tokens in | Tokens out | Cost $ |
|---|---|---|---|---|---|---|
| take_rescue | 6 | 77 | 87 | 58,940 | 8,618 | 0.327 |
| room_classify | 6 | 101 | 91 | 59,099 | 14,095 | 0.383 |
| note_condense | 6 | 51 | 31 | 30,234 | 4,153 | 0.165 |
| pick_detect | 6 | 51 | 23 | 27,450 | 2,562 | 0.138 |
| line_match | 1 | 5 | 7 | 5,761 | 89 | 0.024 |
| **Total** | **25** | **286** | **238** | **181,484** | **29,517** | **1.037** |

No failures; same result as with the default flags (6 rescued, 72 takes). The comparable earlier Sara run with default flags took 27 calls and 379 s of Claude time, with 4 notes rejected for length. Its cost was not logged then.

### Per full episode (extrapolated from the measured per-call costs)

| Setup | Cost | Claude time |
|---|---|---|
| First app run (default flags, room bug) | ≈ $7 | ~42 min, room calls timing out |
| Now (lean calls, room per session) | ≈ $2.90 | ~13 min, one call after another |
| One character again | ≈ $1 | ~5 min (~25 calls) |
| One mark sent | ≈ $0.03–0.06 | ~10 s |
| Learn tab, try settings, first time | ≈ $2–3 | — |

## Optimization backlog

Each item has the expected effect and how to check it. When one is done, move its numbers into a dated section above.

| # | Idea | Expected effect | How to check |
|---|---|---|---|
| 1 | Run calls in parallel (3–4 scenes at once) | ~13 min → ~4 min; cost unchanged | Wall-clock time of a full run |
| 2 | One plain-text call per scene for room classification, picks and notes (same director speech, read once) | 33 calls → 11; saves ~22 × the ~2 ¢ fixed overhead plus repeated input: about −$1 per episode | `ai_costs.py` before and after; picks and notes compared in the Learn tab |
| 3 | Assign each sentence to one scene (neighbouring scene stretches overlap today) | Fewer rescue tokens, no sentence judged twice | Rescue input kB, count of duplicate ids across calls |
| 4 | Turn off prompt caching for these one-off requests (every request is written to the 1-hour cache at 2× the input price and never read back) | Input cost roughly halved | Cost per call; must still work with the subscription login |
| 5 | Smaller or faster model for picks and notes | Cheaper and faster where quality holds | Learn tab variant with that model |
| 6 | Find where the remaining ~4.5k fixed tokens per call come from | Up to ~2 ¢ per call (~$1.40 per episode) | Tokens in for a minimal call |
| 7 | Bleed: compare each take's level on the session mic against the director mic | Fewer director/reader reads counted as takes, fewer real takes dropped | Learn tab (wrong / missed) on a reviewed scene |
| 8 | Short lines ("What?", "Ha!", "Paul!") | Rescue may be finding them; chatter may slip in | Rob's marks on a reviewed dialogue scene |
| 9 | Changed-wording takes (accepted by design) | Measure how often chatter is taken for a changed read | Learn tab: "wrong" on reviewed scenes |
| 10 | Send several marks of one scene in one call | Fewer calls when marking a lot | Calls per mark |
| 11 | Prompt wording and worked examples (adopted in the Learn tab) | Better rescue answers | Learn tab variant "rescue without your examples" vs current |

## Open measurements

- The whole episode through the new pipeline with Claude, measured: not run yet. Expected ≈ $2.90 and ~13 min.
- A reviewed reference scene and a held-out scene (Rob), so that the Learn tab's found / wrong / missed scores mean something.
- Rescue quality: of the takes Claude rescued, how many are right (needs the review above).
