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

## 已知可改進

- 最後驗證：每條指令建 eq 表多約 2^(k+1) 個乘法，最後驗證約為原版的 2 倍，指令數多時明顯。
  每層 `lo = table − hi` 可省一半乘法。
- 只用了 degree 2 約束，Interstellar 的高次數約束尚未使用（可縮小 \|w\|，連帶縮小 accumulator 的重新 commit）。

## 尚未做

- 論文規模的參數（最多 2^15 條指令、每條 2^15 gate、50,000 步）在 x86 實體機上的正式量測
- 時間的 profiling（VOLE、witness 計算、折疊運算各佔多少）
- 指令隱藏的自動化測試（目前是靠結構保證：每步 commit 的數量只跟整組指令有關）
- 形式化安全證明
