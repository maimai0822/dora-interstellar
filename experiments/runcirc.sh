#!/bin/bash
# usage: runcirc.sh CIRCDIR LABEL
# The verifier listens and the prover connects, so the verifier is started first.
# Ports are kept below the Linux ephemeral range (32768+): a prover retrying a
# connection to an unused ephemeral port can end up connected to itself.
BIN="$(git rev-parse --show-toplevel)/target/release/dietmc_0p"
D=$1; OUT="${OUT:-/tmp/dora-bench}"; mkdir -p $OUT
PORT=$((10000 + RANDOM % 20000))
export RUST_LOG=info
# GNU timeout is not available on macOS by default (gtimeout via `brew install coreutils`)
TO=$(command -v timeout || command -v gtimeout || true)
[ -n "$TO" ] && TO="$TO 500"
# optional wrapper for the prover, e.g. PWRAP="/usr/bin/time -l" (output lands in the prover log)
PWRAP="${PWRAP:-}"
$BIN --text --lpn medium --relation $D/relation.txt --instance $D/public.txt --connection-addr 127.0.0.1:$PORT > $OUT/$2-v.log 2>&1 &
V=$!; sleep 1
$TO $PWRAP $BIN --text --lpn medium --relation $D/relation.txt --instance $D/public.txt --connection-addr 127.0.0.1:$PORT prover --witness $D/private.txt > $OUT/$2-p.log 2>&1; echo "prover exit $?"
wait $V; echo "verifier exit $?"
grep -iE "time circ exec|bytes total|error|maximum resident" $OUT/$2-p.log | tail -5
