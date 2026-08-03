# 效能基準線

> 版型與量測流程由 [`specs/E1-perf.md`](./specs/E1-perf.md) 定義，本檔只放**數字**。
> 每次量測**追加一列而非覆寫**——歷史值就是回歸偵測。
>
> 狀態：**空表**（2026-08-03 建立）。裝置未到位，所有數字待實測。

## 量測條件

每次量測前逐條確認，不靠記憶（`E1-perf §1`）：

- [ ] 電量 > 50% 且**未充電**
- [ ] 飛航模式
- [ ] 重開 App
- [ ] 關閉低耗電模式
- [ ] 螢幕亮度固定 50%
- [ ] 距離上一輪量測已靜置 **5 分鐘**（熱節流會讓第二輪差 20% 以上）

`m2p` 欄一律填**5 趟的中位數**，不是最小值——最小值是運氣，使用者感受到的是分佈。

---

## \<高階機型\>｜\<iOS 版本\>

ProMotion 120 Hz。`preferredFrameRateRange` 與 `maximumDrawableCount` 的效果只在這裡看得出來。

| 日期 | commit | m2p 慢速 (ms) | m2p 快速 (ms) | m2p 起筆 (ms) | frame p99 (ms) | Pass 1 (ms) | Pass 3 (ms) | 記憶體 peak (MB) | 變更 |
|---|---|---|---|---|---|---|---|---|---|
| | | | | | | | | | |

## \<中階機型\>｜\<iOS 版本\>

60 Hz、A13–A15 世代。真實下限——一格 16.7 ms，latency 絕對值明顯較差。

| 日期 | commit | m2p 慢速 (ms) | m2p 快速 (ms) | m2p 起筆 (ms) | frame p99 (ms) | Pass 1 (ms) | Pass 3 (ms) | 記憶體 peak (MB) | 變更 |
|---|---|---|---|---|---|---|---|---|---|
| | | | | | | | | | |

**「可重複」的證明**（`E1-perf §10`）：m2p 要在**不同日**量兩次，中位數差異 < 15%。

---

## 對帳：`architecture.md §4.1.1`

三步，缺一不可。**第 1 步即使總額沒超標也必須做**——預算表漏兩筆是已知事實。

| 步驟 | 內容 | 數字 |
|---|---|---|
| 1 | 實測 swapchain drawable，回填 `§4.1.1` | 待實測（估 24–36 MB） |
| 1 | 實測 `region_ids` CPU 副本，回填 `§4.1.1` | 待實測（估 8 MB） |
| 2 | 加上 E3 undo pool 的估算值（`§4.1.1` 已有該列） | 64 MB |
| 3 | **對修正後的總額判定**。超標 → 此時調畫布解析度 | 待判定 |

E1 的劇本：開最大畫布 → 連續塗抹 30 秒 → 連填 20 個區域（動畫全開）→ 切出 App 再切回
（順便驗 `detach` 不釋放資源）。**E1 沒有 undo pool**，不照 `§13.1` 的原劇本量。

---

## 調校記錄

`E1-perf §7` 的七項。**一次只調一個**，否則效果混在一起分不出是誰的功勞。

| 日期 | 項目 | 舊值 | 新值 | 判斷依據 |
|---|---|---|---|---|
| 2026-08-03 | （初值登記，未調校） | — | 見下 | — |

初值與各自的出處：

| 項目 | 初值 | 出處 |
|---|---|---|
| One-Euro 位置 `min_cutoff`／`beta`／`d_cutoff` | 1.0／0.05／1.0 | `stroke::OneEuroParams::POSITION` |
| One-Euro radius 同上三項 | 0.5／0.0／1.0 | `stroke::OneEuroParams::RADIUS` |
| `R_EPS` | 4.0 點 | `stroke::R_EPS` |
| `TIP_FALLOFF` | 1.0（線性衰減） | `render::TIP_FALLOFF` |
| `FILL_EDGE` | 24 px | `shaders/composite.wgsl` |
| 動畫時長 | 180 ms ease-out | `render::FILL_DURATION` |
| `maximumDrawableCount` | 2 | `EngineCanvasView.swift` |
| `MAX_DABS_PER_DRAW` | 4096 | `stroke::MAX_DABS_PER_DRAW` |

> `TIP_FALLOFF` 是 Pass 1 落地時才出現的第八項（`E1-stroke §6.1` 的筆尖是程序生成的，
> 衰減曲線因此是個真的參數）。它直接決定筆跡邊緣的軟硬。

---

## D2／D3／D4

| 檢查點 | 狀態 | 結論 |
|---|---|---|
| D2 · `R16Uint` 在低階 Android | 未執行 | — |
| D3 · 主觀手感盲測（外部三人、手指、不得自評） | 未執行 | — |
| D4 · Mask Mode A／B | 未執行 | — |

D3 的通過條件比看起來硬：三人皆答「跟手」，且**沒有人主動提到延遲**。
D4 的產出是產品決策，寫回 `prd.md §4.1` 並結案附錄 A6。
