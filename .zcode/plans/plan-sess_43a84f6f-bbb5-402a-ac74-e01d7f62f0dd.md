# Merge ekranının diff-kit özelleştirmesi — runtime toggle'lar

## Context

Merge editörü (ekrandaki üç panelli görünüm) warp-station'da zaten yaşıyor ve Current/Incoming panelleri suite'in diff kitini (`render_aligned`) kullanıyor; seçim kutuları kitin opt-in `lead` kancasıyla geliyor. Bu görev, kalan merge özelliklerini aynı ilkeyle kitle taşımayı tamamlar: **her yeni yetenek `AlignedOptions` üzerinde default-kapalı bir runtime toggle olur**; yalnızca merge ekranı açar, Changes/History/PR/Azure gibi diğer diff yüzeyleri tek satır değişmeden kalır (onlar `render_diff_with_scroll` kullanıyor, `render_aligned`'ı hiç çağırmıyor). Cargo feature seçilmedi: aynı app build'inde ekran başına ayrılamazdı; `DiffOptions.mode/wrap` ve `lead` zaten bu deseni kuruyor.

Kapsam (onaylanan): (1) toggle mekanizması, (2) panellerde sözdizimi renklendirme, (3) planın açık işleri (base pane, kısayol/`?` satırı; "blok içi gölgeleme" maddesi word-emphasis ile çözüldü sayılıp kapanır), (4) app'in çizdiği parçaların kite taşınması.

**Kirli ağaç notu:** `aligned.rs`'te işlenmemiş bir inline-emphasis düzeltmesi var. Aşağıdaki 1. faz o düzeltmeyi evrensel hale getirir (div-parçaları yerine tek `StyledText`): düzeltme revert edilmez, süpersede edilur; commit'ler sadece dokunulan yolları stage eder.

## Değişiklikler

### 1. Kit — toggle mekanizması + kanonik text run (kits/warpui-kit-diff/src/aligned.rs)

- `render_aligned(rows, options)` imzasına `window: &mut Window, cx: &App` eklenir (kitin diğer render'ları `render_diff_with`/`render_code` zaten böyle; `Theme::global(cx)` için gerekli). Tek call site merge.rs — aynı değişiklikte güncellenir.
- `code()`'un div-parçalı emphasis düzeni, kitin her yerde kullandığı tek düzenle değiştirilir: `StyledText::new(text).with_default_highlights(style, merged_runs(len, tokens, emphasis, bg))` — emphasis + token renkleri tek inline layout. `merged_runs` (lib.rs:1615) ve `line_tokens` (lib.rs:1676) `pub(crate)` yapılıp yeniden kullanılır.
- `AlignedOptions` yeni alanlar: `syntax: Option<&'static str>` (dil anahtarı, `lang_from_path`'ten; None = bugünkü düz renk), `base: bool` (faz 4), `emphasis: bool` (default açık; `paired_word_emphasis`'in `show` parametresi zaten var). Default'lar bugünkü görünümü korur; `AlignedOptions::new(font_detail)` kurucusu call site'ları kısa tutar.
- `AlignedRow`'a `base: Option<(u32, &str)>` eklenir (faz 4).

### 2. Kit — sözdizimi toggle'ı (aligned.rs)

- `syntax` açıkken: penceredeki her tarafın satırları tek buffer'da birleştirilip `highlight_buffer` (src/highlight.rs:30, cache'li) ile stil lenir, `slice_runs` ile satır başına düşürülür — `render_code`'un (lib.rs:1210-1217) kullandığı akışın aynısı. Tree-sitter grammar'ları app zaten `all-languages` ile açıyor; diğer yüzeylerin davranışı değişmez.

### 3. Kit — merge parçaları kite taşınır (aligned.rs)

- `pub fn aligned_pane_head(entries: &[(&str, usize)], lead: bool) -> AnyElement` — merge.rs:1386 `pane_head`'in ("Current · 127 lines", micro/semibold/muted, 28px, hairline) kit versiyonu; `PICK_W` yerine kitin `LEAD_W`'si.
- `pub fn aligned_pick_box(checked: bool) -> AnyElement` — merge.rs:1418 `pick_box`'ın kit versiyonu (14px kare, palette accent/on-accent; check glifi code fontuyla "✓", asset altyapısı eklenmez). App'in `lead` closure'ı sadece state/on_click'i sarar.
- `pub fn visible_window(total, offset, viewport, row_h)` — merge.rs:1255'teki pencere matematiği `row_height`'ın yanına, kitin `padding` sözleşmesinin sahibi olarak kite.
- `impl AlignedSide { pub fn wash(self) -> Hsla }` — merge.rs:1323 `result_decorations`'daki Ours→`DIFF_DEL_BG` / Theirs→`DIFF_ADD_BG` eşlemesi.

### 4. Base pane (açık iş) — core + kit + app

- **Core** (warp-station-core/src/git/merge.rs): `TextMerge.base: Vec<String>` ve `MergeRow.base: Option<u32>`; mevcut `ours_base/theirs_base` hizalama omurgası (merge.rs:656+) ve `ConflictBlock.base_text` üzerinden üretilir. `merge` stili çatışmalarda base bölümü yoktur → satırda `base: None` (birim testler: merge, diff3, zdiff3 fixture'ları — `tests/merge_conflicts.rs` desenini izler).
- **Kit**: `base: true` iken üç sütun [Left | Base | Right], aralar hairline; base satırları nötr `DIFF_HUNK_BG` ile yıkanır (yeni palette rengi gerekmez); boş base tarafı bugünkü boş hücreyi çizer. `lead` yalnız Left/Right'ta kalır.
- **App** (src/ui/merge.rs): toolbar'a base-pane toggle'ı (icon button, a11y adı + `debug_selector`; tasarım sistemi "Controls" bölümü kuralı) — `AlignedOptions.base`'i per-render çevirir.

### 5. App tüketimi + kalan açık işler (apps/warp-station/src/ui/merge.rs)

- Call site güncellenir: `syntax: lang_from_path(&file.path)`, kit `pane_head`/`pick_box`/`visible_window`/`side.wash()` kullanılır. Probe selector'ları (`merge-pick-*`, `merge-pane-row`) kit versiyonlarında korunur ya da probe'lar aynı commit'te güncellenir.
- `?` ipucu satırı + `shortcuts::dialog` Merge grubuna yeni girdiler (base toggle, Alt+↑/↓ zaten var).

### 6. Testler

- **Kit**: yeni `kits/warpui-kit-diff/tests/aligned_probe.rs` (bugün aligned.rs için hiç probe yok): default çağrı lead sütunu çizmez; `lead` varken çizer; `padding` gövde yüksekliğini tam tutar (`aligned-panes` yüksekliği = satır sayısı × `row_height`); `syntax` açık/kapalı iki durum da çizilir; `base` üçüncü gutter ekler; `aligned_pane_head`/`aligned_pick_box` varlık kontrolleri. Mevcut probe desenleri (`debug_bounds`, `Render` host, `TextSelectionLayer`) izlenir.
- **App**: `tests/merge_probe.rs` genişletilir (base toggle aç/kapa, kit pane head); diğer diff probe'ları (`diff_mode`, `split`, `diff`) değişmeden geçmeli — "diğer ekranlar etkilenmez" güvencesinin kanıtı budur.

### 7. Doküman + teslim

- `plans/merge-editor.md` durum paragrafı (bugün gövdenin gerisinde) yeniden yazılır; §7'den "satır gölgeleme" kapanır (word-emphasis çözdü), base pane ve `?` satırı landed işaretlenir.
- `kits/README.md` tablosu ve kitin kendi README'sine aligned görünümü + toggle'lar eklenir; `docs/agents/design-system.md` diffs bölümüne kısa bir aligned paragrafı (opt-in toggle ilkesi, base yıkaması `DIFF_HUNK_BG`).
- Kapılar: `cargo fmt --all`; altı kit üzerinde clippy `-D warnings`; `cargo test` altı kit; `cargo test -p warp-station -p warp-station-core`.
- Teslim penceresi: yeni kit tag'i + warp-station `Cargo.toml` pin bump + `suite.toml` kit tag'i (plans/merge-editor.md'nin not ettiği kural).

## Dokunulmayanlar

`render_diff*` imzaları ve Changes/History/PR/Azure call site'ları hiç değişmez; `warp-station-core`'a gpui girmez; kirli ağaçtaki ilgisiz değişiklikler olduğu gibi kalır.

## Verification

1. Kit kapıları ve `cargo test -p warpui-kit-diff` (yeni aligned probe dahil).
2. `cargo test -p warp-station -p warp-station-core` — merge probe genişletilmiş haliyle; diff probe'ları değişikliksiz geçer.
3. Manuel: `scripts/dev/link-apps.sh` sonrası warp-station'ı çakışmalı bir fixture repo ile aç (merge probe'unun fixture deseni / `WARP_STATION_PAGE` kancası), panellerde token renklerini, base toggle'ı ve seçim kutularını doğrula; Changes sekmesinin diff'inin aynı kaldığına bak.