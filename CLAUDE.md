# Dora × Interstellar（碩士論文實作）

## 這個專案在做什麼

- 論文方向：把 Dora（ZK for RAM programs，ACM CCS 2024 / ePrint 2023/1749）的 ZKDisj 折疊層，
  從 relaxed R1CS 換成 Interstellar（PKC 2026 / ePrint 2025/1294）的 circuit interpolation，
  同時保留 Dora 的指令隱藏（oblivious instruction selection）。
- 目前階段：驗證「兩者能不能真的結合」並量測效能。第一版已完成，正確性測試通過。
- 本 repo 是 private 的 `maimai0822/dora-interstellar`，內容來自第二作者的論文實作 `rot256/research-dora`（MIT，基於 Galois 的 swanky）；
  舊的公開 fork `maimai0822/research-dora` 不再使用。

## Branch

- `minor-revision`：原作者版本，不要動
- `zkbag-read-experiment`：原版基準數字
- `interstellar-folding`：目前的實作（主要工作 branch）

## 已完成的設計（interstellar-folding）

只改 `diet-mac-and-cheese/src/dora/`；ZKBag、permutation check、VOLE commitment 都沒動。

- Accumulator 從 (w, e) 改成 (τ, w, y)，關係為 Σᵢ eq(τ, i)·(lᵢ(w)·rᵢ(w) − oᵢ(w)) = y。
  沒有 u、沒有 cross term T。
- 每步順序：commit w₂ → commit 舊 accumulator → 抽 τ₂ → commit y(2)…y(k+2) → 抽 r → 沿直線 w(t) = (1−t)w₁ + t·w₂ 折疊。
- y(t) 次數 k+2（eq 為 k 次、約束為 2 次），y(0)=y₁、y(1)=0 已知，所以 commit k+1 個點。
- 指令隱藏靠：k 由整組指令共用（`Disjunction::log_rows`）、τ 與 y 值都 commit 而非明文送出、維度 pad 到整組最大值。
- 主要檔案：`acc.rs`（accumulator、最後驗證、fold）、`r1cs.rs`（eq table、fold_evals、Lagrange）、
  `comm.rs`（CommittedEvals）、`prover.rs` / `verifier.rs`（步驟順序）、`disjunction.rs`（共用 k）。
- 測試在 `prover.rs` 底部：`test_fold_plain`、`test_range_example`、`test_mul_clauses_honest`、
  `test_mul_clauses_cheat_detected`（第 50 步偽造 gate 值，verifier 必須拒絕）。

## 建置與測試

```bash
cargo build --release -p diet-mac-and-cheese
cargo test --release -p diet-mac-and-cheese dora::
```

- repo 的 `rust-toolchain` 指定 1.71.1；雲端開發時用的是新版 stable（`cargo +stable ...`），編譯正常。
- swanky 的 SIMD（`vectoreyes`）只有 x86 版；Apple Silicon 上應退回 scalar 版（尚未實測）。
- 協定是單執行緒。

## Benchmark

- 電路產生：`python3 diet-mac-and-cheese/gen/disj-small-bench.py f61 /tmp/dora-circ`（論文的 `disj-hyper-param.py` 縮小版）
- 執行：`experiments/runcirc.sh <電路資料夾> <名稱>`（ZKDisj）、`experiments/runbench.sh <size> <steps> <名稱>`（ZKBag RAM）
- 腳本先啟動 verifier（它 listen）再啟動 prover，port 選在 ephemeral 範圍（32768+）以下，避免 prover 連到自己而卡住。
- 記憶體約與「指令數 × 每條 gate 數」成正比（每條指令各自建一份約束表），固定開銷約 0.5 GB。
  16 GB 機器大約跑得到乘積 2^24；論文最大的 2^15 × 2^15 用的是 124 GB 的 AWS 機器。
- 不同機器的絕對時間不能直接比，同一台機器上「原版 vs Interstellar」才是公平比較。

既有數字：
@experiments/BASELINE.md
@experiments/RESULTS.md

## 工作方式

- 用繁體中文回答。
- 除錯先取得實際錯誤訊息再診斷；區分已驗證的事實與推測，每個假設附上可驗證的檢查方式。
- 回報研究資訊時直接給資訊，判斷交給我；我明確問時才給你的評估。
- 修改 `src/` 底下的程式碼、commit 或 push 之前先問我。
- 跑完 benchmark 要回報實際數字（time circ exec、bytes total）與使用的參數。

## 尚未做

- 論文規模參數的量測、每步邊際成本的分離量測
- 指令隱藏的自動化測試（目前靠結構保證）
- 形式化安全證明；跨指令 padding 參數的選擇
