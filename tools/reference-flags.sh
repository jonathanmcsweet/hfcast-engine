#!/usr/bin/env bash
#
# Sourced, not run. The flags the reference is built with, for the O2
# variant in build-variants.sh and for build-trace.sh.
#
# No loop vectorisation: gfortran would send sin and cos loops to glibc's
# vector maths library, which is less accurate than the scalar one the
# source calls and the engine uses.
#
# No fused multiply-add: a compiler targeting newer x86 (the ubuntu-26.04
# runner's targets x86-64-v3) or ARM would turn a*b+c into one
# instruction, which rounds once where the source and the engine round
# twice.
# shellcheck disable=SC2034
REFERENCE_FFLAGS="-g -O2 -fno-tree-vectorize -ffp-contract=off"
