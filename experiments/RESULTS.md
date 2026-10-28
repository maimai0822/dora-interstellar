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

## 指令數的影響（Intel Mac）

環境同上一段（Intel Core i5-1038NG7、16 GB、macOS 14.8.9、Rust 1.71.1，localhost）。
原版的 worktree 重新建立在 `../dora-minor-revision`（`ddf48f9`），並且重新編譯。cargo 在它的 `Cargo.lock` 自動補了一行
（跟本 branch `e46999b9` 相同的 rustc-hash），程式碼沒有改。

參數：從 `disj-hyper-param.py` 改參數產生電路，每條指令 512 gate，步數 2000 / 16000，指令數 8、64、512、4096，
F61p、LPN `medium`、每條指令 100 輸入 / 100 輸出。
分兩批跑，每批 3 輪。第一批（8、64）每輪依序跑 (8, 2000) → (8, 16000) → (64, 2000) → (64, 16000)；第二批（512、4096）同樣的順序。
每組都先跑原版、再跑 Interstellar。
用 `PWRAP="/usr/bin/time -l" caffeinate -i experiments/runcirc.sh` 執行。時間取 time circ exec 的中位數，bytes total 三次都相同。

**冷卻間隔：512、4096 這一批每次執行之間停 30 s（為了減少發熱降頻）；8、64 那一批以及本檔其他段落都沒有停。**
兩批的時間不能直接當作同一條件比較。

邊際成本 = (t16000 − t2000) / 14000；固定成本 = t2000 − 2000 × 邊際成本（bytes 用同樣的公式）。

| 指令數 | 版本 | 2000 步時間（3 次） | 16000 步時間（3 次） | 邊際時間 / 步 | 推算固定時間 | bytes total（2000 / 16000） | 邊際 bytes / 步 | 推算固定 bytes | prover 最大 RSS（2000 / 16000） |
|---|---|---|---|---|---|---|---|---|---|
| 8 | 原版 Dora | 6.45 / 7.31 / 6.25 → **6.45 s** | 16.59 / 17.87 / 15.51 → **16.59 s** | 724.4 µs | 5.00 s | 33,803,536 / 198,139,264 | 11,738.3 | 10,327,003 | 520 / 701 MB |
| 8 | Interstellar | 7.85 / 7.30 / 6.13 → **7.30 s** | 14.74 / 13.68 / 19.96 → **14.74 s** | 531.7 µs | 6.23 s | 23,640,528 / 113,816,392 | 6,441.1 | 10,758,262 | 512 / 621 MB |
| 64 | 原版 Dora | 6.81 / 6.27 / 8.96 → **6.81 s** | 17.04 / 15.04 / 24.47 → **17.04 s** | 730.7 µs | 5.35 s | 34,439,248 / 198,774,976 | 11,738.3 | 10,962,715 | 526 / 708 MB |
| 64 | Interstellar | 7.26 / 6.70 / 9.95 → **7.26 s** | 14.41 / 12.60 / 18.12 → **14.41 s** | 510.9 µs | 6.24 s | 24,591,184 / 114,767,048 | 6,441.1 | 11,708,918 | 510 / 626 MB |
| 512 † | 原版 Dora | 7.05 / 10.13 / 4.72 → **7.05 s** | 16.31 / 22.86 / 17.26 → **17.26 s** | 729.6 µs | 5.59 s | 39,524,944 / 203,860,672 | 11,738.3 | 16,048,411 | 541 / 749 MB |
| 512 † | Interstellar | 5.98 / 8.21 / 6.46 → **6.46 s** | 14.00 / 12.39 / 12.41 → **12.41 s** | 425.4 µs | 5.61 s | 32,196,432 / 122,372,296 | 6,441.1 | 19,314,166 | 546 / 666 MB |
| 4096 † | 原版 Dora | 9.15 / 6.91 / 6.23 → **6.91 s** | 23.59 / 15.06 / 27.03 → **23.59 s** | 1,191.3 µs | 4.53 s | 80,210,512 / 244,546,240 | 11,738.3 | 56,733,979 | 726 / 1049 MB |
| 4096 † | Interstellar | 20.71 / 13.72 / 12.42 → **13.72 s** | 26.39 / 16.68 / 20.48 → **20.48 s** | 482.6 µs | 12.75 s | 95,742,280 / 185,918,144 | 6,441.1 | 82,860,014 | 743 / 992 MB |

（† 每次執行之間有 30 s 冷卻。時間依第 1 / 2 / 3 輪的順序列出；RSS 為三次的中位數，MB = 2^20 bytes。）

bytes total（2000 步）相對 8 條指令的增量：

| 指令數 | 原版增量 | 每多一條 | Interstellar 增量 | 每多一條 |
|---|---|---|---|---|
| 64 | 635,712 | 11,352 | 950,656 | 16,976 |
| 512 | 5,721,408 | 11,352 | 8,555,904 | 16,976 |
| 4096 | 46,406,976 | 11,352 | 72,101,752 | 17,637.4 |

量測上的事實（供解讀）：

- 產生器裡每條指令都是**同一個** function（`clauses.append(RND)`，產生不同 function 的那幾行被註解掉了，上游原本就是這樣）。
  所以指令數只改變 switch 的分支數，電路檔幾乎一樣大：relation.txt 在 2000 步時，8 / 64 / 512 / 4096 條分別是 4641 / 4753 / 5649 / 12817 行。
- 四種指令數的邊際 bytes 完全相同（原版 11,738.3、Interstellar 6,441.1，也跟上一段 512 gate 的數字相同）。
- 指令數只讓 bytes total 多一個固定量（上表）。原版每多一條指令固定多 11,352 bytes（1419 個元素 × 8），從 64 到 4096 都一樣。
  Interstellar 在 512 條以內每條固定多 16,976 bytes（2,122 個元素）；到 4096 條變成平均每條 17,637.4，
  比線性外推多出 2,703,864 bytes。這一組比原版多做了一次 F61p extend（見下方 extend 次數表），已從 log 確認；
  不過 2,703,864 bytes 是否剛好等於一次 extend 的流量，沒有單獨量。
  這些「每條指令」的元素分別對應程式碼的哪個部分，還沒有對照。
- 原版 8 條、512 gate 的 bytes 與 BASELINE、上一段完全相同（33,803,536 / 198,139,264）。
- 推算固定時間：原版 8 → 4096 條為 5.00 / 5.35 / 5.59 / 4.53 s；Interstellar 為 6.23 / 6.24 / 5.61 / 12.75 s。
  Interstellar 4096 條 2000 步的三次（20.71 / 13.72 / 12.42 s）全部高於原版的三次（9.15 / 6.91 / 6.23 s）。
- 時間的變異大：同一組三次之間最多差約 12 s（4096 條原版 16000 步：15.06–27.03 s）；
  第一批第 3 輪的 64 條四組都是三次裡最慢的。邊際時間是兩個帶雜訊中位數相減再除以 14000，單組的數字不穩定。
- RSS：2000 步時原版從 520 MB（8 條）增加到 726 MB（4096 條），Interstellar 從 512 MB 增加到 743 MB。

LPN extend 次數：prover 的 log 不記 extend；verifier 每次 extend 會記一行 `SVOLE<time:… field:…>`（`homcom.rs:573–580`），
這是兩版唯一的 extend 呼叫點，兩版這段程式碼相同。下表是 F61p（`field:[254, 255, …, 31]`）的次數，每組三次都相同；
另外每次執行都還有 1 次 F2（`field:[1, 0, 0, 0, 0]`）的 extend，兩版一樣。
`LPN_EXTEND_MEDIUM` 一次可用的 VOLE 數 = cols − (rows + weight + 1) = 10,168,320 − 162,966 = 10,005,354（`ocelot/src/svole/wykw/svole.rs:76–102`）。

| 指令數 | 原版 2000 步 | 原版 16000 步 | Interstellar 2000 步 | Interstellar 16000 步 |
|---|---|---|---|---|
| 8 | 1 | 3 | 1 | 2 |
| 64 | 1 | 3 | 1 | 2 |
| 512 | 1 | 3 | 1 | 2 |
| 4096 | 1 | 3 | **2** | 3 |

4096 條每次 F61p extend 在 verifier 端花的時間：

| 步數 | 版本 | r1 | r2 | r3 |
|---|---|---|---|---|
| 2000 | 原版 | 2.43 s | 2.17 s | 2.12 s |
| 2000 | Interstellar | 4.23 + 4.03 s | 2.66 + 2.50 s | 2.52 + 2.22 s |
| 16000 | 原版 | 3.75 + 2.62 + 2.43 s | 2.18 + 2.03 + 2.22 s | 3.02 + 3.38 + 2.87 s |
| 16000 | Interstellar | 5.27 + 2.47 + 2.79 s | 2.28 + 2.07 + 2.07 s | 2.70 + 2.37 + 2.24 s |

**兩點相減的限制：** 邊際成本 = (t16000 − t2000) / 14000、固定成本 = t2000 − 2000 × 邊際，前提是成本跟步數成線性。
但 extend 是整批發生的（每次約 2–5 s，流量也是一整塊），兩個量測點之間 extend 次數不同，算出來的值就會失真：
差了幾次 extend，那幾次的成本就被攤進邊際成本；t2000 本身含幾次 extend，那幾次的成本就算進固定成本。

- 原版在每種指令數下都是 1 → 3 次，邊際成本都含 2 次 extend 的成本；Interstellar 在 8 / 64 / 512 條是 1 → 2 次，含 1 次。
- **Interstellar 4096 條的固定成本受影響：** 它的 t2000 含 2 次 extend，其他指令數只含 1 次。
  推算固定時間 12.75 s（原版 4.53 s）裡多了一次 extend 的成本（verifier 端 2.2–4.0 s）；
  固定 bytes 裡多的 2,703,864 也來自同一件事。剩下的差距推測來自最後驗證（每條指令建 eq 表），尚未用 profiler 確認。
- **原版 4096 條的邊際時間偏高（1,191.3 µs，其他指令數約 725–731 µs），但不是 extend 次數造成的：**
  它的 extend 次數跟其他指令數一樣（1 → 3），每次 extend 的時間也在同樣範圍。
  16000 步的三次時間差很多（15.06 / 23.59 / 27.03 s），中位數 23.59 s 落在偏高的一端；原因沒有查。
- 單次執行的牆鐘時間最長 31 s。電路產生每份 5–15 s，跟指令數無關（8、64 條產生時原版正在背景編譯）。

重跑：

```bash
for C in 8 64 512 4096; do for N in 2000 16000; do
  sed -e "s/^params_clauses  = .*/params_clauses  = [$C]/" \
      -e 's/^params_gates    = .*/params_gates    = [2**9]/' \
      -e "s/^params_branches = .*/params_branches = [$N]/" \
      diet-mac-and-cheese/gen/disj-hyper-param.py > /tmp/disj-c$C-n$N.py
  (cd diet-mac-and-cheese/gen && PYTHONPATH=$PWD python3 /tmp/disj-c$C-n$N.py f61 /tmp/circ-clauses/c$C-n$N)
done; done
# 原版：在 ../dora-minor-revision 內執行（runcirc.sh 用 git toplevel 找 binary）
(cd ../dora-minor-revision && PWRAP="/usr/bin/time -l" caffeinate -i $OLDPWD/experiments/runcirc.sh \
  /tmp/circ-clauses/c64-n16000/branches_0000000008192000_16000_clauses_64_gates_512 orig-c64-n16000)
# 512、4096 那一批：每次執行之間 sleep 30
```

## Profiling：prover 時間拆解（Intel Mac）

環境同上（Intel Core i5-1038NG7、16 GB、macOS 14.8.9、Rust 1.71.1，localhost）。兩組設定、每個版本各跑一次（共 4 次），每次之間停 30 s：

- (a) 8 條指令、每條 4096 gate、16000 步（看每一步的時間花在哪）：`disj-hyper-param.py` 改成 clauses = [2^3]、gates = [2^12]、branches = [16000]
- (b) 4096 條指令、每條 512 gate、2000 步（看指令數多時的固定成本）：沿用「指令數的影響」那一段的電路

方法：

- profiler 用 samply 0.13.1，透過 `runcirc.sh` 的 PWRAP 只包住 prover：
  `PWRAP="samply record --save-only --unstable-presymbolicate -o <檔名> --"`。取樣間隔 1 ms，單一執行緒。
  樣本裡有 `__recvfrom` / `__sendto`，所以等待網路的時間也算在內。
- 兩版都用 `CARGO_PROFILE_RELEASE_DEBUG=true cargo build --release --bin dietmc_0p` 重編，但兩份的 `[profile.release]` 本來就有 `debug = true`，
  所以 cargo 沒有重新產生執行檔（UUID：原版 `9EE951E5…`、Interstellar `216E686C…`，跟 profile 裡記的一致）。
- 符號：LTO 會把大部分函式 inline，只看符號表的話，self time 幾乎都算在少數幾個外層函式上
  （例如 `FComProver::input` 佔原版 (a) 的 47.6%，裡面其實包含被 inline 的 LPN extend）。
  所以另外用 `dsymutil` 從 `target/release/deps/*.o` 的 debug map 產生 dSYM（存在 `~/dora-profiles/2026-10-29/dsym/`，沒有改執行檔），
  再用 `atos -i` 把每個位址展開成 inline chain。下面的「self time」是**最內層 inline 函式**的時間。
- 分類規則（每個樣本歸到一類）：call chain 裡只要有 `ocelot::svole` / `ocelot::ot` / `scuttlebutt::cointoss`，不管在哪一層，一律算
  **VOLE/LPN extend**。其餘樣本從最內層往外找，第一個符合的類別就是它的分類：
  QuickSilver 檢查（`quicksilver_*`、`check_zero`、backend `finalize`）→ commit（`FComProver::input/input1/input_low_level/random`、
  `prover_commit_vec`、`*::commit_prover`）→ eq 表與最後驗證（`ComittedAcc::verify`、`r1cs::eq_table`）→ 折疊（`ComittedAcc::fold`、`fold_witness`、
  `fold_evals`、`eval_g`、`cross_terms`、`cross_wit_acc`、`R1CS::constraint`、`lagrange_coeffs`、`ComittedAcc::split`）→ permutation（`dora::perm`、`combine`、
  `DoraProver::compact`、`collapse_trace`）→ witness 計算（`compute_witness`、disjunction `execute`）→ 其他。
  因為由內往外找，最後驗證裡的 `mul` 所做的 commit 會算成 commit，其餘的乘法運算才算最後驗證。
- 注意：`r1cs::eq_table` 只在 `eval_g` 裡被呼叫（`r1cs.rs:413`），是每一步 `fold_evals` 的明文計算，不是最後驗證。
  所以下表把「eq 表與最後驗證」再拆開列。
- 注意：extend 只發生在 VOLE 用完的那一刻，所以「extend 在哪裡被觸發」只代表用完的時點，不代表是那段程式用掉了大部分 VOLE。

**profiler 本身讓執行變慢**，各類別的絕對秒數不能直接跟沒開 profiler 的量測比：

| 設定 | 版本 | time circ exec（samply） | 沒開 profiler 的中位數 | bytes total | F61p extend 次數（verifier log） |
|---|---|---|---|---|---|
| (a) | 原版 Dora | 43.95 s | 34.31 s（「每步邊際成本」那一段） | 561,386,848 | 7 |
| (a) | Interstellar | 44.01 s | 26.27 s（同上） | 295,805,816 | 4 |
| (b) | 原版 Dora | 12.28 s | 6.91 s（「指令數的影響」那一段，†） | 80,210,512 | 1 |
| (b) | Interstellar | 19.37 s | 13.72 s（同上，†） | 95,742,280 | 2 |

（bytes total 都跟沒開 profiler 時相同。每次執行另外都有 1 次 F2 extend。）
在 (a)，profiler 讓 Interstellar 慢了 68%、原版慢了 28%，所以 (a) 開著 profiler 時兩版的總時間幾乎一樣，這跟沒開 profiler 的結果不同。
各類別之間的比例可能也被 profiler 扭曲了，這一點沒有驗證。

#### (a) 8 條指令、4096 gate、16000 步：分類

| 類別 | 原版 Dora | Interstellar | 差（Inter − 原版） |
|---|---|---|---|
| VOLE/LPN extend | 36.60 s（84.0%） | 29.45 s（67.1%） | -7.15 s |
| commit | 2.93 s（6.7%） | 2.49 s（5.7%） | -0.45 s |
| 折疊 | 2.46 s（5.7%） | 9.59 s（21.8%） | +7.12 s |
| eq 表與最後驗證 | 0.00 s（0.0%） | 1.00 s（2.3%） | +1.00 s |
| QuickSilver 檢查 | 0.07 s（0.2%） | 0.07 s（0.2%） | +0.00 s |
| permutation | 1.06 s（2.4%） | 0.64 s（1.5%） | -0.42 s |
| witness 計算 | 0.38 s（0.9%） | 0.62 s（1.4%） | +0.23 s |
| 其他 | 0.05 s（0.1%） | 0.04 s（0.1%） | -0.01 s |
| **合計（取樣）** | **43.56 s** | **43.89 s** | **+0.33 s** |

「其他」細分：其餘（I/O、malloc、雜項） 0.04 / 0.03 s；電路讀取與 R1CS 建構 0.01 / 0.01 s；Trace/accumulator 釋放 0.00 / 0.00 s；電路直譯（evaluate_gates） 0.00 / 0.00 s（原版 / Interstellar）。

extend 依觸發位置（原版 / Interstellar）：

| 觸發位置 | 原版 | Interstellar |
|---|---|---|
| 每步 mux 中 | 29.11 s | 22.26 s |
| 第一個私密輸入（第一次 extend） | 5.67 s | 4.39 s |
| backend finalize（QuickSilver） | 1.68 s | 2.66 s |
| 初始化（EvaluatorCirc::new / load_backend） | 0.15 s | 0.15 s |

<details><summary>(a) 8 條指令、4096 gate、16000 步，原版 Dora：self time 前 30</summary>

| # | self time | % | 函式（最內層 inline） | 樣本所屬類別 |
|---|---|---|---|---|
| 1 | 16.33 s | 37.5% | `<F61p as core::ops::arith::MulAssign<&F61p>>::mul_assign` | VOLE/LPN extend 99%, permutation 0% |
| 2 | 9.27 s | 21.3% | `reduce` | VOLE/LPN extend 79%, 折疊 14% |
| 3 | 1.02 s | 2.3% | `swanky_field::ring::FiniteRing::random_nonzero` | VOLE/LPN extend 100% |
| 4 | 0.99 s | 2.3% | `__recvfrom` | VOLE/LPN extend 99%, QuickSilver 檢查 1% |
| 5 | 0.93 s | 2.1% | `core::ptr::write` | VOLE/LPN extend 39%, commit 32% |
| 6 | 0.88 s | 2.0% | `<F61p as core::ops::arith::AddAssign<&F61p>>::add_assign` | VOLE/LPN extend 80%, 折疊 12% |
| 7 | 0.87 s | 2.0% | `core::core_arch::x86::aes::_mm_aesenc_si128` | VOLE/LPN extend 100% |
| 8 | 0.86 s | 2.0% | `<rand::distributions::uniform::UniformInt<u32> as rand::distributions::uniform::UniformSampler>::sample` | VOLE/LPN extend 100% |
| 9 | 0.84 s | 1.9% | `ocelot::svole::wykw::svole::lpn_mtx_indices` | VOLE/LPN extend 100% |
| 10 | 0.76 s | 1.7% | `_platform_bzero$VARIANT$Haswell` | VOLE/LPN extend 96%, 折疊 2% |
| 11 | 0.63 s | 1.4% | `subtle::black_box` | VOLE/LPN extend 100% |
| 12 | 0.59 s | 1.4% | `ocelot::svole::wykw::svole::Sender<T>::send_internal` | VOLE/LPN extend 100% |
| 13 | 0.57 s | 1.3% | `<u32 as rand::distributions::utils::WideningMultiply>::wmul` | VOLE/LPN extend 100% |
| 14 | 0.48 s | 1.1% | `<rand_core::block::BlockRng64<R> as rand_core::RngCore>::next_u32` | VOLE/LPN extend 100% |
| 15 | 0.48 s | 1.1% | `<rand_core::block::BlockRng64<R> as rand_core::RngCore>::next_u64` | VOLE/LPN extend 100% |
| 16 | 0.47 s | 1.1% | `swanky_field_binary::small_binary_fields::_<impl core::ops::arith::Mul<swanky_field_binary::small_binary_fi…` | VOLE/LPN extend 100% |
| 17 | 0.41 s | 0.9% | `<u64 as subtle::ConstantTimeEq>::ct_eq` | VOLE/LPN extend 100% |
| 18 | 0.39 s | 0.9% | `vectoreyes::generated::array_impls::_<impl vectoreyes::array_utils::UnrollableArraySize<2_usize> for vector…` | VOLE/LPN extend 100% |
| 19 | 0.35 s | 0.8% | `__sendto` | commit 97%, VOLE/LPN extend 3% |
| 20 | 0.33 s | 0.7% | `_platform_memmove$VARIANT$Haswell` | commit 95%, VOLE/LPN extend 4% |
| 21 | 0.32 s | 0.7% | `<core::slice::iter::Iter<T> as core::iter::traits::iterator::Iterator>::any` | VOLE/LPN extend 100% |
| 22 | 0.28 s | 0.6% | `ocelot::svole::wykw::svole::lpn_mtx_indices::_{{closure}}` | VOLE/LPN extend 100% |
| 23 | 0.25 s | 0.6% | `alloc::vec::Vec<T,A>::push` | commit 50%, VOLE/LPN extend 18% |
| 24 | 0.25 s | 0.6% | `<core::slice::iter::Iter<T> as core::iter::traits::iterator::Iterator>::next` | VOLE/LPN extend 53%, 折疊 46% |
| 25 | 0.21 s | 0.5% | `core::hint::black_box` | VOLE/LPN extend 100% |
| 26 | 0.20 s | 0.5% | `dmc::dora::r1cs::LinComb<F>::eval` | 折疊 100% |
| 27 | 0.20 s | 0.5% | `dmc::backend::Monitor::tick` | permutation 73%, 折疊 27% |
| 28 | 0.18 s | 0.4% | `<subtle::Choice as core::convert::From<u8>>::from` | VOLE/LPN extend 100% |
| 29 | 0.18 s | 0.4% | `core::convert::num::ptr_try_from_impls::_<impl core::convert::TryFrom<u32> for usize>::try_from` | VOLE/LPN extend 100% |
| 30 | 0.17 s | 0.4% | `<dmc::dora::tx::TxChannel<C> as scuttlebutt::channel::AbstractChannel>::write_bytes` | commit 100% |

</details>

<details><summary>(a) 8 條指令、4096 gate、16000 步，Interstellar：self time 前 30</summary>

| # | self time | % | 函式（最內層 inline） | 樣本所屬類別 |
|---|---|---|---|---|
| 1 | 15.00 s | 34.2% | `<F61p as core::ops::arith::MulAssign<&F61p>>::mul_assign` | VOLE/LPN extend 95%, 折疊 5% |
| 2 | 11.32 s | 25.8% | `reduce` | 折疊 52%, VOLE/LPN extend 40% |
| 3 | 1.06 s | 2.4% | `<F61p as core::ops::arith::AddAssign<&F61p>>::add_assign` | 折疊 55%, VOLE/LPN extend 40% |
| 4 | 0.97 s | 2.2% | `core::ptr::write` | commit 42%, VOLE/LPN extend 34% |
| 5 | 0.90 s | 2.1% | `swanky_field_binary::small_binary_fields::_<impl core::ops::arith::Mul<swanky_field_binary::small_binary_fi…` | VOLE/LPN extend 100% |
| 6 | 0.85 s | 1.9% | `swanky_field::ring::FiniteRing::random_nonzero` | VOLE/LPN extend 100% |
| 7 | 0.74 s | 1.7% | `<rand::distributions::uniform::UniformInt<u32> as rand::distributions::uniform::UniformSampler>::sample` | VOLE/LPN extend 100% |
| 8 | 0.71 s | 1.6% | `ocelot::svole::wykw::svole::Sender<T>::send_internal` | VOLE/LPN extend 100% |
| 9 | 0.71 s | 1.6% | `ocelot::svole::wykw::svole::lpn_mtx_indices` | VOLE/LPN extend 100% |
| 10 | 0.67 s | 1.5% | `dmc::dora::r1cs::LinComb<F>::eval` | 折疊 100% |
| 11 | 0.63 s | 1.4% | `_platform_memmove$VARIANT$Haswell` | 折疊 57%, commit 40% |
| 12 | 0.61 s | 1.4% | `subtle::black_box` | VOLE/LPN extend 100%, 折疊 0% |
| 13 | 0.60 s | 1.4% | `__recvfrom` | VOLE/LPN extend 99%, QuickSilver 檢查 1% |
| 14 | 0.58 s | 1.3% | `core::core_arch::x86::aes::_mm_aesenc_si128` | VOLE/LPN extend 100% |
| 15 | 0.56 s | 1.3% | `<rand_core::block::BlockRng64<R> as rand_core::RngCore>::next_u32` | VOLE/LPN extend 100% |
| 16 | 0.53 s | 1.2% | `<u32 as rand::distributions::utils::WideningMultiply>::wmul` | VOLE/LPN extend 100% |
| 17 | 0.49 s | 1.1% | `<core::slice::iter::Iter<T> as core::iter::traits::iterator::Iterator>::next` | 折疊 83%, VOLE/LPN extend 14% |
| 18 | 0.43 s | 1.0% | `_platform_bzero$VARIANT$Haswell` | VOLE/LPN extend 92%, 折疊 5% |
| 19 | 0.40 s | 0.9% | `<rand_core::block::BlockRng64<R> as rand_core::RngCore>::next_u64` | VOLE/LPN extend 100% |
| 20 | 0.38 s | 0.9% | `__sendto` | commit 98%, VOLE/LPN extend 2% |
| 21 | 0.31 s | 0.7% | `<u64 as subtle::ConstantTimeEq>::ct_eq` | VOLE/LPN extend 100%, witness 計算 0% |
| 22 | 0.28 s | 0.6% | `alloc::vec::Vec<T,A>::push` | eq 表與最後驗證 52%, commit 24% |
| 23 | 0.27 s | 0.6% | `<core::slice::iter::Iter<T> as core::iter::traits::iterator::Iterator>::any` | VOLE/LPN extend 100% |
| 24 | 0.24 s | 0.5% | `vectoreyes::generated::array_impls::_<impl vectoreyes::array_utils::UnrollableArraySize<2_usize> for vector…` | VOLE/LPN extend 100% |
| 25 | 0.19 s | 0.4% | `<F61p as core::ops::arith::SubAssign<&F61p>>::sub_assign` | 折疊 65%, commit 35% |
| 26 | 0.18 s | 0.4% | `ocelot::svole::wykw::svole::lpn_mtx_indices::_{{closure}}` | VOLE/LPN extend 100% |
| 27 | 0.18 s | 0.4% | `core::hint::black_box` | VOLE/LPN extend 100% |
| 28 | 0.15 s | 0.3% | `<core::iter::adapters::copied::Copied<I> as core::iter::traits::iterator::Iterator>::next` | 折疊 100% |
| 29 | 0.15 s | 0.3% | `dmc::backend::Monitor::tick` | permutation 59%, 折疊 41% |
| 30 | 0.14 s | 0.3% | `<usize as core::slice::index::SliceIndex<[T]>>::index` | witness 計算 54%, VOLE/LPN extend 43% |

</details>

#### (b) 4096 條指令、512 gate、2000 步：分類

| 類別 | 原版 Dora | Interstellar | 差（Inter − 原版） |
|---|---|---|---|
| VOLE/LPN extend | 8.93 s（72.4%） | 15.85 s（81.7%） | +6.92 s |
| commit | 1.33 s（10.7%） | 1.14 s（5.9%） | -0.18 s |
| 折疊 | 1.14 s（9.3%） | 1.42 s（7.3%） | +0.28 s |
| eq 表與最後驗證 | 0.15 s（1.2%） | 0.26 s（1.3%） | +0.10 s |
| QuickSilver 檢查 | 0.30 s（2.5%） | 0.33 s（1.7%） | +0.03 s |
| permutation | 0.10 s（0.8%） | 0.08 s（0.4%） | -0.03 s |
| witness 計算 | 0.03 s（0.3%） | 0.03 s（0.1%） | -0.01 s |
| 其他 | 0.35 s（2.8%） | 0.30 s（1.6%） | -0.05 s |
| **合計（取樣）** | **12.35 s** | **19.41 s** | **+7.06 s** |

「其他」細分：電路讀取與 R1CS 建構 0.30 / 0.28 s；Trace/accumulator 釋放 0.04 / 0.02 s；其餘（I/O、malloc、雜項） 0.01 / 0.01 s（原版 / Interstellar）。

extend 依觸發位置（原版 / Interstellar）：

| 觸發位置 | 原版 | Interstellar |
|---|---|---|
| 第一個私密輸入（第一次 extend） | 6.44 s | 6.75 s |
| 最後驗證 verify 中 | 0.00 s | 6.51 s |
| backend finalize（QuickSilver） | 2.34 s | 2.43 s |
| 初始化（EvaluatorCirc::new / load_backend） | 0.15 s | 0.16 s |

<details><summary>(b) 4096 條指令、512 gate、2000 步，原版 Dora：self time 前 30</summary>

| # | self time | % | 函式（最內層 inline） | 樣本所屬類別 |
|---|---|---|---|---|
| 1 | 3.33 s | 26.9% | `<F61p as core::ops::arith::MulAssign<&F61p>>::mul_assign` | VOLE/LPN extend 99%, QuickSilver 檢查 0% |
| 2 | 1.30 s | 10.5% | `reduce` | VOLE/LPN extend 84%, 折疊 5% |
| 3 | 1.07 s | 8.7% | `__sendto` | commit 89%, VOLE/LPN extend 11% |
| 4 | 0.74 s | 6.0% | `swanky_field_binary::small_binary_fields::_<impl core::ops::arith::Mul<swanky_field_binary::small_binary_fi…` | VOLE/LPN extend 100% |
| 5 | 0.52 s | 4.2% | `ocelot::svole::wykw::svole::Sender<T>::send_internal` | VOLE/LPN extend 100% |
| 6 | 0.46 s | 3.7% | `core::ptr::write` | VOLE/LPN extend 76%, commit 14% |
| 7 | 0.36 s | 2.9% | `nanov2_malloc` | 折疊 100% |
| 8 | 0.35 s | 2.8% | `__recvfrom` | QuickSilver 檢查 56%, VOLE/LPN extend 44% |
| 9 | 0.27 s | 2.2% | `<rand::distributions::uniform::UniformInt<u32> as rand::distributions::uniform::UniformSampler>::sample` | VOLE/LPN extend 100% |
| 10 | 0.27 s | 2.2% | `ocelot::svole::wykw::svole::lpn_mtx_indices` | VOLE/LPN extend 100% |
| 11 | 0.26 s | 2.1% | `_nanov2_free` | 折疊 58%, 其他 41% |
| 12 | 0.22 s | 1.8% | `_platform_memmove$VARIANT$Haswell` | 折疊 74%, commit 25% |
| 13 | 0.21 s | 1.7% | `<rand_core::block::BlockRng64<R> as rand_core::RngCore>::next_u32` | VOLE/LPN extend 100% |
| 14 | 0.20 s | 1.6% | `swanky_field::ring::FiniteRing::random_nonzero` | VOLE/LPN extend 100% |
| 15 | 0.19 s | 1.5% | `core::core_arch::x86::aes::_mm_aesenc_si128` | VOLE/LPN extend 100% |
| 16 | 0.18 s | 1.4% | `<u32 as rand::distributions::utils::WideningMultiply>::wmul` | VOLE/LPN extend 100% |
| 17 | 0.16 s | 1.3% | `subtle::black_box` | VOLE/LPN extend 99%, QuickSilver 檢查 1% |
| 18 | 0.14 s | 1.2% | `_platform_bzero$VARIANT$Haswell` | VOLE/LPN extend 74%, 折疊 24% |
| 19 | 0.14 s | 1.1% | `<F61p as core::ops::arith::AddAssign<&F61p>>::add_assign` | VOLE/LPN extend 89%, 折疊 4% |
| 20 | 0.12 s | 0.9% | `madvise` | 其他 97%, 折疊 3% |
| 21 | 0.11 s | 0.9% | `vectoreyes::generated::array_impls::_<impl vectoreyes::array_utils::UnrollableArraySize<2_usize> for vector…` | VOLE/LPN extend 100% |
| 22 | 0.09 s | 0.8% | `<rand_core::block::BlockRng64<R> as rand_core::RngCore>::next_u64` | VOLE/LPN extend 100% |
| 23 | 0.09 s | 0.7% | `<core::slice::iter::Iter<T> as core::iter::traits::iterator::Iterator>::any` | VOLE/LPN extend 100% |
| 24 | 0.07 s | 0.6% | `ocelot::svole::wykw::svole::lpn_mtx_indices::_{{closure}}` | VOLE/LPN extend 100% |
| 25 | 0.06 s | 0.5% | `<u64 as subtle::ConstantTimeEq>::ct_eq` | VOLE/LPN extend 95%, QuickSilver 檢查 5% |
| 26 | 0.05 s | 0.4% | `dmc::dora::r1cs::LinComb<F>::eval_commit` | eq 表與最後驗證 100% |
| 27 | 0.04 s | 0.3% | `<usize as core::slice::index::SliceIndex<[T]>>::index` | VOLE/LPN extend 77%, witness 計算 14% |
| 28 | 0.04 s | 0.3% | `ocelot::svole::wykw::ggm_utils::ggm_prime` | VOLE/LPN extend 100% |
| 29 | 0.04 s | 0.3% | `<core::slice::iter::Iter<T> as core::iter::traits::iterator::Iterator>::next` | VOLE/LPN extend 78%, 折疊 15% |
| 30 | 0.04 s | 0.3% | `<subtle::Choice as core::convert::From<u8>>::from` | VOLE/LPN extend 95%, QuickSilver 檢查 5% |

</details>

<details><summary>(b) 4096 條指令、512 gate、2000 步，Interstellar：self time 前 30</summary>

| # | self time | % | 函式（最內層 inline） | 樣本所屬類別 |
|---|---|---|---|---|
| 1 | 7.01 s | 36.1% | `<F61p as core::ops::arith::MulAssign<&F61p>>::mul_assign` | VOLE/LPN extend 99%, 折疊 0% |
| 2 | 2.73 s | 14.1% | `reduce` | VOLE/LPN extend 82%, 折疊 9% |
| 3 | 0.83 s | 4.3% | `swanky_field_binary::small_binary_fields::_<impl core::ops::arith::Mul<swanky_field_binary::small_binary_fi…` | VOLE/LPN extend 100% |
| 4 | 0.83 s | 4.3% | `__sendto` | commit 99%, VOLE/LPN extend 1% |
| 5 | 0.54 s | 2.8% | `__recvfrom` | VOLE/LPN extend 70%, QuickSilver 檢查 30% |
| 6 | 0.51 s | 2.6% | `ocelot::svole::wykw::svole::Sender<T>::send_internal` | VOLE/LPN extend 100% |
| 7 | 0.43 s | 2.2% | `<rand::distributions::uniform::UniformInt<u32> as rand::distributions::uniform::UniformSampler>::sample` | VOLE/LPN extend 100% |
| 8 | 0.43 s | 2.2% | `core::ptr::write` | VOLE/LPN extend 81%, commit 10% |
| 9 | 0.40 s | 2.1% | `swanky_field::ring::FiniteRing::random_nonzero` | VOLE/LPN extend 100% |
| 10 | 0.39 s | 2.0% | `ocelot::svole::wykw::svole::lpn_mtx_indices` | VOLE/LPN extend 100% |
| 11 | 0.37 s | 1.9% | `nanov2_malloc` | 折疊 99%, commit 1% |
| 12 | 0.33 s | 1.7% | `<u32 as rand::distributions::utils::WideningMultiply>::wmul` | VOLE/LPN extend 100% |
| 13 | 0.31 s | 1.6% | `subtle::black_box` | VOLE/LPN extend 100% |
| 14 | 0.31 s | 1.6% | `<F61p as core::ops::arith::AddAssign<&F61p>>::add_assign` | VOLE/LPN extend 82%, 折疊 9% |
| 15 | 0.30 s | 1.6% | `core::core_arch::x86::aes::_mm_aesenc_si128` | VOLE/LPN extend 100% |
| 16 | 0.30 s | 1.5% | `<rand_core::block::BlockRng64<R> as rand_core::RngCore>::next_u32` | VOLE/LPN extend 100% |
| 17 | 0.27 s | 1.4% | `_nanov2_free` | 折疊 62%, 其他 37% |
| 18 | 0.23 s | 1.2% | `_platform_bzero$VARIANT$Haswell` | VOLE/LPN extend 87%, 折疊 12% |
| 19 | 0.20 s | 1.0% | `_platform_memmove$VARIANT$Haswell` | 折疊 77%, commit 19% |
| 20 | 0.18 s | 0.9% | `<rand_core::block::BlockRng64<R> as rand_core::RngCore>::next_u64` | VOLE/LPN extend 100% |
| 21 | 0.17 s | 0.9% | `<core::slice::iter::Iter<T> as core::iter::traits::iterator::Iterator>::any` | VOLE/LPN extend 100% |
| 22 | 0.16 s | 0.8% | `vectoreyes::generated::array_impls::_<impl vectoreyes::array_utils::UnrollableArraySize<2_usize> for vector…` | VOLE/LPN extend 100% |
| 23 | 0.13 s | 0.7% | `<u64 as subtle::ConstantTimeEq>::ct_eq` | VOLE/LPN extend 100% |
| 24 | 0.11 s | 0.6% | `ocelot::svole::wykw::svole::lpn_mtx_indices::_{{closure}}` | VOLE/LPN extend 100% |
| 25 | 0.09 s | 0.5% | `core::hint::black_box` | VOLE/LPN extend 99%, 折疊 1% |
| 26 | 0.09 s | 0.5% | `madvise` | 其他 91%, 折疊 6% |
| 27 | 0.07 s | 0.4% | `<core::slice::iter::Iter<T> as core::iter::traits::iterator::Iterator>::next` | VOLE/LPN extend 75%, 折疊 19% |
| 28 | 0.07 s | 0.4% | `<subtle::Choice as core::convert::From<u8>>::from` | VOLE/LPN extend 100% |
| 29 | 0.07 s | 0.4% | `alloc::vec::Vec<T,A>::push` | VOLE/LPN extend 42%, commit 22% |
| 30 | 0.06 s | 0.3% | `core::convert::num::ptr_try_from_impls::_<impl core::convert::TryFrom<u32> for usize>::try_from` | VOLE/LPN extend 100% |

</details>

「eq 表與最後驗證」拆開（原版 / Interstellar）：
(a) 每步 `eq_table` 0.00 / 1.00 s，最後驗證 0.00 / 0.00 s；
(b) 每步 `eq_table` 0.00 / 0.06 s，最後驗證 0.15 / 0.19 s。

量測上的事實：

- (a)：Interstellar 的 extend 少 7.15 s（4 次 extend 對 7 次），折疊多 7.12 s，每步的 `eq_table` 再多 1.00 s，兩版取樣的總時間只差 +0.33 s。
  Interstellar 的折疊時間幾乎都在 `fold_evals`（inclusive 9.45 s）。它底下依序是 `eval_g` 8.35 s → `R1CS::constraint` 5.84 s → `Row::eval` 5.02 s → `LinComb::eval` 4.93 s，
  也就是在明文中對 k+1 個點逐列計算 G(τ(t), w(t))。原版的折疊主要是 `cross_wit_acc` / `cross_terms`（1.68 s）。
- (a)：兩版的 extend 大部分都在每步的 `mux` 中觸發（29.11 / 22.26 s）。
- (b)：**Interstellar 多出來的時間，扣掉 extend 之後只剩 +0.14 s**（取樣總差距 +7.06 s，其中 extend 佔 +6.92 s）。
  - extend：Interstellar 的第二次 F61p extend 在最後驗證的 `acc.rs:75`（`hi = backend.mul(&table[i], t)`，eq 表的乘法）被觸發，佔 6.51 s。
    `acc.rs:75` 的 inclusive 6.62 s 裡，這次 extend 就佔了 6.5 s；結構相同的下一行 `acc.rs:76` 只有 0.07 s。
    原版只有在第一個私密輸入時做的那一次 F61p extend（6.44 s，Interstellar 對應的是 6.76 s）。
  - 非 extend 部分增加的：折疊 +0.28 s（`fold_evals` 0.41 s、`eval_g` 0.36 s、`R1CS::constraint` 0.24 s、`Row::eval` 0.21 s，這幾個是巢狀的 inclusive 時間，不能相加；
    這是每一步的成本，不是固定成本）、
    最後驗證 `ComittedAcc::verify` +0.19 s（行號：`acc.rs:75` 0.12、`:88` 0.09、`:76` 0.07、`:90` 0.06 s）、每步 `eq_table` +0.06 s。
    以最內層函式來看，增加最多的是 F61p `reduce`（+0.30 s）。
  - 非 extend 部分減少的：commit −0.18 s（其中 `__sendto` −0.13 s）。
  - 這些差距只有幾十到幾百 ms，而每組只跑了一次，沒有評估雜訊大小。
- (b)：兩版在 backend `finalize`（QuickSilver）觸發的 extend 都約 2.3–2.4 s；電路讀取與 R1CS 建構約 0.3 s，兩版相同。

重跑：

```bash
# (a) 電路
sed -e 's/^params_clauses  = .*/params_clauses  = [2**3]/' -e 's/^params_gates    = .*/params_gates    = [2**12]/' \
    -e 's/^params_branches = .*/params_branches = [16000]/' diet-mac-and-cheese/gen/disj-hyper-param.py > /tmp/disj-prof.py
(cd diet-mac-and-cheese/gen && PYTHONPATH=$PWD python3 /tmp/disj-prof.py f61 /tmp/circ-profile)
cargo +stable install --locked samply
# 在要測的那份 worktree 內執行（例如 (b) 原版）：
(cd ../dora-minor-revision && PWRAP="$HOME/.cargo/bin/samply record --save-only --unstable-presymbolicate -o /tmp/prof-b-orig.json.gz --" \
  caffeinate -i $OLDPWD/experiments/runcirc.sh /tmp/circ-clauses/c4096-n2000/branches_0000000001024000_2000_clauses_4096_gates_512 prof-b-orig)
# inline 展開：
dsymutil ../dora-minor-revision/target/release/dietmc_0p -o /tmp/orig.dSYM
atos -o /tmp/orig.dSYM/Contents/Resources/DWARF/dietmc_0p -arch x86_64 -l 0x100000000 -i <0x100000000 + profile 裡的位址>
```

分析腳本與 profile 放在 `~/dora-profiles/2026-10-29/`（不在 repo 裡）：

- `prof.py`：解析 samply 輸出（符號表層級）；`prof2.py`：用 `atos -i` 展開 inline frame；`classify.py`：分類並印出 self time 前 30
- `prof/prof-{a,b}-{orig,inter}.json.gz` 與 `.json.syms.json`：samply 原始 profile；`.pkl`：展開 inline 之後的樣本（`classify.py` 讀這個）；`.txt`：runcirc.sh 的輸出
- `dsym/orig.dSYM`、`dsym/inter.dSYM`：兩版執行檔的 dSYM（UUID 跟 profile 相符），`prof2.py` 用它展開 inline
- `logs/prof-{a,b}-{orig,inter}-{p,v}.log`：prover／verifier 的 log；extend 次數是從 verifier log 的 `SVOLE<…>` 行數出來的
- `profile-run.sh`：跑這 4 次 profile 的腳本（裡面的輸出路徑仍是當時的 scratchpad 和 `/tmp/dora-bench-profile`，重跑前要改）
- 在該資料夾內執行 `python3 classify.py prof-a-orig prof-a-inter prof-b-orig prof-b-inter` 即可重現上面的表格；
  `prof2.py` 從 `.json.gz` 重新展開的結果與存好的 `.pkl` 相同（已用 prof-b-orig 確認）。

## 已知可改進

- 最後驗證：每條指令建 eq 表多約 2^(k+1) 個乘法，最後驗證約為原版的 2 倍，指令數多時明顯。
  每層 `lo = table − hi` 可省一半乘法。
- 只用了 degree 2 約束，Interstellar 的高次數約束尚未使用（可縮小 \|w\|，連帶縮小 accumulator 的重新 commit）。

## 尚未做

- 論文規模的參數（最多 2^15 條指令、每條 2^15 gate、50,000 步）在 x86 實體機上的正式量測
- 時間的 profiling（VOLE、witness 計算、折疊運算各佔多少）
- 指令隱藏的自動化測試（目前是靠結構保證：每步 commit 的數量只跟整組指令有關）
- 形式化安全證明
