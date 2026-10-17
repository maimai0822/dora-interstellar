# Baseline：原版 ZKBag 讀取效能

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
