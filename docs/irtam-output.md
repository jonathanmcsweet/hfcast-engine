# Real-time foF2 against monthly climatology

Same engine, same configuration, one change: the foF2 coefficient file is replaced per day with the IRTAM map for that day. Scored against per-day WSPR medians.

## 2025-06 (73576 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model       | error |
| ----------- | ----: |
| climatology |  5.00 |
| IRTAM foF2  |  5.00 |

Day-to-day deviations from each path-hour's monthly median.
Climatology predicts zero deviation for every day, so any
positive correlation is information climatology cannot have:

| condition          | day-hours | correlation | slope | predicted size (dB) | observed size (dB) |
| ------------------ | --------: | ----------: | ----: | ------------------: | -----------------: |
| all days           |     73177 |      +0.146 | 0.176 |                1.00 |               2.50 |
| quiet (Kp < 3)     |     13237 |      +0.156 | 0.230 |                0.00 |               2.25 |
| unsettled (3-5)    |     41127 |      +0.141 | 0.189 |                0.00 |               2.50 |
| storm (Kp >= 5)    |     18813 |      +0.127 | 0.126 |                1.00 |               3.25 |
| bands up to 8 MHz  |     25162 |      +0.071 | 0.123 |                0.00 |               2.50 |
| bands 8-15 MHz     |     42591 |      +0.195 | 0.290 |                1.00 |               2.50 |
| bands above 15 MHz |      5424 |      +0.178 | 0.094 |                2.00 |               2.50 |

## 2025-07 (78662 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model       | error |
| ----------- | ----: |
| climatology |  4.50 |
| IRTAM foF2  |  4.00 |

Day-to-day deviations from each path-hour's monthly median.
Climatology predicts zero deviation for every day, so any
positive correlation is information climatology cannot have:

| condition          | day-hours | correlation | slope | predicted size (dB) | observed size (dB) |
| ------------------ | --------: | ----------: | ----: | ------------------: | -----------------: |
| all days           |     78330 |      +0.142 | 0.183 |                0.00 |               2.50 |
| quiet (Kp < 3)     |     24840 |      +0.157 | 0.259 |                0.00 |               2.50 |
| unsettled (3-5)    |     48952 |      +0.132 | 0.158 |                0.00 |               2.50 |
| storm (Kp >= 5)    |      4538 |      +0.150 | 0.157 |                0.00 |               2.50 |
| bands up to 8 MHz  |     27033 |      +0.117 | 0.239 |                0.00 |               2.25 |
| bands 8-15 MHz     |     47077 |      +0.167 | 0.225 |                0.00 |               2.50 |
| bands above 15 MHz |      4220 |      +0.140 | 0.084 |                1.00 |               2.50 |

## 2025-03 (73418 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model       | error |
| ----------- | ----: |
| climatology |  4.00 |
| IRTAM foF2  |  4.00 |

Day-to-day deviations from each path-hour's monthly median.
Climatology predicts zero deviation for every day, so any
positive correlation is information climatology cannot have:

| condition          | day-hours | correlation |  slope | predicted size (dB) | observed size (dB) |
| ------------------ | --------: | ----------: | -----: | ------------------: | -----------------: |
| all days           |     72977 |      +0.066 |  0.064 |                0.00 |               2.00 |
| quiet (Kp < 3)     |     13967 |      +0.051 |  0.067 |                0.00 |               2.00 |
| unsettled (3-5)    |     43209 |      +0.069 |  0.070 |                0.00 |               2.00 |
| storm (Kp >= 5)    |     15801 |      +0.068 |  0.054 |                0.00 |               2.00 |
| bands up to 8 MHz  |     27561 |      +0.201 |  0.344 |                0.00 |               2.00 |
| bands 8-15 MHz     |     38667 |      +0.066 |  0.071 |                0.00 |               2.00 |
| bands above 15 MHz |      6749 |      -0.017 | -0.008 |                0.50 |               2.00 |

## 2024-12 (67791 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model       | error |
| ----------- | ----: |
| climatology |  3.50 |
| IRTAM foF2  |  3.50 |

Day-to-day deviations from each path-hour's monthly median.
Climatology predicts zero deviation for every day, so any
positive correlation is information climatology cannot have:

| condition          | day-hours | correlation | slope | predicted size (dB) | observed size (dB) |
| ------------------ | --------: | ----------: | ----: | ------------------: | -----------------: |
| all days           |     67081 |      +0.098 | 0.100 |                0.00 |               2.00 |
| quiet (Kp < 3)     |     32225 |      +0.078 | 0.078 |                0.00 |               2.00 |
| unsettled (3-5)    |     32110 |      +0.122 | 0.132 |                0.00 |               2.00 |
| storm (Kp >= 5)    |      2746 |      +0.117 | 0.090 |                0.00 |               2.00 |
| bands up to 8 MHz  |     30505 |      +0.222 | 0.489 |                0.00 |               2.00 |
| bands 8-15 MHz     |     32473 |      +0.110 | 0.152 |                0.00 |               2.00 |
| bands above 15 MHz |      4103 |      +0.061 | 0.018 |                0.00 |               2.00 |

## 2022-09 (69804 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model       | error |
| ----------- | ----: |
| climatology |  4.50 |
| IRTAM foF2  |  4.50 |

Day-to-day deviations from each path-hour's monthly median.
Climatology predicts zero deviation for every day, so any
positive correlation is information climatology cannot have:

| condition          | day-hours | correlation | slope | predicted size (dB) | observed size (dB) |
| ------------------ | --------: | ----------: | ----: | ------------------: | -----------------: |
| all days           |     69083 |      +0.110 | 0.165 |                0.00 |               2.00 |
| quiet (Kp < 3)     |     21882 |      +0.100 | 0.175 |                0.00 |               2.00 |
| unsettled (3-5)    |     37269 |      +0.076 | 0.120 |                0.00 |               2.00 |
| storm (Kp >= 5)    |      9932 |      +0.179 | 0.212 |                1.00 |               2.75 |
| bands up to 8 MHz  |     23470 |      +0.132 | 0.296 |                1.00 |               2.25 |
| bands 8-15 MHz     |     45042 |      +0.112 | 0.149 |                0.00 |               2.00 |
| bands above 15 MHz |       571 |      +0.033 | 0.024 |                0.00 |               3.25 |

## 2015-03 (37163 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model       | error |
| ----------- | ----: |
| climatology |  3.50 |
| IRTAM foF2  |  3.50 |

Day-to-day deviations from each path-hour's monthly median.
Climatology predicts zero deviation for every day, so any
positive correlation is information climatology cannot have:

| condition          | day-hours | correlation |  slope | predicted size (dB) | observed size (dB) |
| ------------------ | --------: | ----------: | -----: | ------------------: | -----------------: |
| all days           |     36210 |      +0.135 |  0.185 |                0.00 |               2.00 |
| quiet (Kp < 3)     |      5787 |      +0.054 |  0.108 |                0.00 |               2.00 |
| unsettled (3-5)    |     24187 |      +0.085 |  0.120 |                0.00 |               2.00 |
| storm (Kp >= 5)    |      6236 |      +0.262 |  0.299 |                0.00 |               2.50 |
| bands up to 8 MHz  |      7927 |      +0.073 |  0.191 |                0.00 |               2.00 |
| bands 8-15 MHz     |     26946 |      +0.182 |  0.264 |                0.00 |               2.00 |
| bands above 15 MHz |      1337 |      -0.039 | -0.016 |                1.00 |               2.00 |

## 2019-06 (67226 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model       | error |
| ----------- | ----: |
| climatology |  3.50 |
| IRTAM foF2  |  3.50 |

Day-to-day deviations from each path-hour's monthly median.
Climatology predicts zero deviation for every day, so any
positive correlation is information climatology cannot have:

| condition          | day-hours | correlation | slope | predicted size (dB) | observed size (dB) |
| ------------------ | --------: | ----------: | ----: | ------------------: | -----------------: |
| all days           |     66972 |      +0.061 | 0.179 |                0.00 |               2.50 |
| quiet (Kp < 3)     |     60967 |      +0.062 | 0.193 |                0.00 |               2.50 |
| unsettled (3-5)    |      3345 |      +0.033 | 0.062 |                0.00 |               2.50 |
| storm (Kp >= 5)    |      2660 |      +0.087 | 0.217 |                0.00 |               2.50 |
| bands up to 8 MHz  |     31843 |      +0.077 | 0.202 |                0.00 |               2.50 |
| bands 8-15 MHz     |     34604 |      +0.049 | 0.157 |                0.00 |               2.50 |
| bands above 15 MHz |       525 |      +0.008 | 0.091 |                0.00 |               4.00 |

## 2019-12 (57682 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model       | error |
| ----------- | ----: |
| climatology |  4.00 |
| IRTAM foF2  |  4.00 |

Day-to-day deviations from each path-hour's monthly median.
Climatology predicts zero deviation for every day, so any
positive correlation is information climatology cannot have:

| condition          | day-hours | correlation | slope | predicted size (dB) | observed size (dB) |
| ------------------ | --------: | ----------: | ----: | ------------------: | -----------------: |
| all days           |     56704 |      +0.086 | 0.160 |                0.00 |               2.00 |
| quiet (Kp < 3)     |     52553 |      +0.085 | 0.161 |                0.00 |               2.00 |
| unsettled (3-5)    |      4151 |      +0.094 | 0.143 |                0.00 |               2.00 |
| storm (Kp >= 5)    |         0 |         n/a |   n/a |                0.00 |               0.00 |
| bands up to 8 MHz  |     41057 |      +0.103 | 0.256 |                0.00 |               2.00 |
| bands 8-15 MHz     |     15292 |      +0.075 | 0.095 |                0.00 |               2.00 |
| bands above 15 MHz |       355 |      +0.078 | 0.034 |                1.00 |               2.00 |
