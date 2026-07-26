# Is the reliability number honest?

VOACAP claims a day-to-day spread for every hour: 10% of days fall more than `SNR LW` dB below the hour's monthly median, 10% rise more than `SNR UP` above it. The app's "chance of rain" is computed from those claims, so this checks them against the WSPR record, day by day. All comparisons are deviations from each path-hour's own median, which no unknown antenna can shift.

Fitted on 2025-06: the engine's lower decile is 2.51 times too wide (2122 path-hours), the upper 1.70 times (2239 path-hours). Scale factors below 40% mean the engine overstates how much days differ from each other.

Fitted spread scales: lower 0.399, upper 0.587.

## Tested on 2025-07 (2427 spread records)

Days falling BELOW the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB below  |       38.8% |             24.2% |             24.0% |       2331 |
| 6 dB below  |       28.6% |              9.4% |              9.9% |       2135 |
| 10 dB below |       17.3% |              2.3% |              3.6% |       1676 |
| 15 dB below |        8.2% |              0.4% |              1.5% |       1045 |

Days rising ABOVE the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB above  |       31.2% |             20.6% |             22.8% |       2425 |
| 6 dB above  |       17.0% |              6.2% |              8.1% |       2416 |
| 10 dB above |        6.5% |              1.1% |              2.1% |       2373 |
| 15 dB above |        1.9% |              0.2% |              0.4% |       2217 |

## Tested on 2022-09 (2151 spread records)

Days falling BELOW the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB below  |       38.7% |             24.1% |             22.1% |       2097 |
| 6 dB below  |       28.5% |              9.4% |             10.0% |       1977 |
| 10 dB below |       17.6% |              2.4% |              4.6% |       1582 |
| 15 dB below |        9.2% |              0.4% |              2.1% |        988 |

Days rising ABOVE the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB above  |       30.4% |             19.2% |             19.2% |       2138 |
| 6 dB above  |       15.5% |              4.7% |              5.5% |       2121 |
| 10 dB above |        5.0% |              0.5% |              1.1% |       2035 |
| 15 dB above |        1.0% |              0.1% |              0.2% |       1869 |

## Tested on 2024-12 (2052 spread records)

Days falling BELOW the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB below  |       36.6% |             19.9% |             21.3% |       2023 |
| 6 dB below  |       24.9% |              5.3% |              9.2% |       1940 |
| 10 dB below |       13.2% |              0.8% |              3.9% |       1707 |
| 15 dB below |        5.4% |              0.2% |              1.5% |       1255 |

Days rising ABOVE the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB above  |       30.1% |             19.0% |             17.4% |       2023 |
| 6 dB above  |       15.3% |              4.9% |              4.7% |       2016 |
| 10 dB above |        5.2% |              0.8% |              1.1% |       1945 |
| 15 dB above |        1.3% |              0.2% |              0.3% |       1675 |

## Tested on 2019-12 (1594 spread records)

Days falling BELOW the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB below  |       37.2% |             21.0% |             20.6% |       1559 |
| 6 dB below  |       26.0% |              6.5% |              8.0% |       1382 |
| 10 dB below |       14.8% |              1.2% |              3.0% |       1059 |
| 15 dB below |        6.3% |              0.1% |              1.5% |        571 |

Days rising ABOVE the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB above  |       31.6% |             21.1% |             18.1% |       1580 |
| 6 dB above  |       17.5% |              6.7% |              4.6% |       1571 |
| 10 dB above |        7.1% |              1.4% |              1.0% |       1540 |
| 15 dB above |        2.2% |              0.2% |              0.3% |       1470 |

## Tested on 2015-03 (608 spread records)

Days falling BELOW the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB below  |       39.7% |             26.1% |             20.1% |        604 |
| 6 dB below  |       30.3% |             11.3% |              8.3% |        575 |
| 10 dB below |       20.1% |              3.4% |              3.4% |        492 |
| 15 dB below |       11.4% |              0.6% |              2.2% |        312 |

Days rising ABOVE the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB above  |       31.5% |             20.8% |             16.7% |        597 |
| 6 dB above  |       17.1% |              5.9% |              4.5% |        597 |
| 10 dB above |        6.3% |              0.8% |              1.0% |        568 |
| 15 dB above |        1.4% |              0.1% |              0.2% |        499 |
