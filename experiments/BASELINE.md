# Baseline：原版 Dora 效能

## ZKDisj 折疊層（Interstellar 改動要比較的對象）

`dietmc_0p` 跑隨機產生的 disjunction 電路（產生器：`diet-mac-and-cheese/gen/disj-small-bench.py`，
跟論文的 `disj-hyper-param.py` 相同，只是參數縮小）。每條指令 100 個輸入、100 個輸出（gate 數較少時取 gate 數）。
欄位為 F61p，LPN `medium`，環境同下方。

| 指令數 | 每條 gate 數 | 步數 | prover 時間 | 通訊量 (bytes total) |
|---|---|---|---|---|
| 8 | 64 | 2000 | 4.21 s | 19,666,928 |
| 8 | 512 | 2000 | 4.65 s | 33,803,536 |

重跑：

```bash
python3 diet-mac-and-cheese/gen/disj-small-bench.py f61 /tmp/dora-circ
cargo build --release --bin dietmc_0p
experiments/runcirc.sh /tmp/dora-circ/branches_0000000000128000_2000_clauses_8_gates_64 c8-g64
```

---

## ZKBag 讀取（RAM）

這是**未經修改**的 Dora 實作（上游 `rot256/research-dora`，commit `ddf48f9`）的基準數字，之後所有改動都拿來跟這份比。

## 測什麼

`diet-mac-and-cheese/bin/dietmc_ram.rs`：每一步對隨機位址做一次讀取，也就是
`ram.remove(addr)` 之後接 `ram.insert(addr, value)`（remove-then-reinsert），
最後 `finalize` 做 permutation check。欄位為 F61p，LPN 參數 `medium`。

## 環境

- 雲端 Linux，2 vCPU，prover 和 verifier 跑在同一台機器上，透過 localhost 連線（沒有模擬網路延遲）
- Rust 1.97 stable（repo 指定的 1.71.1 在此環境下載不到）
- `cargo build --release --bin dietmc_ram`，沒有加 `-C target-cpu=native`

## 結果

| 記憶體大小 ℓ | 步數 t | prover 時間 | 通訊量 (bytes total) |
|---|---|---|---|
| 2^16 | 2^20 | 3.82 s / 4.02 s / 3.83 s（三次） | 60,209,776 |
| 2^16 | 2^22 | 13.51 s | 216,612,448 |

換算下來（2^22 那組）：每步約 3.2 µs，通訊量約 51.6 bytes/step。

## 怎麼重跑

```bash
cargo build --release --bin dietmc_ram
experiments/runbench.sh 65536 1048576 base-s16-t20
```

注意：腳本每次隨機挑一個 port；如果剛好撞到還沒釋放的 port，verifier 會報
`Address already in use`，prover 會一直等。遇到時把卡住的 `dietmc_ram` 程序關掉重跑即可。
