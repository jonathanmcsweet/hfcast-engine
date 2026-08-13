
## 2025-06

26396 samples from 15 stations: AL945 AT138 DB049 EB040 EG931 FF051 GR13L HE13N JI91J JR055 LM42B MHJ45 PA836 PQ052 SO148

### foF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.743 |  0.914 |  1.394 | 10126 |
| irtam                    |  -0.011 |  0.364 |  0.732 | 10111 |
| climatology, quiet       |  +0.625 |  0.789 |  1.249 |  7429 |
| irtam, quiet             |  -0.039 |  0.331 |  0.646 |  7429 |
| climatology, storm       |  +1.215 |  1.362 |  1.735 |  2682 |
| irtam, storm             |  +0.090 |  0.498 |  0.928 |  2682 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.745, 10126 day pairs

### hmF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              | +61.495 | 62.178 | 76.569 | 10126 |
| irtam                    |  +3.494 | 14.940 | 30.511 | 10111 |
| climatology+dudeney      | +42.190 | 43.459 | 56.496 | 10126 |
| climatology, quiet       | +63.247 | 63.667 | 74.553 |  7429 |
| irtam, quiet             |  +3.399 | 14.043 | 27.922 |  7429 |
| climatology, storm       | +55.400 | 57.538 | 82.018 |  2682 |
| irtam, storm             |  +3.790 | 18.638 | 36.743 |  2682 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.533, 10126 day pairs

### MUFD (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |       - |      - |      - |     0 |
| irtam                    |       - |      - |      - |     0 |
| climatology, quiet       |       - |      - |      - |     0 |
| irtam, quiet             |       - |      - |      - |     0 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, 0 day pairs

### foE (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.201 |  0.227 |  0.367 |  6144 |
| irtam                    |  +0.201 |  0.228 |  0.367 |  6139 |
| climatology, quiet       |  +0.202 |  0.228 |  0.366 |  4601 |
| irtam, quiet             |  +0.202 |  0.228 |  0.366 |  4601 |
| climatology, storm       |  +0.197 |  0.227 |  0.371 |  1538 |
| irtam, storm             |  +0.197 |  0.227 |  0.371 |  1538 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, 6139 day pairs

### NVIS MUF(d) from foF2 x secant (n = 10126)

| range | model        |    bias |    MAE |    RMS | band calls right |
| ----: | ------------ | ------: | -----: | -----: | ---------------: |
|    0k | climatology  |  +0.743 |  0.914 |  1.394 |            86.6% |
|    0k | clim+dudeney |  +0.743 |  0.914 |  1.394 |            86.6% |
|    0k | irtam-foF2   |  -0.011 |  0.364 |  0.732 |            94.4% |
|    0k | irtam-both   |  -0.011 |  0.364 |  0.732 |            94.4% |
|  300k | climatology  |  +0.539 |  0.842 |  1.385 |            88.0% |
|  300k | clim+dudeney |  +0.613 |  0.870 |  1.411 |            87.9% |
|  300k | irtam-foF2   |  -0.292 |  0.467 |  0.885 |            93.2% |
|  300k | irtam-both   |  -0.041 |  0.407 |  0.814 |            93.7% |
|  600k | climatology  |  +0.149 |  0.960 |  1.657 |            89.8% |
|  600k | clim+dudeney |  +0.374 |  0.971 |  1.652 |            89.6% |
|  600k | irtam-foF2   |  -0.842 |  0.933 |  1.492 |            91.4% |
|  600k | irtam-both   |  -0.097 |  0.541 |  1.082 |            94.2% |
