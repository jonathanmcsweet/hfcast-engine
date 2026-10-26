#!/usr/bin/env bash
#
# Sourced, not run. The flags the reference is built with, for the O2
# variant in build-variants.sh and for build-trace.sh.
#
# No loop vectorisation: gfortran would send sin and cos loops to glibc's
# vector maths library, which is less accurate than the scalar one the
# source calls and the engine uses.
# shellcheck disable=SC2034
REFERENCE_FFLAGS="-g -O2 -fno-tree-vectorize"
