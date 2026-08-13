
## 2015-03

34559 samples from 21 stations: AL945 AT138 AU930 BC840 DB049 EB040 EG931 FF051 GR13L HE13N JI91J JR055 LM42B MHJ45 MO155 PA836 PQ052 PRJ18 RO041 SAA0K WP937

### foF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  -0.261 |  0.710 |  1.140 | 13884 |
| irtam                    |  -0.046 |  0.342 |  0.694 | 13884 |
| essn (holdout)           |  +0.001 |  0.583 |  1.070 | 13884 |
| essn+storm               |  -0.022 |  0.576 |  1.069 | 13884 |
| climatology, quiet       |  -0.401 |  0.669 |  1.066 | 10878 |
| irtam, quiet             |  -0.064 |  0.327 |  0.666 | 10878 |
| essn, quiet              |  -0.018 |  0.551 |  0.989 | 10878 |
| essn+storm, quiet        |  -0.021 |  0.552 |  0.988 | 10878 |
| climatology, storm       |  +0.421 |  0.888 |  1.376 |  3006 |
| irtam, storm             |  +0.038 |  0.406 |  0.787 |  3006 |
| essn, storm              |  +0.066 |  0.763 |  1.323 |  3006 |
| essn+storm, storm        |  -0.028 |  0.706 |  1.322 |  3006 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.794, essn +0.390, essn+storm +0.395, 13864 day pairs

### hmF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              | +30.964 | 33.250 | 42.621 | 13883 |
| irtam                    |  +0.321 | 10.509 | 21.450 | 13883 |
| climatology+dudeney      | +18.558 | 23.078 | 34.266 | 13883 |
| climatology, quiet       | +31.761 | 33.143 | 41.360 | 10878 |
| irtam, quiet             |  +0.007 |  9.875 | 19.401 | 10878 |
| climatology, storm       | +26.702 | 33.667 | 46.903 |  3005 |
| irtam, storm             |  +1.712 | 13.673 | 27.625 |  3005 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.626, essn +0.000, 13863 day pairs

### MUFD (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |       - |      - |      - |     0 |
| irtam                    |       - |      - |      - |     0 |
| essn (holdout)           |       - |      - |      - |     0 |
| essn+storm               |       - |      - |      - |     0 |
| climatology, quiet       |       - |      - |      - |     0 |
| irtam, quiet             |       - |      - |      - |     0 |
| essn, quiet              |       - |      - |      - |     0 |
| essn+storm, quiet        |       - |      - |      - |     0 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |
| essn, storm              |       - |      - |      - |     0 |
| essn+storm, storm        |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, essn+storm +0.000, 0 day pairs

### foE (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.107 |  0.214 |  0.334 |  6792 |
| irtam                    |  +0.107 |  0.214 |  0.334 |  6792 |
| climatology, quiet       |  +0.092 |  0.211 |  0.330 |  5347 |
| irtam, quiet             |  +0.092 |  0.211 |  0.330 |  5347 |
| climatology, storm       |  +0.152 |  0.217 |  0.348 |  1445 |
| irtam, storm             |  +0.152 |  0.217 |  0.348 |  1445 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, 6748 day pairs

### NVIS MUF(d) from foF2 x secant (n = 13883)

| range | model        |    bias |    MAE |    RMS | band calls right |
| ----: | ------------ | ------: | -----: | -----: | ---------------: |
|    0k | climatology  |  -0.261 |  0.710 |  1.140 |            90.8% |
|    0k | clim+dudeney |  -0.261 |  0.710 |  1.140 |            90.8% |
|    0k | irtam-foF2   |  -0.046 |  0.342 |  0.694 |            94.9% |
|    0k | essn+dudeney |  +0.000 |  0.583 |  1.070 |            91.7% |
|    0k | essn+st+dud  |  -0.022 |  0.576 |  1.069 |            91.7% |
|    0k | irtam-both   |  -0.046 |  0.342 |  0.694 |            94.9% |
|  300k | climatology  |  -0.454 |  0.865 |  1.336 |            90.2% |
|  300k | clim+dudeney |  -0.397 |  0.839 |  1.310 |            90.3% |
|  300k | irtam-foF2   |  -0.209 |  0.432 |  0.831 |            94.6% |
|  300k | essn+dudeney |  -0.104 |  0.675 |  1.212 |            91.5% |
|  300k | essn+st+dud  |  -0.125 |  0.664 |  1.211 |            91.7% |
|  300k | irtam-both   |  -0.057 |  0.383 |  0.770 |            95.0% |
|  600k | climatology  |  -0.825 |  1.241 |  1.865 |            92.7% |
|  600k | clim+dudeney |  -0.673 |  1.156 |  1.756 |            92.9% |
|  600k | irtam-foF2   |  -0.532 |  0.727 |  1.263 |            95.4% |
|  600k | essn+dudeney |  -0.300 |  0.909 |  1.607 |            93.6% |
|  600k | essn+st+dud  |  -0.325 |  0.896 |  1.605 |            93.7% |
|  600k | irtam-both   |  -0.074 |  0.502 |  0.987 |            96.0% |

## 2019-06

37055 samples from 22 stations: AL945 AT138 AU930 BC840 DB049 EB040 EG931 FF051 GR13L HE13N JI91J JR055 LM42B MHJ45 MO155 PA836 PQ052 PRJ18 RO041 SAA0K SO148 WP937

### foF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.374 |  0.488 |  0.830 | 14081 |
| irtam                    |  -0.005 |  0.280 |  0.598 | 14062 |
| essn (holdout)           |  +0.020 |  0.399 |  0.746 | 14081 |
| essn+storm               |  +0.018 |  0.397 |  0.747 | 14081 |
| climatology, quiet       |  +0.375 |  0.484 |  0.830 | 13475 |
| irtam, quiet             |  -0.004 |  0.279 |  0.598 | 13475 |
| essn, quiet              |  +0.022 |  0.396 |  0.743 | 13475 |
| essn+storm, quiet        |  +0.022 |  0.396 |  0.743 | 13475 |
| climatology, storm       |  +0.364 |  0.554 |  0.832 |   587 |
| irtam, storm             |  -0.018 |  0.292 |  0.600 |   587 |
| essn, storm              |  -0.029 |  0.467 |  0.814 |   587 |
| essn+storm, storm        |  -0.082 |  0.436 |  0.816 |   587 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.493, essn +0.147, essn+storm +0.154, 14075 day pairs

### hmF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              | +43.272 | 44.817 | 64.956 | 14079 |
| irtam                    |  +8.232 | 17.860 | 35.629 | 14060 |
| climatology+dudeney      | +25.864 | 28.873 | 48.254 | 14079 |
| climatology, quiet       | +43.554 | 45.116 | 64.846 | 13473 |
| irtam, quiet             |  +8.318 | 17.764 | 35.535 | 13473 |
| climatology, storm       | +33.755 | 38.372 | 67.879 |   587 |
| irtam, storm             |  +6.165 | 19.780 | 37.729 |   587 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.301, essn +0.000, 14073 day pairs

### MUFD (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |       - |      - |      - |     0 |
| irtam                    |       - |      - |      - |     0 |
| essn (holdout)           |       - |      - |      - |     0 |
| essn+storm               |       - |      - |      - |     0 |
| climatology, quiet       |       - |      - |      - |     0 |
| irtam, quiet             |       - |      - |      - |     0 |
| essn, quiet              |       - |      - |      - |     0 |
| essn+storm, quiet        |       - |      - |      - |     0 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |
| essn, storm              |       - |      - |      - |     0 |
| essn+storm, storm        |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, essn+storm +0.000, 0 day pairs

### foE (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  -0.097 |  0.168 |  0.240 |  8895 |
| irtam                    |  -0.097 |  0.168 |  0.240 |  8889 |
| climatology, quiet       |  -0.097 |  0.167 |  0.239 |  8505 |
| irtam, quiet             |  -0.097 |  0.167 |  0.239 |  8505 |
| climatology, storm       |  -0.086 |  0.175 |  0.245 |   384 |
| irtam, storm             |  -0.086 |  0.175 |  0.245 |   384 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, 8887 day pairs

### NVIS MUF(d) from foF2 x secant (n = 14079)

| range | model        |    bias |    MAE |    RMS | band calls right |
| ----: | ------------ | ------: | -----: | -----: | ---------------: |
|    0k | climatology  |  +0.374 |  0.488 |  0.830 |            92.5% |
|    0k | clim+dudeney |  +0.374 |  0.488 |  0.830 |            92.5% |
|    0k | irtam-foF2   |  -0.005 |  0.280 |  0.598 |            95.6% |
|    0k | essn+dudeney |  +0.020 |  0.399 |  0.746 |            94.2% |
|    0k | essn+st+dud  |  +0.018 |  0.397 |  0.747 |            94.2% |
|    0k | irtam-both   |  -0.005 |  0.280 |  0.598 |            95.6% |
|  300k | climatology  |  +0.125 |  0.441 |  0.905 |            91.3% |
|  300k | clim+dudeney |  +0.219 |  0.448 |  0.906 |            91.7% |
|  300k | irtam-foF2   |  -0.285 |  0.416 |  0.839 |            91.7% |
|  300k | essn+dudeney |  -0.212 |  0.480 |  0.920 |            90.7% |
|  300k | essn+st+dud  |  -0.216 |  0.483 |  0.923 |            90.6% |
|  300k | irtam-both   |  -0.075 |  0.333 |  0.732 |            93.6% |
|  600k | climatology  |  -0.209 |  0.727 |  1.417 |            89.9% |
|  600k | clim+dudeney |  +0.039 |  0.642 |  1.313 |            92.1% |
|  600k | irtam-foF2   |  -0.705 |  0.805 |  1.549 |            88.4% |
|  600k | essn+dudeney |  -0.541 |  0.787 |  1.461 |            89.0% |
|  600k | essn+st+dud  |  -0.547 |  0.790 |  1.465 |            88.9% |
|  600k | irtam-both   |  -0.162 |  0.515 |  1.115 |            93.2% |

## 2019-12

37581 samples from 22 stations: AL945 AT138 AU930 BC840 DB049 EB040 EG931 FF051 GR13L HE13N JI91J JR055 LM42B MHJ45 MO155 PA836 PQ052 PRJ18 RO041 SAA0K SO148 WP937

### foF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.073 |  0.543 |  0.928 | 15463 |
| irtam                    |  +0.032 |  0.300 |  0.547 | 15463 |
| essn (holdout)           |  -0.007 |  0.545 |  0.920 | 15463 |
| essn+storm               |  -0.005 |  0.546 |  0.920 | 15463 |
| climatology, quiet       |  +0.073 |  0.543 |  0.928 | 15463 |
| irtam, quiet             |  +0.032 |  0.300 |  0.547 | 15463 |
| essn, quiet              |  -0.007 |  0.545 |  0.920 | 15463 |
| essn+storm, quiet        |  -0.005 |  0.546 |  0.920 | 15463 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |
| essn, storm              |       - |      - |      - |     0 |
| essn+storm, storm        |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.572, essn +0.108, essn+storm +0.108, 15463 day pairs

### hmF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              | +29.995 | 32.500 | 43.072 | 15463 |
| irtam                    |  +4.079 | 13.303 | 26.239 | 15463 |
| climatology+dudeney      | +17.830 | 22.448 | 35.246 | 15463 |
| climatology, quiet       | +29.995 | 32.500 | 43.072 | 15463 |
| irtam, quiet             |  +4.079 | 13.303 | 26.239 | 15463 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.319, essn +0.000, 15463 day pairs

### MUFD (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |       - |      - |      - |     0 |
| irtam                    |       - |      - |      - |     0 |
| essn (holdout)           |       - |      - |      - |     0 |
| essn+storm               |       - |      - |      - |     0 |
| climatology, quiet       |       - |      - |      - |     0 |
| irtam, quiet             |       - |      - |      - |     0 |
| essn, quiet              |       - |      - |      - |     0 |
| essn+storm, quiet        |       - |      - |      - |     0 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |
| essn, storm              |       - |      - |      - |     0 |
| essn+storm, storm        |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, essn+storm +0.000, 0 day pairs

### foE (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  -0.071 |  0.161 |  0.251 |  6655 |
| irtam                    |  -0.071 |  0.161 |  0.251 |  6655 |
| climatology, quiet       |  -0.071 |  0.161 |  0.251 |  6655 |
| irtam, quiet             |  -0.071 |  0.161 |  0.251 |  6655 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, 6642 day pairs

### NVIS MUF(d) from foF2 x secant (n = 15463)

| range | model        |    bias |    MAE |    RMS | band calls right |
| ----: | ------------ | ------: | -----: | -----: | ---------------: |
|    0k | climatology  |  +0.073 |  0.543 |  0.928 |            91.8% |
|    0k | clim+dudeney |  +0.073 |  0.543 |  0.928 |            91.8% |
|    0k | irtam-foF2   |  +0.032 |  0.300 |  0.547 |            95.1% |
|    0k | essn+dudeney |  -0.007 |  0.545 |  0.920 |            92.3% |
|    0k | essn+st+dud  |  -0.005 |  0.546 |  0.920 |            92.3% |
|    0k | irtam-both   |  +0.032 |  0.300 |  0.547 |            95.1% |
|  300k | climatology  |  -0.061 |  0.617 |  1.029 |            90.0% |
|  300k | clim+dudeney |  +0.006 |  0.628 |  1.048 |            89.9% |
|  300k | irtam-foF2   |  -0.121 |  0.373 |  0.661 |            93.2% |
|  300k | essn+dudeney |  -0.082 |  0.636 |  1.046 |            90.0% |
|  300k | essn+st+dud  |  -0.080 |  0.636 |  1.045 |            90.0% |
|  300k | irtam-both   |  +0.018 |  0.357 |  0.642 |            93.5% |
|  600k | climatology  |  -0.291 |  0.833 |  1.365 |            88.2% |
|  600k | clim+dudeney |  -0.118 |  0.829 |  1.385 |            88.0% |
|  600k | irtam-foF2   |  -0.388 |  0.631 |  1.023 |            92.3% |
|  600k | essn+dudeney |  -0.230 |  0.850 |  1.393 |            87.9% |
|  600k | essn+st+dud  |  -0.228 |  0.850 |  1.392 |            87.9% |
|  600k | irtam-both   |  -0.002 |  0.505 |  0.899 |            93.4% |

## 2022-09

29218 samples from 18 stations: AL945 AT138 AU930 BC840 DB049 EB040 EG931 FF051 GR13L HE13N JR055 LM42B MHJ45 PA836 PQ052 RO041 SO148 WP937

### foF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.530 |  0.686 |  1.120 | 11621 |
| irtam                    |  -0.063 |  0.357 |  0.714 | 11604 |
| essn (holdout)           |  +0.000 |  0.511 |  0.855 | 11621 |
| essn+storm               |  -0.008 |  0.511 |  0.857 | 11621 |
| climatology, quiet       |  +0.445 |  0.625 |  1.023 |  9806 |
| irtam, quiet             |  -0.069 |  0.352 |  0.682 |  9806 |
| essn, quiet              |  -0.004 |  0.494 |  0.828 |  9806 |
| essn+storm, quiet        |  -0.006 |  0.495 |  0.828 |  9806 |
| climatology, storm       |  +1.105 |  1.164 |  1.548 |  1798 |
| irtam, storm             |  -0.028 |  0.388 |  0.867 |  1798 |
| essn, storm              |  +0.028 |  0.602 |  0.990 |  1798 |
| essn+storm, storm        |  -0.016 |  0.600 |  1.001 |  1798 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.730, essn +0.462, essn+storm +0.452, 11621 day pairs

### hmF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              | +55.215 | 55.374 | 60.804 | 11621 |
| irtam                    |  +3.288 | 12.971 | 24.394 | 11604 |
| climatology+dudeney      | +40.877 | 41.327 | 47.309 | 11621 |
| climatology, quiet       | +56.025 | 56.140 | 60.726 |  9806 |
| irtam, quiet             |  +3.220 | 12.480 | 23.006 |  9806 |
| climatology, storm       | +48.644 | 49.325 | 61.358 |  1798 |
| irtam, storm             |  +4.032 | 16.058 | 30.882 |  1798 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.413, essn +0.000, 11621 day pairs

### MUFD (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |       - |      - |      - |     0 |
| irtam                    |       - |      - |      - |     0 |
| essn (holdout)           |       - |      - |      - |     0 |
| essn+storm               |       - |      - |      - |     0 |
| climatology, quiet       |       - |      - |      - |     0 |
| irtam, quiet             |       - |      - |      - |     0 |
| essn, quiet              |       - |      - |      - |     0 |
| essn+storm, quiet        |       - |      - |      - |     0 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |
| essn, storm              |       - |      - |      - |     0 |
| essn+storm, storm        |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, essn+storm +0.000, 0 day pairs

### foE (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.144 |  0.223 |  0.386 |  5976 |
| irtam                    |  +0.144 |  0.223 |  0.386 |  5972 |
| climatology, quiet       |  +0.147 |  0.220 |  0.383 |  5031 |
| irtam, quiet             |  +0.147 |  0.220 |  0.383 |  5031 |
| climatology, storm       |  +0.125 |  0.231 |  0.399 |   941 |
| irtam, storm             |  +0.125 |  0.231 |  0.399 |   941 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, 5957 day pairs

### NVIS MUF(d) from foF2 x secant (n = 11621)

| range | model        |    bias |    MAE |    RMS | band calls right |
| ----: | ------------ | ------: | -----: | -----: | ---------------: |
|    0k | climatology  |  +0.530 |  0.686 |  1.120 |            89.7% |
|    0k | clim+dudeney |  +0.530 |  0.686 |  1.120 |            89.7% |
|    0k | irtam-foF2   |  -0.063 |  0.357 |  0.714 |            94.2% |
|    0k | essn+dudeney |  +0.000 |  0.511 |  0.855 |            92.4% |
|    0k | essn+st+dud  |  -0.008 |  0.511 |  0.857 |            92.3% |
|    0k | irtam-both   |  -0.063 |  0.357 |  0.714 |            94.2% |
|  300k | climatology  |  +0.329 |  0.714 |  1.114 |            90.7% |
|  300k | clim+dudeney |  +0.391 |  0.718 |  1.135 |            90.7% |
|  300k | irtam-foF2   |  -0.331 |  0.499 |  0.845 |            93.8% |
|  300k | essn+dudeney |  -0.209 |  0.615 |  0.988 |            91.8% |
|  300k | essn+st+dud  |  -0.218 |  0.612 |  0.987 |            91.8% |
|  300k | irtam-both   |  -0.090 |  0.397 |  0.758 |            94.7% |
|  600k | climatology  |  -0.033 |  0.953 |  1.405 |            91.5% |
|  600k | clim+dudeney |  +0.138 |  0.920 |  1.375 |            91.8% |
|  600k | irtam-foF2   |  -0.838 |  0.933 |  1.402 |            91.5% |
|  600k | essn+dudeney |  -0.623 |  0.903 |  1.453 |            91.7% |
|  600k | essn+st+dud  |  -0.631 |  0.901 |  1.448 |            91.7% |
|  600k | irtam-both   |  -0.147 |  0.517 |  0.936 |            95.0% |

## 2024-12

22396 samples from 14 stations: AL945 AT138 DB049 EB040 EG931 FF051 GR13L HE13N JR055 LM42B MHJ45 PA836 PQ052 SO148

### foF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.857 |  0.963 |  1.466 |  9519 |
| irtam                    |  -0.045 |  0.392 |  0.638 |  9519 |
| essn (holdout)           |  +0.005 |  0.617 |  0.941 |  9519 |
| essn+storm               |  +0.006 |  0.615 |  0.941 |  9519 |
| climatology, quiet       |  +0.846 |  0.955 |  1.468 |  9142 |
| irtam, quiet             |  -0.047 |  0.390 |  0.633 |  9142 |
| essn, quiet              |  +0.004 |  0.616 |  0.939 |  9142 |
| essn+storm, quiet        |  +0.011 |  0.616 |  0.939 |  9142 |
| climatology, storm       |  +1.067 |  1.124 |  1.415 |   377 |
| irtam, storm             |  +0.004 |  0.448 |  0.766 |   377 |
| essn, storm              |  +0.038 |  0.646 |  0.994 |   377 |
| essn+storm, storm        |  -0.039 |  0.592 |  0.976 |   377 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.757, essn +0.428, essn+storm +0.426, 9515 day pairs

### hmF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              | +53.260 | 53.418 | 60.224 |  9519 |
| irtam                    |  +5.639 | 13.395 | 22.870 |  9519 |
| climatology+dudeney      | +43.509 | 43.976 | 50.387 |  9519 |
| climatology, quiet       | +53.462 | 53.547 | 60.222 |  9142 |
| irtam, quiet             |  +5.807 | 13.360 | 22.581 |  9142 |
| climatology, storm       | +47.670 | 47.743 | 60.282 |   377 |
| irtam, storm             |  +1.677 | 14.951 | 29.018 |   377 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.520, essn +0.000, 9515 day pairs

### MUFD (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |       - |      - |      - |     0 |
| irtam                    |       - |      - |      - |     0 |
| essn (holdout)           |       - |      - |      - |     0 |
| essn+storm               |       - |      - |      - |     0 |
| climatology, quiet       |       - |      - |      - |     0 |
| irtam, quiet             |       - |      - |      - |     0 |
| essn, quiet              |       - |      - |      - |     0 |
| essn+storm, quiet        |       - |      - |      - |     0 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |
| essn, storm              |       - |      - |      - |     0 |
| essn+storm, storm        |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, essn+storm +0.000, 0 day pairs

### foE (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.181 |  0.220 |  0.373 |  3358 |
| irtam                    |  +0.181 |  0.220 |  0.373 |  3358 |
| climatology, quiet       |  +0.179 |  0.220 |  0.371 |  3225 |
| irtam, quiet             |  +0.179 |  0.220 |  0.371 |  3225 |
| climatology, storm       |  +0.211 |  0.233 |  0.433 |   133 |
| irtam, storm             |  +0.211 |  0.233 |  0.433 |   133 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, 3336 day pairs

### NVIS MUF(d) from foF2 x secant (n = 9519)

| range | model        |    bias |    MAE |    RMS | band calls right |
| ----: | ------------ | ------: | -----: | -----: | ---------------: |
|    0k | climatology  |  +0.857 |  0.963 |  1.466 |            87.7% |
|    0k | clim+dudeney |  +0.857 |  0.963 |  1.466 |            87.7% |
|    0k | irtam-foF2   |  -0.045 |  0.392 |  0.638 |            94.3% |
|    0k | essn+dudeney |  +0.005 |  0.617 |  0.941 |            91.1% |
|    0k | essn+st+dud  |  +0.006 |  0.615 |  0.941 |            91.1% |
|    0k | irtam-both   |  -0.045 |  0.392 |  0.638 |            94.3% |
|  300k | climatology  |  +0.690 |  0.934 |  1.430 |            89.8% |
|  300k | clim+dudeney |  +0.728 |  0.943 |  1.450 |            89.7% |
|  300k | irtam-foF2   |  -0.291 |  0.485 |  0.791 |            94.4% |
|  300k | essn+dudeney |  -0.211 |  0.712 |  1.109 |            91.9% |
|  300k | essn+st+dud  |  -0.205 |  0.710 |  1.107 |            91.9% |
|  300k | irtam-both   |  -0.085 |  0.438 |  0.716 |            94.8% |
|  600k | climatology  |  +0.393 |  1.046 |  1.603 |            92.0% |
|  600k | clim+dudeney |  +0.519 |  1.051 |  1.613 |            91.8% |
|  600k | irtam-foF2   |  -0.773 |  0.854 |  1.388 |            94.7% |
|  600k | essn+dudeney |  -0.588 |  1.051 |  1.647 |            93.0% |
|  600k | essn+st+dud  |  -0.588 |  1.049 |  1.644 |            93.0% |
|  600k | irtam-both   |  -0.163 |  0.569 |  0.950 |            95.6% |

## 2025-03

24275 samples from 14 stations: AL945 AT138 DB049 EB040 EG931 FF051 GR13L HE13N JR055 LM42B MHJ45 PA836 PQ052 SO148

### foF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.338 |  0.695 |  1.203 |  9720 |
| irtam                    |  -0.126 |  0.401 |  0.695 |  9720 |
| essn (holdout)           |  +0.001 |  0.620 |  1.064 |  9720 |
| essn+storm               |  -0.007 |  0.602 |  1.058 |  9720 |
| climatology, quiet       |  +0.244 |  0.625 |  1.070 |  7490 |
| irtam, quiet             |  -0.145 |  0.382 |  0.632 |  7490 |
| essn, quiet              |  -0.013 |  0.572 |  0.972 |  7490 |
| essn+storm, quiet        |  -0.018 |  0.565 |  0.970 |  7490 |
| climatology, storm       |  +0.812 |  0.989 |  1.568 |  2230 |
| irtam, storm             |  -0.036 |  0.498 |  0.873 |  2230 |
| essn, storm              |  +0.082 |  0.813 |  1.328 |  2230 |
| essn+storm, storm        |  +0.038 |  0.761 |  1.310 |  2230 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.795, essn +0.404, essn+storm +0.401, 9695 day pairs

### hmF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              | +50.904 | 51.401 | 56.445 |  9720 |
| irtam                    |  +2.049 | 11.536 | 21.468 |  9720 |
| climatology+dudeney      | +39.698 | 40.747 | 46.926 |  9720 |
| climatology, quiet       | +52.625 | 52.790 | 57.431 |  7490 |
| irtam, quiet             |  +2.064 | 11.036 | 19.748 |  7490 |
| climatology, storm       | +42.696 | 45.113 | 53.002 |  2230 |
| irtam, storm             |  +1.860 | 13.480 | 26.439 |  2230 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.675, essn +0.000, 9695 day pairs

### MUFD (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |       - |      - |      - |     0 |
| irtam                    |       - |      - |      - |     0 |
| essn (holdout)           |       - |      - |      - |     0 |
| essn+storm               |       - |      - |      - |     0 |
| climatology, quiet       |       - |      - |      - |     0 |
| irtam, quiet             |       - |      - |      - |     0 |
| essn, quiet              |       - |      - |      - |     0 |
| essn+storm, quiet        |       - |      - |      - |     0 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |
| essn, storm              |       - |      - |      - |     0 |
| essn+storm, storm        |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, essn+storm +0.000, 0 day pairs

### foE (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.230 |  0.266 |  0.407 |  4835 |
| irtam                    |  +0.230 |  0.266 |  0.407 |  4835 |
| climatology, quiet       |  +0.239 |  0.271 |  0.418 |  3787 |
| irtam, quiet             |  +0.239 |  0.271 |  0.418 |  3787 |
| climatology, storm       |  +0.194 |  0.247 |  0.363 |  1048 |
| irtam, storm             |  +0.194 |  0.247 |  0.363 |  1048 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, 4802 day pairs

### NVIS MUF(d) from foF2 x secant (n = 9720)

| range | model        |    bias |    MAE |    RMS | band calls right |
| ----: | ------------ | ------: | -----: | -----: | ---------------: |
|    0k | climatology  |  +0.338 |  0.695 |  1.203 |            90.5% |
|    0k | clim+dudeney |  +0.338 |  0.695 |  1.203 |            90.5% |
|    0k | irtam-foF2   |  -0.126 |  0.401 |  0.695 |            94.7% |
|    0k | essn+dudeney |  +0.001 |  0.620 |  1.064 |            91.7% |
|    0k | essn+st+dud  |  -0.007 |  0.602 |  1.058 |            91.9% |
|    0k | irtam-both   |  -0.126 |  0.401 |  0.695 |            94.7% |
|  300k | climatology  |  +0.120 |  0.759 |  1.285 |            92.1% |
|  300k | clim+dudeney |  +0.164 |  0.755 |  1.290 |            92.1% |
|  300k | irtam-foF2   |  -0.386 |  0.556 |  0.863 |            94.8% |
|  300k | essn+dudeney |  -0.193 |  0.735 |  1.230 |            92.6% |
|  300k | essn+st+dud  |  -0.203 |  0.724 |  1.218 |            92.7% |
|  300k | irtam-both   |  -0.146 |  0.447 |  0.770 |            95.2% |
|  600k | climatology  |  -0.317 |  1.087 |  1.692 |            93.4% |
|  600k | clim+dudeney |  -0.176 |  1.030 |  1.650 |            93.4% |
|  600k | irtam-foF2   |  -0.906 |  1.019 |  1.442 |            95.0% |
|  600k | essn+dudeney |  -0.574 |  1.079 |  1.731 |            94.1% |
|  600k | essn+st+dud  |  -0.588 |  1.070 |  1.709 |            94.1% |
|  600k | irtam-both   |  -0.207 |  0.579 |  0.991 |            95.8% |

## 2025-06

26396 samples from 15 stations: AL945 AT138 DB049 EB040 EG931 FF051 GR13L HE13N JI91J JR055 LM42B MHJ45 PA836 PQ052 SO148

### foF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.743 |  0.914 |  1.394 | 10126 |
| irtam                    |  -0.011 |  0.364 |  0.732 | 10111 |
| essn (holdout)           |  -0.010 |  0.574 |  1.165 | 10126 |
| essn+storm               |  -0.050 |  0.548 |  1.158 | 10126 |
| climatology, quiet       |  +0.625 |  0.789 |  1.249 |  7429 |
| irtam, quiet             |  -0.039 |  0.331 |  0.646 |  7429 |
| essn, quiet              |  -0.018 |  0.522 |  0.983 |  7429 |
| essn+storm, quiet        |  -0.021 |  0.516 |  0.983 |  7429 |
| climatology, storm       |  +1.215 |  1.362 |  1.735 |  2682 |
| irtam, storm             |  +0.090 |  0.498 |  0.928 |  2682 |
| essn, storm              |  +0.021 |  0.772 |  1.562 |  2682 |
| essn+storm, storm        |  -0.139 |  0.653 |  1.545 |  2682 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.745, essn +0.164, essn+storm +0.228, 10126 day pairs

### hmF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              | +61.495 | 62.178 | 76.569 | 10126 |
| irtam                    |  +3.493 | 14.941 | 30.511 | 10111 |
| climatology+dudeney      | +42.189 | 43.459 | 56.496 | 10126 |
| climatology, quiet       | +63.247 | 63.667 | 74.553 |  7429 |
| irtam, quiet             |  +3.399 | 14.043 | 27.922 |  7429 |
| climatology, storm       | +55.400 | 57.538 | 82.018 |  2682 |
| irtam, storm             |  +3.790 | 18.638 | 36.743 |  2682 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.533, essn +0.000, 10126 day pairs

### MUFD (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |       - |      - |      - |     0 |
| irtam                    |       - |      - |      - |     0 |
| essn (holdout)           |       - |      - |      - |     0 |
| essn+storm               |       - |      - |      - |     0 |
| climatology, quiet       |       - |      - |      - |     0 |
| irtam, quiet             |       - |      - |      - |     0 |
| essn, quiet              |       - |      - |      - |     0 |
| essn+storm, quiet        |       - |      - |      - |     0 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |
| essn, storm              |       - |      - |      - |     0 |
| essn+storm, storm        |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, essn+storm +0.000, 0 day pairs

### foE (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.201 |  0.227 |  0.367 |  6144 |
| irtam                    |  +0.201 |  0.228 |  0.367 |  6139 |
| climatology, quiet       |  +0.202 |  0.228 |  0.366 |  4601 |
| irtam, quiet             |  +0.202 |  0.228 |  0.366 |  4601 |
| climatology, storm       |  +0.197 |  0.227 |  0.371 |  1538 |
| irtam, storm             |  +0.197 |  0.227 |  0.371 |  1538 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, 6139 day pairs

### NVIS MUF(d) from foF2 x secant (n = 10126)

| range | model        |    bias |    MAE |    RMS | band calls right |
| ----: | ------------ | ------: | -----: | -----: | ---------------: |
|    0k | climatology  |  +0.743 |  0.914 |  1.394 |            86.6% |
|    0k | clim+dudeney |  +0.743 |  0.914 |  1.394 |            86.6% |
|    0k | irtam-foF2   |  -0.011 |  0.364 |  0.732 |            94.4% |
|    0k | essn+dudeney |  -0.010 |  0.574 |  1.165 |            90.7% |
|    0k | essn+st+dud  |  -0.050 |  0.548 |  1.158 |            90.8% |
|    0k | irtam-both   |  -0.011 |  0.364 |  0.732 |            94.4% |
|  300k | climatology  |  +0.539 |  0.842 |  1.385 |            88.0% |
|  300k | clim+dudeney |  +0.613 |  0.870 |  1.411 |            87.9% |
|  300k | irtam-foF2   |  -0.292 |  0.467 |  0.885 |            93.2% |
|  300k | essn+dudeney |  -0.217 |  0.617 |  1.322 |            90.0% |
|  300k | essn+st+dud  |  -0.262 |  0.609 |  1.328 |            89.9% |
|  300k | irtam-both   |  -0.040 |  0.407 |  0.814 |            93.7% |
|  600k | climatology  |  +0.149 |  0.960 |  1.657 |            89.8% |
|  600k | clim+dudeney |  +0.374 |  0.971 |  1.652 |            89.6% |
|  600k | irtam-foF2   |  -0.842 |  0.933 |  1.492 |            91.4% |
|  600k | essn+dudeney |  -0.645 |  0.918 |  1.830 |            89.9% |
|  600k | essn+st+dud  |  -0.698 |  0.931 |  1.856 |            89.6% |
|  600k | irtam-both   |  -0.097 |  0.541 |  1.082 |            94.2% |

## 2025-07

26026 samples from 15 stations: AL945 AT138 DB049 EB040 EG931 FF051 GR13L HE13N JI91J JR055 LM42B MHJ45 PA836 PQ052 SO148

### foF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.590 |  0.750 |  1.108 | 10025 |
| irtam                    |  -0.019 |  0.319 |  0.579 | 10025 |
| essn (holdout)           |  -0.037 |  0.514 |  0.882 | 10025 |
| essn+storm               |  -0.041 |  0.507 |  0.877 | 10025 |
| climatology, quiet       |  +0.562 |  0.735 |  1.088 |  9433 |
| irtam, quiet             |  -0.028 |  0.321 |  0.576 |  9433 |
| essn, quiet              |  -0.036 |  0.510 |  0.878 |  9433 |
| essn+storm, quiet        |  -0.034 |  0.507 |  0.875 |  9433 |
| climatology, storm       |  +1.033 |  1.059 |  1.383 |   592 |
| irtam, storm             |  +0.103 |  0.300 |  0.631 |   592 |
| essn, storm              |  -0.050 |  0.578 |  0.943 |   592 |
| essn+storm, storm        |  -0.153 |  0.512 |  0.910 |   592 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.751, essn +0.287, essn+storm +0.300, 10025 day pairs

### hmF2 (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              | +54.490 | 54.897 | 68.959 | 10025 |
| irtam                    |  +4.330 | 14.356 | 28.276 | 10025 |
| climatology+dudeney      | +37.249 | 37.902 | 50.695 | 10025 |
| climatology, quiet       | +54.252 | 54.557 | 68.302 |  9433 |
| irtam, quiet             |  +4.083 | 14.276 | 27.582 |  9433 |
| climatology, storm       | +60.720 | 61.785 | 78.695 |   592 |
| irtam, storm             |  +8.304 | 16.001 | 37.646 |   592 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.481, essn +0.000, 10025 day pairs

### MUFD (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |       - |      - |      - |     0 |
| irtam                    |       - |      - |      - |     0 |
| essn (holdout)           |       - |      - |      - |     0 |
| essn+storm               |       - |      - |      - |     0 |
| climatology, quiet       |       - |      - |      - |     0 |
| irtam, quiet             |       - |      - |      - |     0 |
| essn, quiet              |       - |      - |      - |     0 |
| essn+storm, quiet        |       - |      - |      - |     0 |
| climatology, storm       |       - |      - |      - |     0 |
| irtam, storm             |       - |      - |      - |     0 |
| essn, storm              |       - |      - |      - |     0 |
| essn+storm, storm        |       - |      - |      - |     0 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, essn+storm +0.000, 0 day pairs

### foE (model - observed)

| model                    |    bias |    MAE |    RMS |     n |
| ------------------------ | ------: | -----: | -----: | ----: |
| climatology              |  +0.113 |  0.209 |  0.348 |  5976 |
| irtam                    |  +0.113 |  0.209 |  0.348 |  5976 |
| climatology, quiet       |  +0.112 |  0.207 |  0.347 |  5617 |
| irtam, quiet             |  +0.112 |  0.207 |  0.347 |  5617 |
| climatology, storm       |  +0.135 |  0.231 |  0.360 |   359 |
| irtam, storm             |  +0.135 |  0.231 |  0.360 |   359 |

day-to-day: climatology +0.000 (guard: must be +0.000), irtam +0.000, essn +0.000, 5961 day pairs

### NVIS MUF(d) from foF2 x secant (n = 10025)

| range | model        |    bias |    MAE |    RMS | band calls right |
| ----: | ------------ | ------: | -----: | -----: | ---------------: |
|    0k | climatology  |  +0.590 |  0.750 |  1.108 |            89.3% |
|    0k | clim+dudeney |  +0.590 |  0.750 |  1.108 |            89.3% |
|    0k | irtam-foF2   |  -0.019 |  0.319 |  0.579 |            95.3% |
|    0k | essn+dudeney |  -0.037 |  0.514 |  0.882 |            92.8% |
|    0k | essn+st+dud  |  -0.041 |  0.507 |  0.877 |            92.8% |
|    0k | irtam-both   |  -0.019 |  0.319 |  0.579 |            95.3% |
|  300k | climatology  |  +0.408 |  0.714 |  1.060 |            90.4% |
|  300k | clim+dudeney |  +0.478 |  0.739 |  1.087 |            90.3% |
|  300k | irtam-foF2   |  -0.273 |  0.412 |  0.704 |            93.7% |
|  300k | essn+dudeney |  -0.211 |  0.553 |  0.980 |            91.6% |
|  300k | essn+st+dud  |  -0.224 |  0.548 |  0.981 |            91.6% |
|  300k | irtam-both   |  -0.047 |  0.354 |  0.638 |            94.5% |
|  600k | climatology  |  +0.011 |  0.797 |  1.264 |            91.3% |
|  600k | clim+dudeney |  +0.246 |  0.809 |  1.250 |            91.1% |
|  600k | irtam-foF2   |  -0.787 |  0.843 |  1.260 |            92.5% |
|  600k | essn+dudeney |  -0.593 |  0.803 |  1.384 |            91.2% |
|  600k | essn+st+dud  |  -0.606 |  0.808 |  1.392 |            91.2% |
|  600k | irtam-both   |  -0.109 |  0.478 |  0.869 |            94.7% |
