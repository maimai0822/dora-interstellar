#!/bin/bash
# usage: runbench.sh SIZE STEPS LABEL
BIN="$(git rev-parse --show-toplevel)/target/release/dietmc_ram"
OUT="${OUT:-/tmp/dora-bench}"
mkdir -p $OUT
PORT=$((10000 + RANDOM % 20000))  # below the ephemeral range, see runcirc.sh
export RUST_LOG=info
$BIN --lpn medium --ram-size $1 --ram-steps $2 --connection-addr 127.0.0.1:$PORT > $OUT/$3-v.log 2>&1 &
V=$!; sleep 1
$BIN --lpn medium --ram-size $1 --ram-steps $2 --connection-addr 127.0.0.1:$PORT --prover > $OUT/$3-p.log 2>&1
wait $V
for r in p v; do echo "[$3 $r]"; grep -E "time ram exec|bytes total" $OUT/$3-$r.log; done
