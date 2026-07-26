# VOACAP against ITU-R P.533

96 of 96 sweep cases ran on both engines.

These are two different models, so this is disagreement, not error. Neither engine is the truth here, and nothing below says which is more accurate. That question needs measured reception reports.

## Directly comparable

Differences are P.533 minus VOACAP.

| quantity        |    n |  mean | median | 5th pct | 95th pct | max abs | unit |
| --------------- | ---: | ----: | -----: | ------: | -------: | ------: | ---- |
| Basic MUF       | 2304 | -2.69 |  -2.61 |   -6.63 |    +0.52 |   11.71 | MHz  |
| Operational MUF | 2304 | +1.47 |  +1.31 |   -2.11 |    +5.91 |   12.15 | MHz  |

Path distance check: 2304 hours, mean 7370.4 km. Both engines compute this from the same great-circle geometry, so a disagreement here would mean the two runs were not the same circuit.

## A real behavioural difference

Of 20736 hour and frequency combinations, P.533 found no propagating mode at all in 12996 (62.7%). VOACAP named a mode in every one of them. The two engines disagree about how often a band is usable, which matters more to somebody deciding whether to call than any difference of a decibel.

## Indicative only

Both engines were run with isotropic antennas and the same transmit power, but they do not define their signal reference points identically, so treat this as a rough check rather than a measurement. 4974 pairs were left out because at least one engine printed a dead-path sentinel below -250 dBW.

| quantity     |     n |  mean | median | 5th pct | 95th pct | max abs | unit |
| ------------ | ----: | ----: | -----: | ------: | -------: | ------: | ---- |
| Signal power | 15762 | +9.45 |  +5.23 |  -17.81 |   +53.33 |  122.30 | dB   |

## Not comparable

- **Propagation mode.** The two use different vocabularies. VOACAP labels the mode mix (`F2F2`, `EF2`, `F2 E`); P.533 names one dominant mode with a hop count (`1F2`, `2E`) or `NONE`. Matching the labels measures nothing.
- **Signal-to-noise ratio and reliability.** P.533 takes man-made noise as a named environment over a stated bandwidth; VOACAP takes a number at 3 MHz. There is no exact mapping, so any difference would mix the models with the input conversion.
