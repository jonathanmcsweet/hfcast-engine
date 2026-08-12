# Notes for the next engine

A findings and decisions record from the 2026-08-12 exploration of both
repositories, made before work started on an improved, non-parity
prediction pipeline. It exists so the reasons behind the decisions stay
readable after the sessions that made them are gone. Open work is
tracked by the maintainer outside this repository; this document records
findings and decisions, not progress.

## The goal

An engine that is accurate and granular: accurate on every path-length
class, and granular in space, in time, and from day to day. Today the
port is monthly climatology — within a month, only the hour changes the
answer. NVIS (paths under about 650 km) is a required class and gets its
own validation metrics, but it is one beneficiary of the goal, not the
whole goal. Performance is part of the goal: large area grids must get
much cheaper, with GPU offload where a device has one.

## Decisions taken (user, 2026-08-12)

| Decision | Answer |
| --- | --- |
| Where the improved engine lives | A new pipeline in this repository: a new module tree and entry point. The parity engine is not changed. The new pipeline's contract is measured accuracy against stored data, not Fortran parity. |
| GPU technology | wgpu, in a sibling workspace crate, so the core crate keeps zero dependencies. wgpu runs on Linux (Vulkan; lavapipe for machines with no GPU), on Android (Vulkan), and on Metal if an iOS build ever exists. A CPU path always remains. |
| Datasets to store | GIRO daily ionosonde characteristics, archived IRTAM coefficient maps, Kp/F10.7/SSN indices, WSPR aggregates, RBN history. PSKReporter is collected forward (see below). |
| First milestone | Data and the validation harness first. Every later change is judged by that ruler. |
| PSKReporter collection | A scheduled GitHub Actions collector in a separate small data repository, reading the public MQTT feed and storing path-hour aggregates. |

## Why the parity engine cannot become more accurate

This crate's identity is byte-identical agreement with the Fortran
reference: `portcheck` (23,040 cells), `fuzz` (149,584 listing lines),
`areacheck` (17,791 cells). Any accuracy improvement moves numbers, so
it cannot live behind flags here — `src/voacap/model.rs` states the same
rule for pervasive changes. The improved pipeline therefore stands
beside the port, and the port remains one oracle for shared code.

## Prior measurements that bound this work

These verdicts are already in this repository and stay load-bearing:

- `docs/daily.md` — day-to-day structure in WSPR residuals is real
  (lag-1 correlation +0.340) but small: a perfect next-day model would
  recover about 0.08 dB on mid and long paths. Days after Kp ≥ 5 carry
  much more structure (+0.424; +0.726 in 2022-09).
- `docs/irtam.md` — per-day IRTAM foF2 assimilation was built and
  measured end to end (41,000 runs, 8 months): day-to-day correlation
  near +0.1, overall error better in one month of eight, clearly better
  in the two storm months. Recorded decision: do not build real-time
  IRTAM plumbing. Named openings: a storm-time mode, hmF2 as well as
  foF2, and ground truth better than WSPR.
- `docs/engines.md` — VOACAP and ITU-R P.533 disagree systematically;
  neither is the truth; measured reception data decides.

Why those verdicts do not close this program: both used mid and long
WSPR paths, scored on SNR. Short paths were not represented, and the
NVIS question is a band-edge question (foF2 and MUF against the
operating frequency), not an SNR question. Day-to-day foF2 variability
(±20–30%) moves the NVIS usable window directly. The missing instrument
is ionosonde-grade ground truth — which is the first milestone.

## Data sources (researched 2026-08-12)

| Source | What | Access | Notes |
| --- | --- | --- | --- |
| GIRO DIDBase | Scaled ionosonde characteristics (foF2, hmF2, foE, MUFD, more), about 70 stations, 15-minute cadence, decades of archive | Bulk: `https://lgdc.uml.edu/common/DIDBGetValues?ursiCode=<code>&charName=foF2,hmF2,foE,MUFD&DMUF=3000&fromDate=YYYY.MM.DD+00:00:00&toDate=...`. Live: FastChar `getbest` (the app already uses it). | Research network. Follow the Rules of the Road: identify the client, space requests, give attribution. Ground truth for foF2/MUF, including the NVIS class. |
| IRTAM / GAMBIT | Assimilated global foF2/hmF2/B0/B1 maps as Jones-Gallet coefficient sets, every 15 minutes, archive to 2000 | `https://lgdc.uml.edu/rix/gambit-coeffs?time=...&charName=foF2` (also hmF2); `tools/fetch-irtam.sh` | Same mathematical form the engine reads; `src/irtam.rs` converts. hmF2 needs an M(3000)F2 conversion step. |
| GFZ / NOAA SWPC | Kp, ap, F10.7, SSN, daily; 3-day Kp forecast | `tools/fetch-kp.sh` (GFZ file also carries SN and F10.7); SWPC JSON (the app already uses it) | Conditions the storm mode — the strongest measured daily signal. |
| WSPR | Link-level SNR spots since 2008 | wsprnet monthly CSV; wspr.live ClickHouse; `tools/fetch-wspr.sh` | Noisy truth; keeps existing calibration comparable. |
| RBN | CW/RTTY/digital skimmer spots with SNR since 2009 | Full historical daily CSV archive, free, at reversebeacon.net/raw_data | Covers all eight validation months. Different mode and station mix than WSPR. Contest days exceed 300k spots. The project shares analyses with the RBN community in return. |
| PSKReporter | FT8-dominated reception reports, 50–100M spots/day | No historical bulk access. HTTP query API is small and rate-limited. Public MQTT broker `mqtt.pskreporter.info`, topic `pskr/filter/v2/#`, JSON spots. | Forward collection only. SNR uses the 2500 Hz convention (+34 dB to the engine scale). FT8 and receiver-density bias. |
| HamSCI / Madrigal | Standardized aggregates of RBN + WSPR + PSKReporter in the CEDAR Madrigal database; Grape Doppler datasets | Madrigal exports | May supply PSKReporter-derived history without a collector. Check before building on the MQTT feed. |
| IGS IONEX / NOAA GloTEC | Global TEC maps, daily archive since 1998 | CDDIS; NCEI | Candidate later assimilation input. Not first. |

## The suggested improvement avenues, against what exists

1. Better data instead of climatology — half built here already
   (`src/irtam.rs`, the overlay root in `src/voacap/data.rs`, the
   EFVAR/ESVAR/EDP injection cards). The open, promising parts are
   short paths, storm time, hmF2, and ionosonde-grade truth.
2. Real ray-tracing — nothing exists; expensive; wait until the
   harness can prove or disprove its value.
3. D-region and auroral absorption — semi-empirical in the port; the
   app ships Kp-driven storm widening. A Kp-conditioned mode in the new
   pipeline targets the one measured strength.
4. Crowdsourced calibration — done and shipped for WSPR (correction
   constants over eight validation months). Extend with RBN and
   PSKReporter; do not reinvent.
5. Statistical residual correction — a linear version is shipped
   (swing, decile rescale, storm widening). Richer conditioning becomes
   possible once daily ground truth is stored.
6. Regional recalibration — not started; a natural extension of 5 on
   the same datasets.

## NVIS notes

- The usable window is bounded above by overhead foF2 times a small
  secant factor (about 1.4 at 800 km), and below by D-region absorption
  and foE screening.
- Day-to-day foF2 variability moves that window directly. This is where
  daily data pays, and where the WSPR verdicts do not apply.
- Engine specifics to examine on short paths: one control point;
  the top of the `ANG` elevation table; `curmuf` hop geometry near
  vertical incidence.
- App specifics: the fine grid cell (1.25° × 1.5°) is coarse against a
  500-mile radius. The patch machinery already has a finer rung; a
  station-centered dense lattice serves this class without a
  whole-world run.

## Performance facts that shape the work

- Measured hot profile of an area run: `genion` 31.7%, `gethp` 29.7%,
  `luffy_freq_loop` 25.0%, `iono.hour` 15.7%. About half the run is a
  pure function of position, time, and solar index — fixed loops, no
  divergence, near-ideal GPU shape. A fine globe is 34,560 points × 12
  frequencies = 414,720 independent work items.
- The engine scales 8.3× on 8 desktop threads (`threadscale`), so there
  is no contention inside it. A Pixel 8 reading (2026-08-10, recorded in
  the app's `engineBudget.ts`) shows strips running in parallel but
  starved: 5.2 cores busy at 8 threads, each strip 4–5× slower than
  alone, 1.5× total scaling. Memory traffic, not thread count, is the
  working hypothesis, and the batch restructure is designed against it.
- The app-side costs after the engine are also measured: 2.2 MB of JSON
  per fine-globe hour across JNI and 34,560 objects on the JavaScript
  heap. A packed float answer removes both.
- The GPU cannot serve the parity engine — the parity contract alone
  forbids it (GPU arithmetic differs by construction). It fits the new
  pipeline, whose contract is a stated accuracy envelope.
- Fleet rule: the legacy Android 5 build can never run GPU compute, so
  a CPU path always remains. Battery and heat are features; the GPU
  becomes a default only where it is measured both faster and no more
  costly in energy per grid.

## Constraints the work keeps

- The parity engine is not changed; `portcheck` runs when any shared
  line or visibility moves, and must show zero disagreements.
- The core crate keeps zero dependencies; GPU code lives in a sibling
  crate; the data collector lives in its own repository.
- Fetched datasets stay out of the repository (`data/` is ignored);
  every fetch script identifies the client and spaces its requests;
  provenance is written beside the data.
- Every change batch runs fmt, clippy, both test lines, and the
  complexity gate, and bumps the version when it changes what the
  package ships.

## Open items recorded here so they are not lost

- The engine repository's own roadmap file (kept outside git) was
  absent from the checkout that produced this record. Its section
  "Beating VOACAP on level" should be folded into the maintainer's
  tracking against this record.
- Confirm the IRTAM hmF2 archive is symmetric with foF2 for all eight
  validation months (2015 depth especially).
- Check whether HamSCI's Madrigal exports carry PSKReporter-derived
  aggregates for the eight validation months.
- One live verification fetch of DIDBGetValues before bulk use.
