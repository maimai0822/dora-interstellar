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

- 通訊量下降，且指令越大降越多。
- 時間在這個規模幾乎不變；64 與 512 gate 的時間本身也差不多，表示這個規模下時間主要不是花在折疊上，
  需要更大的電路或 profiling 才看得出折疊本身的差異。
- 總通訊量包含每步的輸入、輸出與 QuickSilver 檢查等固定成本，所以比「每步 commit 的元素數」的差距小。

## 尚未做

- 論文規模的參數（最多 2^15 條指令、每條 2^15 gate、50,000 步），以及每步邊際成本的分離量測
- 指令隱藏的自動化測試（目前是靠結構保證：每步 commit 的數量只跟整組指令有關）
- 形式化安全證明
