#!/bin/bash
# usage: runcirc.sh CIRCDIR LABEL
BIN="$(git rev-parse --show-toplevel)/target/release/dietmc_0p"
D=$1; OUT="${OUT:-/tmp/dora-bench}"; mkdir -p $OUT
PORT=$((20000 + RANDOM % 20000))
export RUST_LOG=info
$BIN --text --lpn medium --relation $D/relation.txt --instance $D/public.txt --connection-addr 127.0.0.1:$PORT prover --witness $D/private.txt > $OUT/$2-p.log 2>&1 &
P=$!; sleep 1
timeout 500 $BIN --text --lpn medium --relation $D/relation.txt --instance $D/public.txt --connection-addr 127.0.0.1:$PORT > $OUT/$2-v.log 2>&1; echo "verifier exit $?"
wait $P; echo "prover exit $?"
grep -iE "time|bytes total|VERIFIER|error" $OUT/$2-p.log | tail -6
