# Warp Suite — mimari, davranış ve performans denetimi

**Tarih:** 5 Kasım 2026
**Kapsam:** `apps/warp-station`, `apps/warp-agent`, `apps/warp-notes`, `apps/warp-snapit` ve `kits/warpui-kit-{core,platform,markdown,diff,picker}`
**Yöntem:** kaynak okuma (yaklaşık 94 bin satır Rust), derleme kapıları, testler ve çapraz-uygulama sözleşmelerinin karşılaştırılması. Ölçüm için `cargo fmt`, `cargo clippy -D warnings`, `cargo clippy -W pedantic` ve `cargo test` çalıştırıldı; **çalışan uygulamada profil çıkarılmadı**, performans bulguları kod yolunun karmaşıklığına ve çağrı sıklığına dayanır.

## Özet

Suite'in genel mimarisi sağlam: ağ işi UI thread'inden ayrılmış, store olay güdümlü, iki uygulama arasındaki kapı (door) tek bir sözleşme modülünden üretiliyor, dosya yazımları atomik, alt süreçlerin çoğu zaman aşımlı ve çıktı sınırlı. `cargo fmt`, `clippy -D warnings` ve 654 testin tamamı geçiyor.

Buna karşılık üç sınıf sorun öne çıkıyor:

1. **Sessiz veri kaybı riski** — warp-notes açık notun dışarıda değişip değişmediğini hiç kontrol etmiyor; çoklu örnek (uygulamanın kendi belgelediği bir kullanım) veya harici düzenleme, otomatik kayıtla eziliyor.
2. **Kalıcı "yükleniyor" durumları** — Warp Station'da kapalı PR/issue listesi bir kez başarısız olursa ya da bir PR merge edilirse istek koruması (`closed_requested`) cevap/merge sonrası temizlenmediği için liste sonsuza kadar "Loading…" kalıyor; wand çalışması iptalden sonra cevap gelmezse süresiz asılı kalıyor.
3. **Ölçekte UI ve ağ tıkanması** — Warp Station'ın tek "net" komut şeridi ağır fetch/probe geçişlerinin arkasında kullanıcı komutlarını bekletiyor; seçim değişiklikleri debounce edilmediği için ok tuşuyla gezinmek eşzamanlı tam git okumaları başlatıyor; warp-agent'ta 25–40 s sürebilen sunucu başlatma aynı şeridi kilitliyor; markdown editörü her tuş vuruşunda belgeyi iki kez baştan ayrıştırıyor.

Aşağıda bulgular önem sırasına göre; her biri dosya:satır kanıtı ve küçük bir düzeltme önerisiyle verilmiştir. "P0" canlıda veri kaybı/asılı kalma, "P1" yanlış davranış veya normal ölçekte belirgin yavaşlama, "P2" yük/ölçek altında sorun ve mimari borç anlamına gelir.

## Ölçümler

| Ölçü | Değer |
|---|---|
| Rust satırı (target hariç) | 94.406 |
| En büyük modüller | `apps/warp-agent/src/ui/tile.rs` 4.905, `apps/warp-agent/src/ui/window.rs` 3.092, `kits/warpui-kit-markdown/src/editor/state.rs` 3.079, `kits/warpui-kit-markdown/src/live.rs` 2.995, `apps/warp-station/crates/warp-station/src/store/mod.rs` 2.814 |
| Cargo lock paketi | 953 |
| Test | 654 geçti, 0 başarısız (46 test ikilisi) |
| `cargo fmt --all --check` | temiz |
| `cargo clippy --workspace --all-targets -- -D warnings` | temiz |
| `cargo clippy -W clippy::pedantic` | 1.720 uyarı; incelenenler stil/must_use/doc ağırlıklı, `Duration` çıkarma uyarılarının ikisi de korumalı |

## Mimari harita

- **warp-station**: `warp-station-core` (gpui'siz, test edilebilir: git, GitHub, Azure, NuGet, süreç, ayarlar, yollar) + `warp-station` (GPUI, `net.rs` ağ thread'i, `store/` durum ve projeksiyon, `ui/`). Ağ tarafı tek OS thread + 2 işçili tokio runtime; komutlar `async_channel` ile, olaylar ters yönde akıyor.
- **warp-agent**: tek crate. `oc.rs` opencode REST/SSE istemcisi ve ağ thread'i, `dsh.rs` DeepSeek Harness WebSocket istemcisi + opencode olay çevirisi, `ipc.rs` loopback kapısı, `store/` sohbet durumu, `ui/` döşeme ızgarası. Kapı sözleşmesi `warpui-kit-platform/src/door.rs` içinde iki tarafça paylaşılıyor.
- **warp-notes**: `notes_store` (dosya CRUD) + `ui/notes.rs` (ağaç, editör, otomatik kayıt) + `ui_prefs`. Markdown düzenleme `warpui-kit-markdown`'da.
- **warp-snapit**: Windows'a özgü motor (`cfg(windows)`), platformdan bağımsız `zones`/`geometry`/`settings` modülleri Linux'ta test ediliyor.
- **Kits**: `core` (palet, tipografi, shell, ölçek, ayar satırları), `platform` (yollar, atomik yazım, kapı, not arama), `markdown` (canlı editör + ayrıştırıcı), `diff`, `picker`.
- **Süreçler arası**: warp-agent kapısı (`~/.warp/agent/ipc.port|ipc.token`, NDJSON, token'lı), warp-notes başlatma argümanları, `~/.warp/ui.json` (metin boyu + palet), `~/.warp/snapit` ayarları, `~/notes` deposu.

---

# P0 — veri kaybı ve kalıcı asılı kalma

## A1. warp-notes dışarıdan değişen notu habersiz eziyor (veri kaybı)
**Yer:** `apps/warp-notes/src/ui/notes.rs:390-450`, `:505-522`, `:528-543`; `apps/warp-notes/src/notes_store.rs:96-103`
**Sorun:** Not açılırken dosya bir kez okunuyor (`baseline`), otomatik kayıt 1 s sonra editördeki metni koşulsuz `write_atomically` ile dosyanın üzerine yazıyor. Ne zaman damgası (mtime), ne boyut, ne de iyimser kilit kontrolü var; `on_app_quit` ve pencere kapatma da aynı şekilde çakışma kontrolü yapmadan yazıyor. Uygulama kendi belgelerinde "multi-instance is free" diyor (`apps.toml`).
**Etki:** İki warp-notes penceresi aynı notu açtığında, ya da not harici bir editörde/git pull ile değiştiğinde, son yazan diğerinin değişikliğini sessizce siler. Bu bir not uygulaması için en ağır hata sınıfı.
**Düzeltme (öneri):**
- `open_note` sırasında `(mtime, len)` sakla; `save_current`/`flush_saves` öncesi `fs::metadata` ile karşılaştır, farklıysa yazma ve kullanıcıya "dosya dışarıda değişti" seçeneği sun (yeniden yükle / üzerine yaz / farklı kaydet).
- En azından çoklu örnek için `OpenOptions::new().create_new(true)` tabanlı bir `.lock` dosyası veya salt-okunur mod.
- `flush_saves` başarısız yazımda `dirty`'yi `mem::take` ile temizlemek yerine koru (kurtarma dosyasına yaz).

## A2. Kapalı PR/issue listesi kalıcı olarak "Loading…" kalabiliyor
**Yer:** `apps/warp-station/crates/warp-station/src/store/mod.rs:1719-1737` (`closed_requested`), `:624-636` (cevap), `:672` (merge sonrası önbellek temizliği), `:2303` (yalnız Refresh temizliği); `ui/pr_issues.rs:351-354`, `:362`, `:398-400`
**Sorun:** `load_closed` isteği `closed_requested` kümesine ekliyor ve bu anahtar yalnızca `refresh_panel` (Refresh düğmesi) çalıştığında siliniyor; cevap geldiğinde (başarı ya da hata) küme güncellenmiyor. Merge başarılı olduğunda `github_closed` önbelleği siliniyor ama istek koruması silinmediği için bir sonraki açılışta istek atılmıyor; `closed_items == None` olduğundan görünüm sonsuza kadar "Loading closed Pull requests…" gösteriyor. `gh` geçici olarak hata verdiğinde de aynı durum kalıcı.
**Etki:** PR merge ettikten sonra kapalı PR sekmesi, uygulama yeniden başlatılana veya Refresh'e basılana kadar kullanılamaz; geçici bir `gh` hatası aynı şekilde kalıcılaşır.
**Düzeltme:** `load_closed` çağrısını render'dan çıkarıp `set_closed`/sekme açılışına taşı (render her karede çağırdığı için anahtarı başarıda silmek sonsuz istek döngüsü yapar). Sonra: başarıda anahtar kalsın (önbellek geçerli); hatada anahtarı sil ve görünüme "yüklenemedi — Yeniden dene" durumu ekle; merge'de hem `github_closed` önbelleğini hem anahtarı sil, tek yeniden istek bir sonraki sekme açılışında gelsin. Alternatif: `github_closed` için TTL tut.

## A3. Wand iptalden sonra cevap gelmezse süresiz asılı kalıyor
**Yer:** `apps/warp-station/crates/warp-station-core/src/warp.rs:249-297`
**Sorun:** `run_prompt` normalde `RUN_CAP` deadline'ı ile korunuyor. Ancak iptal sinyali geldiğinde `cancelled = true` yapılıyor ve `tokio::select!` içindeki iki kol da `if !cancelled` koşuluyla **devre dışı** kalıyor; geriye yalnızca `read_frame()` kalıyor ve sonrasında hiçbir zaman aşımı yok. Kapı (door) iptal edilen koşuya cevap vermezse (agent kilitlenmesi, yarım kalmış bağlantı), future sonsuza kadar bekler.
**Etki:** Koşu hiç bitmez; `wands` haritasındaki kayıt ve görev sızar, UI'daki "busy" durumu kalıcı olur, ikinci Cancel da işe yaramaz (kill kanalı artık dinlenmiyor).
**Düzeltme:** İptalden sonra ayrı bir `CANCEL_GRACE` (ör. 30 s) deadline'ı kur; dolduğunda `Err("cancel timed out")` dön ve kaydı temizle. `cancelled` bayrağı yerine ikinci bir `select!` dalı kullan.

## A4. Ortak `save_generation` sayacı, debounce edilmiş etiket kaydını iptal edebiliyor
**Yer:** `apps/warp-station/crates/warp-station/src/store/mod.rs:183`, `:2360-2379` (`persist_tags_soon`), `:964-965` (`run_summary`), `:1054-1055` (`run_wand`)
**Sorun:** Aynı sayaç hem "repo önbelleği yazım nesli" hem de wand/summary kimlik üreteci olarak kullanılıyor. Yıldız/etiket tıklaması `generation = N` ile 400 ms'lik bir yazım zamanlıyor; bu süre içinde bir wand veya summary çalıştırılırsa sayaç `N+1` oluyor, zamanlayıcı `store.save_generation == generation` kontrolünde eşleşmeyip **önbellek yazımını atlıyor**. Ayrıca uygulama kapanışında bekleyen yazım hiç flush edilmiyor.
**Etki:** Yıldız/favori/etiket değişikliği, araya giren bir AI çalıştırması veya 400 ms içinde kapatma ile kaybolabiliyor (bir sonraki tarama önbelleği yeniden yazana kadar).
**Düzeltme:** `cache_save_generation` ile `next_id`/`id_seq` sayaçlarını ayır; kapanışta `flush_pending_cache()` çağır.

---

# P1 — yanlış davranış ve belirgin performans sorunları

## B1. Warp Station tek komut şeridi: kullanıcı işlemleri ağır geçişlerin arkasında bekliyor
**Yer:** `apps/warp-station/crates/warp-station/src/net.rs:765-810` (Scan/ProbeAll/FetchAll ana döngüde `await`), `:617-722` (yalnız 5 okuma tipi öne alınıyor), `:1178-1228` (LoadHistory/GetCommitDetails/GetChangePatch ana döngüde), `:1229-1257` (Git mutasyonu)
**Sorun:** `FetchAll` (3 eşzamanlı `git fetch`) ve `ProbeAll` (4 eşzamanlı status) ana komut döngüsünde bekleniyor. Yalnızca `LoadChanges`/`GitHub*` okumaları `spawn_read_intake` ile öne alınmış. Bir otomatik fetch 100 repoda dakikalarca sürebilirken History sekmesi, commit detayı, dosya patch'i ve hatta kullanıcının Commit/Pull/Push eylemi kuyrukta bekliyor.
**Etki:** "Tıkladım, hiçbir şey olmadı" hissi; eşzamanlı fetch sırasında commit drill-down ve stage/discard işlemleri gecikiyor. UI'da kuyruk/ilerleme göstergesi yok.
**Düzeltme:** Komutları öncelik sınıflarına ayır (kullanıcı eylemleri > arka plan geçişleri) veya tüm okumaları intake'e taşı; ağır geçişleri tek bir "background lane" semaforuna al ve UI'ya "fetch sürüyor, işleminiz kuyruğa alındı" durumu göster.

## B2. Seçim değişikliği debounce edilmiyor: ok tuşuyla gezinmek eşzamanlı tam git okumaları başlatıyor
**Yer:** `apps/warp-station/crates/warp-station/src/store/mod.rs:1651-1662` (`select_repo`), `:2125-2151` (`step_selection`); `apps/warp-station/crates/warp-station/src/net.rs:630-639` (her istek için görev), `:571-591` + `apps/warp-station/crates/warp-station-core/src/git/read.rs` (`change_snapshot` + `branches` + `recent_commits` + `unpushed_hashes` = 4 ayrı `Repository::discover/open`)
**Sorun:** Intake her `LoadChanges` için bir görev açar; görev önce **okumayı yapar**, sonra "en yeni mi" kontrolüyle sonucu atar (`apps/warp-station/crates/warp-station/src/net.rs:634-639`). Yani 20 satır aşağı inmek, 20 eşzamanlı tam çalışma alanı okuması (her biri status + iki diff + branch + revwalk) demek. Ayrıca `select_repo` her adımda `update_settings` ile tüm ayar dosyasını senkron yazıyor.
**Etki:** Büyük repolarda ok tuşuyla gezinme net thread'i ve disk G/Ç'sini doldurur; gereksiz `git status` fırtınası. UI tarafında her adımda senkron JSON yazımı (~1 ms + fsyncsiz rename) birikir.
**Düzeltme:**
- `select_repo` için 150–250 ms debounce (GitHubDetails zaten 250 ms debounce'lu); yalnız son isteği gönder.
- `workspace_loaded`'ı okumadan **önce** `is_newest` kontrolüyle erken çık (veya iptal token'ı).
- `selected_repo` yazımını `persist_tags_soon` gibi debounce et; `save_state` benzeri tek bir flush.
- `change_snapshot`/`branches`/`recent`/`unpushed` dört ayrı açılış yerine tek `Repository` açılışında toplanabilir (`with_repo` bir kez).

## B3. warp-agent: sunucu başlatma tüm ağ döngüsünü 25–40 s kilitliyor
**Yer:** `apps/warp-agent/src/oc.rs:1211-1251` (`net_main` içinde `start_workspace(...).await`), `:1379-1412`, `:255-294` (25 s opencode / 40 s dsh beklemesi)
**Sorun:** `NetCommand::StartServer` ana komut döngüsünde bekleniyor. Bu süre boyunca `StopServer`, diğer çalışma alanlarına `SendMessage`, `Cancel`, kapıdan gelen `models/status` istekleri kuyrukta bekliyor. Kullanıcı yeni bir çalışma alanı açarken mevcut sohbetlerde mesaj gönderemiyor ve **başlatmayı iptal edemiyor** (StopServer sıraya giriyor).
**Etki:** Yavaş açılan bir dsh sunucusu (40 s) tüm uygulamanın ağ trafiğini dondurur; kapı istemcileri 60 s cevap süresiyle "timed out" alabilir.
**Düzeltme:** Başlatmayı ayrı bir göreve al ve çalışma alanını `Starting` durumunda tut; `StopServer` için bir iptal kanalı ekle; `select!` ile başlatmayı iptal edilebilir yap.

## B4. warp-agent: kapanışta `opencode serve` / `dsh web` süreçleri sahipsiz kalabiliyor
**Yer:** `apps/warp-agent/src/main.rs:176-184` (`shutdown()` + hemen `cx.quit()`), `apps/warp-agent/src/store/mod.rs:1465-1473`, `apps/warp-agent/src/oc.rs:297-303` (`ServerProcess::stop`), `:204` (`kill_on_drop`)
**Sorun:** `shutdown()` yalnızca net thread'e `StopServer` komutları gönderiyor; `cx.quit()` aynı anda süreci sonlandırıyor. Net döngüsü komutları işleyemeden süreç çıkarsa `kill_on_drop(true)` hiç çalışmaz (drop edilmez) ve alt süreçler OS tarafından evlat edinilip yaşamaya devam eder. opencode her açılışta rastgele boş port seçtiği için bu süreçler birikir; dsh her açılışta yeniden spawn edildiği için asıl sızıntı orada.
**Etki:** Yeniden başlatmalar arasında yetim sunucular, portlar ve bellek birikir; dsh bağlantıları eski süreçlere düşebilir.
**Düzeltme:** `on_window_closed` içinde `shutdown()` sonrası net thread'in durmasını bekle (join/ack kanalı), ardından `cx.quit()`; ayrıca çıkışta net thread'e senkron bir "kill all children" yolu ekle. Alternatif: çocuk süreçleri bir iş nesnesi (Windows Job Object) veya `prctl(PR_SET_PDEATHSIG)`/parent-watch ile bağla.

## B5. dsh pompası: sınırlı (64) `ws_tx` kanalına pompa içinden `send_blocking` → kendi kendini kilitleme
**Yer:** `apps/warp-agent/src/oc.rs:1421` (`bounded::<String>(64)`; `:1415-1429` kanalı kuran dsh dalı), `apps/warp-agent/src/dsh.rs:453-488` (`reopen_streams` → `send_ws_frame`), `:353-359` (`send_blocking`), `:1672-1708` (pompa ancak bundan sonra `outbound`'u boşaltıyor)
**Sorun:** `reopen_streams` yeniden bağlanma sırasında `1 + takip edilen oturum` kadar çerçeveyi **pompa görevinin kendi içinden** kanala yazıyor. Kanal 64 kapasiteli ve pompa bu sırada `outbound` tarafını henüz okumuyor (okuma döngüsü `reopen_streams` bittikten sonra başlıyor). 64'ten fazla takipli oturum varsa ya da bağlantı kopukken kanal dolduysa 65. `send_blocking` sonsuza kadar bloklar: pompa ilerleyemez, dsh tamamen ölür.
**Etki:** Yoğun kullanımda (çok sayıda dsh sohbeti) veya uzun bir WS kesintisinden sonra dsh akışı kalıcı olarak durur; yeniden bağlanma da çalışmaz.
**Düzeltme:** `reopen_streams`'u `async` yapıp `send().await` kullan veya `try_send` ile kuyruğu boşalt/at; yeniden bağlanmadan önce `outbound`'u boşalt; kapasiteyi takip edilen oturum sayısına göre ayarla.

## B6. Markdown editörü her tuş vuruşunda belgeyi iki kez baştan ayrıştırıyor
**Yer:** `kits/warpui-kit-markdown/src/editor/state.rs:1112-1115` (`reparse`), `:1054-1108` (revizyon değişince **tüm** satır görünümleri yeniden), `kits/warpui-kit-markdown/src/highlighter.rs:173-189` (`text.to_string()` + `live::parse`), `:191-240` (`styles` her çağrıda span listesini baştan tarıyor); `kits/warpui-kit-markdown/src/editor/ops.rs` içinde Enter/soft-break vb. her biri ayrıca `parse_document`
**Sorun:** Her düzenlemede (a) editör durumu tüm belgeyi ayrıştırıyor, (b) gpui-base `drive_highlighter` her düzenlemede `InputHighlighter::update` çağırıyor ve o da tüm belgeyi ikinci kez ayrıştırıyor, (c) `refresh_views` tüm satır görünümlerini yeniden kuruyor, (d) kod blokları için tree-sitter tokenizasyonu yenileniyor. Artımlı yol yok.
**Etki:** 100–500 KB'lık bir notta her tuş vuruşu doğrusal (ve sabit çarpanlı) iş; büyük notlarda yazma gecikmesi ve CPU artışı. Notlar genelde küçük olduğu için bugün çoğu kullanıcıda hissedilmez; ölçek sınırı düşük.
**Düzeltme:** Ayrıştırılmış `LiveDocument`'ı yüksekleyiciye paylaş (ikinci `parse` yerine aynı span'ler); yalnız değişen satır aralığını yeniden ayrıştır (satır bazlı önbellek + blok sınırı); `refresh_views`'u etkilenen satırlarla sınırla; `styles` içinde `partition_point` ile span aramasını logaritmik yap.

## B7. warp-agent transkripti sanallaştırılmamış; her karede tüm öğeler için anahtar üretiliyor
**Yer:** `apps/warp-agent/src/ui/tile.rs:3815-3845` (tüm `items` için `(id, Arc)` toplama), `:3980-4023` (`for` döngüsünde her öğe için `render_item` + `format!` anahtarları + `HashSet`), `:1229-1240`, `:1338` (`render_diff`/`render_code` satır başına div), `kits/warpui-kit-diff/src/lib.rs:85-140`
**Sorun:** Transkript için `uniform_list`/sanallaştırma yok; döşeme yeniden çizildiğinde (akış sırasında 10 fps'e throttle edilmiş olsa da) tüm sohbet öğeleri için eleman ağacı ve `format!("{id}:title")` gibi anahtarlar yeniden üretiliyor. `render_diff`/`render_code` satır sayısına göre div üretiyor; araç input/output metni 4 KB'de sınırlı (`store/parsing.rs:112-120`) ama `metadata.filediff.patch`'ten gelen `tool.diff` (`store/parsing.rs:222-227`) sınırsız, yani kullanıcı çok büyük bir diff'i açtığında on binlerce satır elemanı oluşabilir.
**Etki:** Uzun sohbetlerde bellek/CPU kullanımı doğrusal büyür; "details" açılan büyük bir diff veya kod bloğu arayüzü saniyelerce dondurabilir. `tiles_waiting()`/`is_waiting()` de her çağrıda tüm öğeleri tarıyor (`apps/warp-agent/src/store/mod.rs:986-999`, `apps/warp-agent/src/store/tile.rs:71-78`).
**Düzeltme:** Transkripti `uniform_list`/sanal listeye geçir (öğe yüksekliği sabit değilse `list` + ölçüm önbelleği); diff/kod için satır üst sınırı ve "tümünü göster" düğmesi; `format!` anahtarlarını kalıcı `ElementId`/hash'e çevir; bekleyen kart sayısını olay anında güncellenen bir sayaçta tut.

## B8. Warp Station repo listesi de sanallaştırılmamış ve satır başına tahsis yapıyor
**Yer:** `apps/warp-station/crates/warp-station/src/ui/repos_list.rs:78-150`
**Sorun:** Her karede tüm projeksiyon satırları için eleman kuruluyor; her satır için `launchers(&settings, repo).into_iter().next()` ile bir `Vec` üretilip ilk öğe alınıyor, `repo_tabs.iter().any(...)` ve `to_string_lossy()` tekrarlanıyor. `settings()` her çizimde tüm `AppSettings`'i klonluyor.
**Etki:** Yüzlerce repoda her store bildiriminde (status, GitHub, toast vb.) yüzlerce gereksiz tahsis; `ReposPage` store gözlemcisi ayrıca her bildirimde `ensure_row_selected` çağırıyor (`apps/warp-station/crates/warp-station/src/ui/repos.rs:65-80` → `apps/warp-station/crates/warp-station/src/store/mod.rs:2164-2215`, repolar üzerinde `to_string_lossy` taraması).
**Düzeltme:** Repo listesini `uniform_list`'e al; satır başına birincil başlatıcıyı projeksiyonda/önbellekte hesapla; `ensure_row_selected`'i yalnız `ScanSettled`/`changes` gibi ilgili olaylarda çalıştır (gözlemciyi koşullu yap).

## B9. Ayar kaydı her seferinde tüm repolarda GitHub + Azure taraması başlatıyor
**Yer:** `apps/warp-station/crates/warp-station/src/ui/settings.rs:362` (`save`), `:503-519` (koşulsuz geçişler + `autostart::reconcile`), `apps/warp-station/crates/warp-station-core/src/autostart.rs:26-69`
**Sorun:** `SettingsPage::save()` her alan değişikliğinden 600 ms sonra çalışıyor ve **koşulsuz** `kick_github_pass`, `kick_azure_pass`, `load_models_for` çağırıyor. GitHub geçişi repo başına iki `gh` çağrısı (2 eşzamanlı repo, 20 s timeout), Azure geçişi repo başına REST çağrısı demek. Ayrıca `autostart::reconcile` her kayıtta çalışıyor ve Windows'ta `reg.exe` **senkron** (`command.output()`, `CREATE_NO_WINDOW`) çalıştırıyor — değer değişmemiş olsa bile.
**Etki:** Örneğin branch öneki metnini yazıp 600 ms beklemek, tüm repolarda onlarca `gh` süreci ve ağ isteği başlatır; Windows'ta her kayıtta UI thread'inde süreç oluşturma gecikmesi.
**Düzeltme:** Yalnız ilgili ayar grubu değiştiyse o geçişi başlat (GitHub/Azure anahtarları, PAT/URL, executable yolları); `autostart::reconcile`'i idempotent yap (mevcut dosya/değer aynıysa hiç yazma/çağırma); `reg.exe`'yi arka plan görevine al.

## B10. NuGet izleme: 1,5 s'de bir tüm ağacı tarama + async runtime'da bloklayan dosya sistemi
**Yer:** `apps/warp-station/crates/warp-station-core/src/nuget.rs:191-235` (döngü), `:433-459` (`scan_packages`/`walk`, derinlik 10), `:232-306` (`remove_dir_all` ve `fs::copy` doğrudan `async` gövdede)
**Sorun:** İzleme açıkken her 1,5 s'de izlenen klasörün tamamı özyinelemeli taranıyor (bin/obj dahil, hariç tutma listesi yok). `process_package`, global paket önbelleğini `std::fs::remove_dir_all` ile siliyor ve `fs::copy` yapıyor — bunlar tokio işçi thread'inde bloklayan çağrılar. warp-station net runtime'ı **2 işçi** ile çalışıyor (`apps/warp-station/crates/warp-station/src/net.rs:486-488`).
**Etki:** Büyük bir build çıktısı klasöründe sürekli disk yükü; yüzlerce MB'lık bir önbellek silinirken iki işçiden biri saniyelerce kilitlenir (git okumaları, GitHub istekleri, kapı yanıtları gecikir).
**Düzeltme:** `notify`/inotify tabanlı izleme veya `*.nupkg` için hedefli tarama + mtime önbelleği; ağır FS işlerini `spawn_blocking`'e taşı; işçi sayısını en az 4 yap.

> B11–B14 dayanıklılık ve süreç yaşam döngüsü sınıfındadır; kullanıcıya görünen sonuçları (asılı yükleme, zombi/yetim süreç, çift örnek) nedeniyle P1 listesine alındı.

## B11. warp-agent HTTP istemcisinde çoğu çağrıda zaman aşımı yok
**Yer:** `apps/warp-agent/src/oc.rs:331-337` (`ApiClient::new` zaman aşımısız), `:339-749` (create/list/rename/delete/abort/permission/question/command/files/config/models çağrıları), `apps/warp-agent/src/dsh.rs:240-247` (`alive`)
**Sorun:** Yalnız `send_message`/`send_command` isteklerinde 600 s'lik bir `.timeout` var; diğer tüm REST çağrıları varsayılan (süresiz) istemciyle gidiyor. Sunucu TCP'yi kabul edip yanıt vermezse ilgili görev ve UI'daki yükleme durumu süresiz asılı kalır; kapıdan gelen `models` isteği 60 s'lik `ANSWER_TIMEOUT` ile "timed out" döner ama iç görev hâlâ asılıdır.
**Etki:** Donmuş bir opencode/dsh sunucusu, iptal edilemeyen "yükleniyor" durumları ve biriken görevler.
**Düzeltme:** İstemciye makul bir varsayılan (`connect_timeout` + `timeout`, ör. 15/60 s) ver; yalnız uzun süreli akış/komut çağrılarında istek bazlı daha uzun timeout kullan.

## B12. `process.rs`: stderr sınırsız toplanıyor, çocuk süreç 25 ms'de bir yoklanıyor
**Yer:** `apps/warp-station/crates/warp-station-core/src/process.rs:120-142`, `:157-175`
**Sorun:** stdout `max_output_bytes` ile sınırlandırılıyor ("chatty child never materializes megabytes" hedefi) ama stderr `read_to_end` ile **sınırsız** okunuyor. Ayrıca çıkış `try_wait` + 25 ms uyku döngüsüyle bekleniyor; uzun bir `git fetch` boyunca saniyede 40 uyanma.
**Etki:** Stderr'e çok yazan bir araç (ör. ilerleme/uyarı üreten bir CLI) belleği şişirebilir; çok sayıda eşzamanlı geçişte gereksiz CPU uyanmaları.
**Düzeltme:** stderr için de aynı üst sınırı uygula (veya `read_to_end`'i `take(cap)` ile sar); `try_wait` döngüsü yerine `child.wait()` future'ını `timeout` ile `select!` et.

## B13. Süreç başlatmaları sahiplenilmeden bırakılıyor (zombi birikimi)
**Yer:** `apps/warp-station/crates/warp-station-core/src/warp_notes.rs:39-42`, `apps/warp-station/crates/warp-station-core/src/terminal.rs:404-410` ve benzeri `Command::spawn()` çağrıları, `apps/warp-agent/src/services.rs:23-42`, `apps/warp-notes/src/ui/notes.rs:1486`
**Sorun:** `Command::spawn()` ile başlatılan çocukların `Child` handle'ı drop ediliyor; `wait()` çağrılmıyor. Unix'te çocuk çıktığında ebeveyn onu toplamadığı için zombi süreç girdisi kalır.
**Etki:** Terminal/VS Code/tarayıcı başlatmalarının her biri kısa süreli de olsa zombi bırakır; uzun oturumda yüzlerce zombi PID'i birikebilir (özellikle `xdg-open` gibi hızlı çıkan süreçlerde).
**Düzeltme:** `let mut child = command.spawn()?; std::thread::spawn(move || { let _ = child.wait(); });` yardımcı bir `spawn_detached` fonksiyonu.

## B14. Çapraz uygulama: Warp Station için tek örnek koruması yok
**Yer:** `apps/warp-station/crates/warp-station/src/main.rs` (tüm başlangıç), `apps/warp-station/crates/warp-station/src/net.rs:536-566` (hotkey soketi), `apps/warp-station/crates/warp-station-core/src/settings.rs` (ayar dosyası)
**Sorun:** warp-agent kapı devri (`try_forward`) ile tek örneğe zorluyor; warp-station'da böyle bir koruma yok. İki örnek aynı `~/.warp/station/settings.json` ve `repos.cache.json` dosyalarına yazar (son yazan kazanır), aynı repolarda paralel fetch/probe, GitHub/Azure geçişleri çalıştırır. Linux'ta ikinci örnek clipboard ping soketini (`apps/warp-station/crates/warp-station/src/net.rs:543-545`) silip yeniden bağlanır; ilk örneğin global kısayolu ölür.
**Etki:** Ayar/önbellek kaybı, çift ağ yükü, bozuk kısayol.
**Düzeltme:** warp-agent'taki kapı yaklaşımının basit bir sürümünü (port+token dosyası veya tek örnek mutex'i) warp-station'a ekle; ikinci örnek ilkine devredip çıksın veya en azından salt-okunur açılsın.

---

# P2 — ölçek, dayanıklılık ve mimari borç

## C1. `~/.warp/ui.json` iki yazıcı arasında kilitsiz "oku-değiştir-yaz"
**Yer:** `kits/warpui-kit-core/src/scale.rs:52-90` (`shared_file`, `write_setting`), `:472-531` (`adopt_palette`/`set_palette`)
**Sorun:** Metin boyu ve palet aynı dosyada; yazım "dosyayı oku → anahtarı değiştir → geçici dosya + rename" şeklinde. İki uygulama (veya iki örnek) aynı anda farklı anahtarları yazarsa biri diğerinin güncellemesini kaybeder. Ayrıca `~/.warp` yolu ve atomik yazım burada yeniden yazılmış; `warpui-kit-platform::paths`/`io` zaten aynı işi yapıyor.
**Etki:** Nadir ama gerçek bir "ayarım neden geri döndü" durumu; iki ayrı yol/atomik yazım uygulaması bakım borcu.
**Düzeltme:** Yolu ve yazımı `warpui-kit-platform`'a taşı; tek bir süreç içi kilit + dosya kilidi (`flock`/`LockFileEx`) veya tüm anahtarları tek seferde yazan bir sahip (ör. ilk açılan uygulama) belirle.

## C2. Migrasyonlar artık farklı anlama gelen klasöre karşı her açılışta çalışıyor
**Yer:** `kits/warpui-kit-platform/src/paths.rs:162-225` (`adopt_notes_store`, `APP_LOCAL_NAMES`), `apps/warp-station/crates/warp-station-core/src/paths.rs:75-78`, `apps/warp-notes/src/paths.rs:37-51`
**Sorun:** `~/.warp/notes` önceden not deposuydu, şimdi warp-notes'un **uygulama verisi** klasörü. `adopt_notes_store` hâlâ her açılışta bu klasöre bakıp alt klasörleri ve `.md` dosyalarını `~/notes`'a taşıyor; yalnız `settings.json`, `ui.json`, `logs` adlı girişler korunuyor. warp-notes yarın başka bir klasör veya `.md` dosyası (önbellek, ek, dışa aktarım) yazarsa sessizce kullanıcının not deposuna taşınır.
**Etki:** Bugün çalışıyor; yeni bir uygulama dosyası eklendiğinde veri karışması riski. Ayrıca "uygulama verisi" ile "kullanıcı içeriği" ayrımı yalnız isim listesine bağlı.
**Düzeltme:** Migrasyonu bir kerelik sürüm işaretine bağla (`~/.warp/.layout-v2` gibi) ve `adopt_notes_store`'u yalnız o işaret yokken çalıştır; ya da app-local adları bir manifest dosyasından oku.

## C3. dsh oturum durumu hiç budanmıyor
**Yer:** `apps/warp-agent/src/dsh.rs:149` (`sessions: Mutex<HashMap<String, DshSessionState>>`), `:330-351` (`ensure_follow` ekler), `:1866-1889` (snapshot), `apps/warp-agent/src/store/sessions.rs:415-447` (`forget_session` dsh tarafını temizlemiyor)
**Sorun:** Her açılan/oluşturulan dsh sohbeti `DshHandle::sessions`'a bir `DshSessionState` ekliyor; sohbet silinse/arşivlense bile kayıt ve içindeki 120'ye kadar mesaj/parça bellekte kalıyor. `archived`, `recent_errors` kümeleri de yalnız büyüyor.
**Etki:** Uzun oturumda dsh kullanımı boyunca bellek doğrusal büyür; sunucu yeniden başlatılana kadar temizlenmez.
**Düzeltme:** `forget_session`/`StopServer` yolunda `DshHandle::drop_session(id)` çağır (takip akışını `cancel` ile kapat); `recent_errors`'ı TTL ile buda.

## C4. `exe::locate` olumsuz sonuçları oturum boyunca önbellekliyor
**Yer:** `apps/warp-station/crates/warp-station-core/src/exe.rs:44-77`
**Sorun:** Bulunamayan bir araç (`gh`, `rg`, `opencode`, `warp-agent`) `None` olarak önbelleğe yazılıyor ve süreç yeniden başlayana kadar hiç yeniden denenmiyor.
**Etki:** Kullanıcı uygulama açıkken aracı kurarsa (veya PATH'i güncellerse) Ayarlar'da yolu düzeltmedikçe özellik "bulunamadı"da kalır.
**Düzeltme:** Olumsuz girdileri TTL ile tut (ör. 30 s) veya yalnızca ayar değişiminde temizle; Ayarlar kaydında `exe::invalidate()` çağır.

## C5. Warp Station store gözlemcisi her bildirimde O(n) iş yapıyor
**Yer:** `apps/warp-station/crates/warp-station/src/ui/repos.rs:65-80` → `apps/warp-station/crates/warp-station/src/store/mod.rs:2164-2215`
**Sorun:** `ReposPage`, store'daki **her** `notify()` için `ensure_row_selected` çağırıyor; bu fonksiyon repo tab'larını ve seçili satırı bulmak için repolar üzerinde `to_string_lossy()` ile taramalar yapıyor, ayrıca `close_repo_tab`/`select_repo` üzerinden yeniden notify üretebiliyor. Bir toast süresinin dolması bile bu maliyeti tetikliyor.
**Etki:** Yüzlerce repoda her bildirimde gereksiz tarama; gözlemci içinde store mutasyonu (yeniden giriş) bakımı zorlaştırıyor.
**Düzeltme:** `ensure_row_selected`'i yalnız `ScanSettled` ve repo listesini değiştiren olaylardan çağır; gözlemciyi `cx.observe` yerine ilgili `UiEvent` işleyicilerine bağla; yol eşleştirmelerini `HashMap<PathBuf, usize>` ile O(1) yap.

## C6. warp-snapit çalışırken ayar değişikliklerini görmüyor
**Yer:** `apps/warp-snapit/src/engine.rs:81-123` (ayarlar başlangıçta okunur), `apps/warp-station/crates/warp-station/src/ui/windows_tools.rs:41-45` (Station yalnız başlat/durdur yapıyor)
**Sorun:** Companion `Settings.json`'ı yalnız açılışta okuyor; dosya dışarıdan (elle, eski SnapIt arayüzüyle veya bir başka araçla) değiştirilirse çalışan süreç bunu görmüyor, kullanıcı elle durdur/başlat yapmalı.
**Etki:** "Ayarı değiştirdim, hiçbir şey olmadı" davranışı; iki uygulama arasındaki sözleşmenin (aynı ayar dosyası) eksik yarısı.
**Düzeltme:** `SettingsStore`'a dosya izleyici ekle (notify) ve değişimde motoru yeniden kur; en azından Station'ın Tools sayfasında "ayar değişikliği için yeniden başlat" uyarısı göster.

## C7. CI: uygulama clippy'si ve Windows derlemesi PR kapısında değil
**Yer:** `.github/workflows/kits.yml:56-78` (clippy yalnız kit paketleri), `:105-140` (apps işi yalnız `cargo test`), `.github/workflows/suite-release.yml:82+` (Windows yalnız release dispatch'inde)
**Sorun:** `warp-station`, `warp-agent`, `warp-notes`, `snapit` için `clippy -D warnings` koşmuyor; `#[cfg(windows)]` kodu (snapit'in motoru dahil) PR'larda hiç derlenmiyor, yalnız release anında. Bu denetimde clippy temiz çıktı, ama regresyonu engelleyen bir kapı yok.
**Etki:** Uygulama kodunda lint/tip hataları birikir; Windows'a özgü kırılmalar ancak sürüm gününde görülür.
**Düzeltme:** `apps` işine `cargo clippy --locked --all-targets -p ... -- -D warnings` ekle; en azından `cargo check --target x86_64-pc-windows-msvc` (veya repodaki `check-windows.sh`) koşan bir Windows tip kontrolü işi ekle.

## C8. Ölü/yanıltıcı soyutlamalar ve kopya kod
**Yer:** `apps/warp-station/crates/warp-station-core/src/refresh.rs:14-69` (`RefreshCoalescer` hiç kullanılmıyor), `apps/warp-agent/src/store/runs.rs:158-205` (`track_session_info` son `None` kolu erişilemez), `apps/warp-agent/src/dsh.rs:1934` (aynı gövdeli kollar), `apps/warp-agent/src/ui/tile.rs:3985-4005` (her karede anahtar üretimi)
**Sorun:** Kullanılmayan genel API, kopyalanmış eşleşme kolları ve yanıltıcı yorumlar bakım maliyeti; `RefreshCoalescer`'ın var olması "geçişler koalesce ediliyor" izlenimi veriyor ama gerçekte kullanılmıyor.
**Etki:** Gelecekteki değişikliklerde yanlış varsayım; kod okunabilirliği düşüyor.
**Düzeltme:** `RefreshCoalescer`'ı ya gerçekten fetch/probe geçişlerine bağla ya da sil; `track_session_info`'daki `None` kollarını tek kola indir; `dsh` eşleşmelerini birleştir.

## C9. Render yolunda tekrar eden ayar klonları ve küçük tahsisler
**Yer:** `apps/warp-station/crates/warp-station/src/store/mod.rs:385-387` (`settings()` tam klon), ~60 çağrı noktası; `apps/warp-station/crates/warp-station/src/ui/settings.rs:1846-1864` ve `apps/warp-agent/src/ui/window.rs:2947-2955` (karşılaştırıcı içinde `to_lowercase`); `apps/warp-agent/src/store/tile.rs:71-78`
**Sorun:** `AppSettings` her okunduğunda derin klonlanıyor; model listeleri sıralanırken karşılaştırma başına `to_lowercase` tahsisi yapılıyor (O(n log n) tahsis); `is_waiting` her çağrıda öğe listesini tarıyor.
**Etki:** Tek tek küçük, ama çizim yolunda birleşince gereksiz CPU/GC baskısı.
**Düzeltme:** `Arc<AppSettings>`/`RwLock` ile klonsuz erişim veya `&AppSettings` döndüren erişimci; sıralama öncesi anahtarları bir kez küçük harfe çevir; bekleyen kart sayısını olay anında güncelle.

---

# Zaten iyi olanlar

- **Derleme kapıları temiz:** `cargo fmt --check`, `clippy --workspace --all-targets -D warnings` ve 654 test geçiyor; üretim kodundaki `unsafe` blokları `// SAFETY:` yorumlu (snapit Win32 interop'u dahil); testlerdeki süreç-global `set_var` çağrıları kilitli ve yorumlu.
- **Süreç disiplini:** `process.rs` zaman aşımı, süreç grubu ile ağaç öldürme, stdout üst sınırı ve `ELECTRON_RUN_AS_NODE` temizliği gibi ayrıntıları doğru uyguluyor; `git2` okumaları UI thread'inden uzakta.
- **Ağ/UI ayrımı:** İki uygulama da ağ işini ayrı OS thread + kendi tokio runtime'ına taşıyor; store olay güdümlü, stream yeniden çizimi 100 ms'ye throttle edilmiş (`apps/warp-agent/src/store/streaming.rs:232-253`), döşeme başına `rev: Entity<()>` ile komşu transkriptlerin yeniden çizilmesi engellenmiş.
- **Kapı sözleşmesi:** `kits/warpui-kit-platform/src/door.rs` tek yerde; token 0o600 ile atomik yayınlanıyor, karşılaştırma uzunluk kontrolü + dallanmasız XOR, `MAX_LINE` iki tarafta da uygulanıyor.
- **Kalıcılık:** Ayar/not/önbellek yazımları geçici dosya + rename ile atomik; not deposu yol kaçışlarına karşı bileşen bazlı doğrulanıyor; migrasyonlar kullanıcı içeriğinde çakışmada iki tarafı da koruyor.
- **Tasarım sistemi:** Renk/tipografi/shell/palet tek `warpui-kit-core`'dan; uygulamalarda renk/font literali yok; kitler arasında net sorumluluk ayrımı var.
- **Kod hijyeni:** TODO/FIXME yok; modül başlarında gerekçeli `//!` dokümantasyonu yaygın; testler gerçek davranışı (probe'larla) doğruluyor.

# Önerilen sıra

1. **A1** (not çakışması) ve **A2/A3** (asılı kalma) — kullanıcıya doğrudan dokunan hatalar.
2. **B2/B1** — seçim debounce'u ve net şeridinin önceliklendirilmesi; en görünür yavaşlama.
3. **B3/B4/B5** — agent başlatma kilidi, yetim süreçler, dsh kanal kilidi.
4. **B6/B7/B8** — editör ve liste sanallaştırma; ölçek sınırını yükseltir.
5. **C3/C7/C1** — dayanıklılık, CI kapıları, ortak ayar dosyası.

# Düzeltme durumu (5 Kasım 2026 oturumu)

Denetimden sonra öncelikli bulgular düzeltildi; her düzeltme kendi testini taşıyor.

| Bulgu | Durum | Değişiklik | Test |
|---|---|---|---|
| A1 | düzeltildi | `FileMeta` + `save_decision`; not editörü çakışma çubuğu (Reload/Overwrite); otomatik kayıt çakışmada durur; çıkışta kurtarma kopyası | `notes_store` birim testi + `notes_probe::an_external_edit_becomes_a_conflict_not_a_silent_overwrite` |
| A2 | düzeltildi | `closed_failed` + Retry düğmesi; başarısız yükleme "Loading…" yerine hata gösterir; merge anahtarı ve önbelleği temizler | `split_probe::a_failed_closed_load_offers_retry`, `the_closed_list_loads_when_it_is_asked_for` |
| A3 | düzeltildi | İptalden sonra `CANCEL_GRACE` (30 s) deadline'ı; door susarsa koşu hata ile kapanır | `warp::tests::a_cancelled_run_settles_when_the_door_stays_silent` |
| A4 | düzeltildi | `cache_generation` ile `run_id_seq` ayrıldı; wand/summary kimliği etiket kaydını iptal etmiyor | `store::tests::run_ids_do_not_touch_the_cache_save_gate` |
| B2 | düzeltildi | `LoadChanges` 100 ms debounce + okumadan önce `is_newest`; seçili satır 300 ms debounce ile yazılıyor | `repos_probe::selection_persists_once_after_the_debounce` + net/queue ve split probe'ları |
| B1 | kısmen | `Scan`, `FetchAll`, `GitHubPass`, `AzurePass` ayrı arka plan şeridine alındı; `Clone`, `DeepScan`, `LoadModels` kendi görevlerinde. `ProbeAll` bilinçli olarak ana şeritte kaldı: durum okumalarının bir Git mutasyonuyla yarışıp eski durumu yazmasını engelliyor; ayrıca arka plan geçişinin durum okuması bir Git işi sürerken uygulanmıyor (`StatusSettled` koruması) | `net_queue_probe` + tüm `warp-station` probe'ları |

# Doğrulama eki

```
$ cargo fmt --all --check                         # temiz (exit 0)
$ cargo clippy --workspace --all-targets --locked -- -D warnings
    Finished dev profile ...                      # temiz (exit 0)
$ cargo test --workspace --locked
    654 passed; 0 failed                          # exit 0
$ cargo clippy ... -W clippy::pedantic            # 1.720 uyarı; stil ağırlıklı
```

Düzeltmelerden sonra aynı kapılar yeniden koşuldu: `cargo fmt --all --check` temiz, `cargo clippy --workspace --all-targets --locked -- -D warnings` temiz, `cargo test --workspace --locked` **660 test geçti, 0 başarısız**. Bu koşu, düzeltme testlerinin yanı sıra `warp-station-core` içinde HOME'u geçici olarak değiştiren testlerle ev/yol testi arasındaki paralel çalışma yarışını da ortaya çıkardı; ikisi artık `test_support::HOME_LOCK` üzerinden sırayla koşuyor.

Pedantic çıktısında incelenen üretim kodu `Duration` çıkarma uyarıları korumalı çıktı (`elapsed >= interval` ve `since_last >= STREAM_REPAINT_INTERVAL` erken çıkışları); gerçek bir taşma yolu bulunmadı. `unsafe` blokları ağırlıklı olarak `// SAFETY:` yorumlu (birkaç test restore satırı hariç); `transmute`, `unsafe impl Send/Sync` veya `static mut` yok.

**Sınırlar:** Bu denetim çalışan uygulamada ölçüm (flamegraph, heap profili) içermez; B6/B7/B8'deki maliyetler kod yoluna ve çağrı sıklığına dayalı tahminlerdir. Windows'a özgü `warp-snapit` davranışı yalnız kod okuyarak ve Linux'ta çalışan platformdan bağımsız testlerle değerlendirildi.
