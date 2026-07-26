# VOACAP evaluation-order sensitivity

Reference build `O2`. 96 sweep cases, 463104 numeric cells parsed from the reference, measured in 7.6s.

Every variant is the same Fortran source compiled with different optimisation flags, so any difference below is the model's sensitivity to floating-point evaluation order, not to physics.

All variants completed every case.

## `O2` vs `O0`

Compared over 96 cases.

| field  | samples | differing |    % | max abs | p95 abs | p99 abs | max rel | only in one |
| ------ | ------: | --------: | ---: | ------: | ------: | ------: | ------: | ----------: |
| DBU    |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| DELAY  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| LOSS   |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| MPROB  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| MUF    |    2304 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| MUFday |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| N DBW  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| REL    |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| RGAIN  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| RPWRG  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| S DBW  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| S PRB  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SIG LW |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SIG UP |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNR    |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNR LW |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNR UP |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNRxx  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| TANGLE |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| TGAIN  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| V HITE |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |

MODE: 23040 compared, 0 mismatched (0.00%), 0 present in only one listing.

## `O2` vs `O1`

Compared over 96 cases.

| field  | samples | differing |    % | max abs | p95 abs | p99 abs | max rel | only in one |
| ------ | ------: | --------: | ---: | ------: | ------: | ------: | ------: | ----------: |
| DBU    |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| DELAY  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| LOSS   |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| MPROB  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| MUF    |    2304 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| MUFday |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| N DBW  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| REL    |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| RGAIN  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| RPWRG  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| S DBW  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| S PRB  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SIG LW |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SIG UP |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNR    |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNR LW |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNR UP |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNRxx  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| TANGLE |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| TGAIN  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| V HITE |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |

MODE: 23040 compared, 0 mismatched (0.00%), 0 present in only one listing.

## `O2` vs `O3`

Compared over 96 cases.

| field  | samples | differing |    % | max abs | p95 abs | p99 abs | max rel | only in one |
| ------ | ------: | --------: | ---: | ------: | ------: | ------: | ------: | ----------: |
| DBU    |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| DELAY  |   23040 |         1 | 0.00 |  0.1000 |       0 |       0 |  0.0033 |           0 |
| LOSS   |   23040 |         1 | 0.00 |       1 |       0 |       0 |  0.0016 |           0 |
| MPROB  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| MUF    |    2304 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| MUFday |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| N DBW  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| REL    |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| RGAIN  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| RPWRG  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| S DBW  |   23040 |         1 | 0.00 |       1 |       0 |       0 |  0.0016 |           0 |
| S PRB  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SIG LW |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SIG UP |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNR    |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNR LW |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNR UP |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| SNRxx  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| TANGLE |   23040 |         2 | 0.01 |  0.1000 |       0 |       0 |  0.0076 |           0 |
| TGAIN  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| V HITE |   23040 |         1 | 0.00 |       1 |       0 |       0 |  0.0022 |           0 |

MODE: 23040 compared, 0 mismatched (0.00%), 0 present in only one listing.

## `O2` vs `fastmath`

Compared over 96 cases.

| field  | samples | differing |    % | max abs | p95 abs | p99 abs | max rel | only in one |
| ------ | ------: | --------: | ---: | ------: | ------: | ------: | ------: | ----------: |
| DBU    |   23040 |         6 | 0.03 |       1 |       0 |       0 |  0.3333 |           0 |
| DELAY  |   23040 |         2 | 0.01 |  0.1000 |       0 |       0 |  0.0036 |           0 |
| LOSS   |   23040 |         9 | 0.04 |       1 |       0 |       0 |  0.0062 |           0 |
| MPROB  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| MUF    |    2304 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| MUFday |   23040 |         2 | 0.01 |  0.0100 |       0 |       0 |  0.0118 |           0 |
| N DBW  |   23040 |         1 | 0.00 |       1 |       0 |       0 |  0.0065 |           0 |
| REL    |   23040 |         4 | 0.02 |  0.0100 |       0 |       0 |  0.0357 |           0 |
| RGAIN  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| RPWRG  |   23040 |         9 | 0.04 |       1 |       0 |       0 |  0.0714 |           0 |
| S DBW  |   23040 |         9 | 0.04 |       1 |       0 |       0 |  0.0071 |           0 |
| S PRB  |   23040 |         3 | 0.01 |  0.0100 |       0 |       0 |  0.0556 |           0 |
| SIG LW |   23040 |         9 | 0.04 |  0.5000 |       0 |       0 |  0.0658 |           0 |
| SIG UP |   23040 |        18 | 0.08 |  0.7000 |       0 |       0 |  0.3182 |           0 |
| SNR    |   23040 |         7 | 0.03 |       1 |       0 |       0 |       1 |           0 |
| SNR LW |   23040 |        10 | 0.04 |  0.3000 |       0 |       0 |  0.0244 |           0 |
| SNR UP |   23040 |        10 | 0.04 |  0.2000 |       0 |       0 |  0.0313 |           0 |
| SNRxx  |   23040 |         9 | 0.04 |       1 |       0 |       0 |  0.0400 |           0 |
| TANGLE |   23040 |         7 | 0.03 |  0.2000 |       0 |       0 |  0.0133 |           0 |
| TGAIN  |   23040 |         0 | 0.00 |       0 |       0 |       0 |       0 |           0 |
| V HITE |   23040 |        41 | 0.18 |       4 |       0 |       0 |  0.0081 |           0 |

MODE: 23040 compared, 0 mismatched (0.00%), 0 present in only one listing.

## Derived tolerance

Widest disagreement between IEEE-conformant builds (O0, O1, O3). A port that stays inside these bounds is no further from the reference than the reference is from itself under a different optimisation level.

| field  | observed max abs | structural disagreements |
| ------ | ---------------: | -----------------------: |
| DBU    |                0 |                        0 |
| DELAY  |           0.1000 |                        0 |
| LOSS   |                1 |                        0 |
| MPROB  |                0 |                        0 |
| MUF    |                0 |                        0 |
| MUFday |                0 |                        0 |
| N DBW  |                0 |                        0 |
| REL    |                0 |                        0 |
| RGAIN  |                0 |                        0 |
| RPWRG  |                0 |                        0 |
| S DBW  |                1 |                        0 |
| S PRB  |                0 |                        0 |
| SIG LW |                0 |                        0 |
| SIG UP |                0 |                        0 |
| SNR    |                0 |                        0 |
| SNR LW |                0 |                        0 |
| SNR UP |                0 |                        0 |
| SNRxx  |                0 |                        0 |
| TANGLE |           0.1000 |                        0 |
| TGAIN  |                0 |                        0 |
| V HITE |                1 |                        0 |

## Path regimes

- `short-eu` — very short, single hop
- `med-eu` — medium mid-latitude, the vendor test circuit
- `long-ew` — long east-west, wide local-time spread, multi-hop
- `long-ns` — long north-south crossing the equator
- `polar` — trans-polar, auroral absorption
- `equatorial` — equatorial, near the anomaly crests
- `antipodal` — near-antipodal, the longest path the model handles
- `south-am` — long north-south in the western hemisphere
