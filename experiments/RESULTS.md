# Dora + Interstellar：第一版結果

Branch `interstellar-folding`：ZKDisj 的折疊從 relaxed R1CS 換成 Interstellar 的 circuit interpolation，
ZKBag、permutation check、VOLE commitment 不變，保留指令隱藏（τ 與 y(t) 都 commit，維度由整組指令決定）。

## 正確性

| 測試 | 內容 | 結果 |
|---|---|---|
| `test_fold_plain` | 不經 commitment，連續折疊 20 步，每步檢查 G(τ, w) = y | 通過；故意壞一步時最後檢查失敗 |
| `test_range_example` | 原版測試（256 條指令的 range check，10,000 步） | 通過 |
| `test_mul_clauses_honest` | 4 條含乘法的指令，完整 prover–verifier 協定，200 步 | 通過 |
| `test_mul_clauses_cheat_detected` | 同上，第 50 步偽造一個乘法 gate 的值 | verifier 拒絕（`check_zero failed`） |

反向檢查：把最後的 Σ eq(τ, i)·Fᵢ(w) = y 檢查拿掉，作弊測試就會通過，
所以拒絕確實來自新的 accumulator 檢查，不是別的錯誤。

## 效能（與 BASELINE.md 同一組電路、同一台機器）

8 條指令、2000 步，prover 與 verifier 在同一台 2 vCPU 雲端機器、localhost 連線。

| 每條 gate 數 | 版本 | prover 時間 | 通訊量 (bytes total) |
|---|---|---|---|
| 64 | 原版 Dora | 4.21 s | 19,666,928 |
| 64 | Interstellar | 4.17 s | 16,156,144（−17.9%） |
| 512 | 原版 Dora | 4.65 s | 33,803,536 |
| 512 | Interstellar | 4.69 s | 23,640,528（−30.1%） |

觀察：

- 2000 步時兩版時間相同：時間被固定成本蓋過（見下方「VOLE 批次」）。
- 總通訊量含固定的 VOLE setup/extend 流量，所以比每步的邊際差距小（見下方拆解）。

## 每步成本拆解（commit 的元素數）

從程式碼數出每一步 prover 要 commit 的元素（每個元素 = 1 個 VOLE + 8 bytes）：

| 項目 | 原版 Dora | Interstellar |
|---|---|---|
| 新 witness | \|w\| | \|w\| |
| 折疊修正項 | T：rows | y(2..k+2)：k+1 |
| 重新 commit 舊 accumulator（ZKBag） | z：dim_ext ＋ e：rows | τ：k ＋ z：dim_ext ＋ y：1 |
| 其他（permutation 2 個乘法、條件輸入 1） | 3 | 3 |

benchmark 電路的實際維度（暫時加 log 印出）：

| 每條 gate 數 | \|w\| | dim_ext | rows | k | Dora 每步 | Interstellar 每步 | 減少 |
|---|---|---|---|---|---|---|---|
| 64 | 117 | 183 | 118 | 7 | 539 | 319 | −41% |
| 512 | 328 | 430 | 329 | 9 | 1419 | 781 | −45% |

與實測對照：

- 兩版每步差距：模型 1760 / 5104 bytes，實測（2000 步）1755 / 5081 bytes。
- 64 gate Interstellar 從 6000 步到 16000 步，每多一步多 2552 bytes = 319 個元素，與模型完全相同。

解讀：對電路轉出的 R1CS，rows ≈ \|w\|，Dora 每步 ≈ 4\|w\| ＋ 輸入數；Interstellar 拿掉兩個 rows，
剩 ≈ 2\|w\| ＋ 輸入數。換掉後最大的一塊是「重新 commit 舊 accumulator」（約 56–60%），
那是 ZKBag 隱藏指令的機制，Interstellar 不碰它。

## VOLE 批次與時間

每個 commit 消耗一個 VOLE。VOLE 由 LPN extend 成批產生，`LPN_EXTEND_MEDIUM` 一批約一千萬個，
不論用多少都要付整批的成本。2000 步的總用量只有 60 萬到 280 萬，一批都用不完，所以兩版時間相同。

步數放大，用量超過一批後時間就拉開（512 gate、8 條指令、50,000 步，雲端 x86，各 2 次）：

| 版本 | 時間 | 通訊量 (bytes total) |
|---|---|---|
| 原版 Dora | 34.9 s / 34.1 s | 597,626,584 |
| Interstellar | 25.1 s / 25.5 s | 331,656,120（−44.5%） |

時間 −27%（1.36×），通訊量 −44.5%。時間的差距小於通訊量，代表還有一部分時間不隨 commit 數量變動
（witness 計算、折疊運算、QuickSilver 檢查等）；各部分佔比尚未用 profiler 量。
→ 正式時間測試的步數至少要 50,000，否則差異會被固定成本蓋掉。

64 gate Interstellar 的步數掃描（2000 / 6000 / 16000 步）：4.17 s / 5.95 s / 6.28 s。

## 每步邊際成本（Intel Mac）

環境：MacBook，Intel Core i5-1038NG7（4 核 8 緒）、16 GB、macOS 14.8.9（Darwin 23.6）、Rust 1.71.1（repo 指定），
prover 與 verifier 同機 localhost。
原版是 `minor-revision`（`ddf48f9`，與 BASELINE 同一 commit），用 git worktree 另外編譯。

參數：電路由 `disj-hyper-param.py` 改參數產生（clauses = [8]、gates = [64, 512, 4096]、branches = [2000, 16000]），
F61p、LPN `medium`、每條指令 100 輸入 / 100 輸出。
每組跑 3 次，兩版交替執行（`experiments/runcirc.sh`，外包 `caffeinate -i`），prover 以 `/usr/bin/time -l` 量最大記憶體。
時間取 time circ exec 的中位數；bytes total 三次都相同。

邊際成本 = (t16000 − t2000) / 14000，bytes 同理。

| 每條 gate 數 | 版本 | 2000 步時間（3 次） | 16000 步時間（3 次） | 邊際時間 / 步 | bytes total（2000 / 16000） | 邊際 bytes / 步 | prover 最大 RSS（2000 / 16000） |
|---|---|---|---|---|---|---|---|
| 64 | 原版 Dora | 6.34 / 5.32 / 5.16 → **5.32 s** | 7.14 / 6.08 / 5.41 → **6.08 s** | 54.5 µs | 19,666,928 / 80,034,928 | 4,312.0 | 497 / 543 MB |
| 64 | Interstellar | 5.36 / 5.13 / 6.04 → **5.36 s** | 7.03 / 6.44 / 6.16 → **6.44 s** | 77.4 µs | 16,156,144 / 51,884,144 | 2,552.0 | 495 / 545 MB |
| 512 | 原版 Dora | 6.11 / 5.38 / 5.29 → **5.38 s** | 17.44 / 15.30 / 18.71 → **17.44 s** | 861.3 µs | 33,803,536 / 198,139,264 | 11,738.3 | 512 / 701 MB |
| 512 | Interstellar | 5.50 / 6.78 / 5.50 → **5.50 s** | 15.14 / 11.25 / 12.12 → **12.12 s** | 472.9 µs | 23,640,528 / 113,816,392 | 6,441.1 | 520 / 621 MB |
| 4096 | 原版 Dora | 7.62 / 5.94 / 5.38 → **5.94 s** | 36.81 / 34.31 / 29.93 → **34.31 s** | 2,025.9 µs | 78,011,664 / 561,386,848 | 34,526.8 | 524 / 1303 MB |
| 4096 | Interstellar | 7.71 / 6.75 / 5.95 → **6.75 s** | 26.27 / 26.39 / 23.87 → **26.27 s** | 1,394.2 µs | 45,886,224 / 295,805,816 | 17,851.4 | 528 / 1082 MB |

（時間列順序為第 1 / 2 / 3 輪；RSS 為三次的中位數，MB = 2^20 bytes。）

Interstellar 相對原版：

| 每條 gate 數 | 邊際時間 | 邊際 bytes |
|---|---|---|
| 64 | +42%（54.5 → 77.4 µs） | −40.8% |
| 512 | −45.1% | −45.1% |
| 4096 | −31.2% | −48.3% |

量測上的事實（供解讀）：

- 64 gate 的邊際 bytes 與上方元素模型完全一致：原版 539 × 8 = 4312、Interstellar 319 × 8 = 2552。
  512 gate 實測（11,738 / 6,441）高於模型（11,352 / 6,248）；16000 步時總 commit 數（約 22.7M / 12.5M）已超過一批 LPN extend（約一千萬），
  差額應是多出來的 extend 流量（推測，可用 log 中 extend 次數驗證）。4096 gate 的元素模型尚未印出維度，沒有對照。
- 64 gate 兩版 16000 步的 commit 總數（約 8.6M / 5.1M）都在一批之內，時間增量只有約 1 s；
  三次之間的差距（約 0.9–1.7 s）與增量同量級，換算成每步約 ±60–120 µs，所以 64 gate 的邊際時間差異在雜訊範圍內。
- 第 1 輪通常最慢（12 組中 8 組），可能是機器暖機或頻率調整。
- 記憶體：2000 步兩版都約 0.5 GB（固定開銷）；16000 步隨 gate 數增加，4096 gate 原版 1.30 GB、Interstellar 1.08 GB。

重跑（產生器修改的只有三個參數列）：

```bash
sed -e 's/^params_clauses  = .*/params_clauses  = [2**3]/' \
    -e 's/^params_gates    = .*/params_gates    = [2**6, 2**9, 2**12]/' \
    -e 's/^params_branches = .*/params_branches = [2000, 16000]/' \
    diet-mac-and-cheese/gen/disj-hyper-param.py > /tmp/disj-marginal.py
(cd diet-mac-and-cheese/gen && PYTHONPATH=$PWD python3 /tmp/disj-marginal.py f61 /tmp/circ-marginal)
git worktree add --detach ../dora-minor-revision origin/minor-revision   # 在其中 cargo build --release --bin dietmc_0p
# 在要測的那份 worktree 內執行（runcirc.sh 用 git toplevel 找 binary）：
PWRAP="/usr/bin/time -l" caffeinate -i experiments/runcirc.sh /tmp/circ-marginal/branches_..._16000_clauses_8_gates_4096 inter-g4096-n16000
```

## 已知可改進

- 最後驗證：每條指令建 eq 表多約 2^(k+1) 個乘法，最後驗證約為原版的 2 倍，指令數多時明顯。
  每層 `lo = table − hi` 可省一半乘法。
- 只用了 degree 2 約束，Interstellar 的高次數約束尚未使用（可縮小 \|w\|，連帶縮小 accumulator 的重新 commit）。

## 尚未做

- 論文規模的參數（最多 2^15 條指令、每條 2^15 gate、50,000 步）在 x86 實體機上的正式量測
- 時間的 profiling（VOLE、witness 計算、折疊運算各佔多少）
- 指令隱藏的自動化測試（目前是靠結構保證：每步 commit 的數量只跟整組指令有關）
- 形式化安全證明
