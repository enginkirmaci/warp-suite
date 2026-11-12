# Scribe: kaydırarak yazma, kelime doğrulama ve öneriler

**Durum:** Öneri (11 Kasım 2026), kod yazılmadı. Kapsam "tam set, aşamalı" ve
sözlük İngilizce olarak seçildi.

**Çözülmemiş kararlar:**

1. **Sözlük kaynağı.** Adaylar: AOSP `en` sözlüğü (Apache-2.0, frekanslı),
   `wordfreq` (Apache-2.0, Zipf frekansı), SCOWL (izin veren lisans, frekans
   yok). Öneri: `wordfreq` frekansı + SCOWL/AOSP kelime listesi; lisans ve
   boyut Faz 1'de doğrulanır.
2. **Kaydırma dekoderi.** Saf Kotlin mi (2,5–4 hafta, APK'ya ~0 ek yük),
   AOSP `libjni_latinime` portu mu (2–3 hafta entegrasyon, NDK/CMake,
   +10–30 MB sözlük, kanıtlanmış kalite)? Öneri: Kotlin ile başla, kabul
   eşiğini geçmezse porta dön.
3. **Sonraki kelime modeli.** Bigram (+2–5 MB veri, corpus lisansı) mı,
   yalnızca unigram + kişisel geçmiş mi? Öneri: v1 unigram + geçmiş; bigram
   ölçümden sonra.
4. **Kompakt düzende şerit.** Öneri: hayır. Kompakt düzende harf tuşu, yani
   "yazılan kelime" yok; şerit yalnızca FULL düzende.

**Hedef:** FULL düzende kaydırarak kelime yazma; yazılan kelimeyi sözlükle
doğrulayıp otomatik düzeltme; sonraki kelime ve emoji önerisi — hepsi tek bir
sözlük ve skorlayıcı üzerinde.

## Bugünkü tablo

- Her tuş tek `combinedClickable` kutu; harf `commitText(ch, 1)` ile gidiyor
  (`ui/ScribeKeyboard.kt`, `ScribeImeService.insertText`). Composing yok:
  `setComposingText` hiç çağrılmıyor, `onUpdateSelection` override edilmemiş.
- Bu yüzden "şu an yazılan kelime" diye bir durum yok. Düzeltme, öneri ve
  kaydırma için önce bu durumun kurulması gerekiyor; işin görünmeyen maliyeti
  burada.
- Sözlük, öneri satırı, n-gram ve NDK/native lib yok; bağımlılıklar Compose,
  OkHttp, coroutines.
- Emoji sayaçları hazır (`ScribePrefs.recordEmojiUse`, `EmojiUse`), emoji
  önerisi için veri kaynağı var.
- FULL düzeni İngilizce QWERTY; `AGENTS.md` Türkçe harf tuşunu yasaklıyor,
  bu yüzden Türkçe sözlük kapsam dışı.
- Compact düzende harf tuşu yok; kaydırma orada anlamsız.
- Android tarafında test yok; kapı `./gradlew assembleDebug` + cihaz.

## Tasarım kararları

### 1. Composing sözleşmesi

- Servis, FULL düzende yazılan son kelimeyi bir `StringBuilder` olarak tutar
  ve her tuşta `setComposingText` ile yazar.
- Boşluk: eşiği geçen aday varsa `commitText(aday + " ")`, yoksa ham kelime.
- Geri silme: composing açıkken ilk basış adayı ham hâle döndürür, ikinci
  basış karakteri siler.
- `onUpdateSelection`: imleç composing bölgesinden çıktıysa
  `finishComposingText` ve durumu bırak; hayalet kelime kalmaz.
- Şu durumlarda composing kapalı, bugünkü `commitText` davranışı sürer:
  `getTextBeforeCursor` null, `TYPE_NULL`, parola veya sayı alanı. Şerit ve
  kaydırma bu alanlarda görünmez.
- Parola alanında hiçbir öneri üretilmez, kaydırma kapalı.

### 2. Öneri şeridi

- FULL düzende üst bar ile tuş alanı arasında tek satır, `ScribeSize.Chip`
  (40 dp) yüksekliğinde; yeni renk veya sayı yok.
- Üç slot: en iyi düzeltme, ikinci aday, gerekirse bir emoji. Dokunma
  kelimeyi ve bir boşluğu commit eder.
- Şerit yalnızca metin alanında ve dikte/transkripsiyon yokken görünür.
- Yeni satır `AGENTS.md`'deki düzen envanterine eklenir; compact düzende yok.

### 3. Motor (Kotlin, Android'siz test edilebilir)

- Yeni `text/` paketi: `Dictionary` (trie + frekans), `WordScorer`
  (Damerau mesafesi, frekans, büyük harf, kullanıcı geçmişi), `Predictor`
  (önceki kelime → sonraki), `EmojiMap`, `GestureDecoder`.
- Tek bir `Suggestor` arayüzü: `onLetter`, `onSpace`, `onBackspace`,
  `candidates(limit)`. Klavye ve servis bunu konuşur; kaydırma da aynı
  arayüze "aday üret" olarak girer, ikinci bir motor yazılmaz.
- Veri: yeni `assets/` altında `en_words.*` (~50k kelime, sıkıştırılmış
  <2 MB) ve `emoji_map.json` (~50–100 KB); ilk kullanımda arka planda
  belleğe alınır.
- Birim testleri yeni `app/src/test/` altında; `./gradlew test` kapıya girer.

### 4. Kaydırarak yazma

- Tuş alanının tek bir `pointerInput` sahibi olur; tuşların
  `combinedClickable`'ı dokunma için kalır, eşikten uzun sürüklemede üst
  katman olayı tüketip dokunmayı iptal eder. Compose olay sırası bir günlük
  spike ile doğrulanır.
- İz: her karede (x, y, t); 32–64 noktaya yeniden örneklenir; tuş merkezleri
  `onGloballyPositioned` ile bir kez toplanır.
- Dekoder: nokta → en yakın tuş maliyeti + trie üzerinde kiriş araması +
  frekans (varsa bigram) skoru; hedef <30 ms.
- Sonuç doğrudan composing'e yazılır (en iyi aday), alternatifler şeritte.
- Kaydırma yalnızca FULL harf düzeninde; sayı ve sembol katmanında kapalı.

## Fazlar

| Faz | İş | Süre | Çıktı |
|---|---|---|---|
| 0 | Şerit + composing altyapısı, `text/` iskeleti, `./gradlew test` | 3–5 gün | Boş şerit, doğru kelime durumu |
| 1 | Sözlük, skorlayıcı, otomatik düzeltme, geri alma, sözlüğe ekle | 1,5–2 hafta | Yazarken düzeltme |
| 2 | Emoji haritası + şeritte emoji | 2–4 gün | Kelimeye göre emoji |
| 3 | Sonraki kelime (unigram + geçmiş; bigram ölçümle) | 1–1,5 hafta | Bağlama göre öneri |
| 4 | Kaydırma: işaretçi sahibi, iz, dekoder, tuning | 2,5–4 hafta + 2 hafta | Kaydırarak kelime |
| 5 | Cila: ayarlar, APK boyutu, README/AGENTS, cihaz matrisi | 3–5 gün | Kabul |

Toplam **10–14 hafta** (tek kişi, tam zamanlı, İngilizce). MVP (Faz 0–2)
**3–4 hafta**.

Uzun tedarik kalemleri Faz 0'da başlar: sözlük lisansı/boyutu ve (seçilirse)
bigram corpusu.

## Riskler

- **Uygulama uyumu:** composing'i bazı alanlar (web view, terminal) bozuyor;
  düşüş yolu bugünkü `commitText` davranışı olmalı, çökme değil.
- **Kalite:** kaydırma doğruluğu ilk sürümde düşük olabilir; AOSP portu yedek
  plan, ama NDK'yı bu repoya sokar.
- **Veri:** sözlük/corpus lisansı ve APK boyutu. Sideload olduğu için boyut
  esnek; yine de sözlük <2 MB hedefi korunur.
- **Test:** Android CI yok; JVM testleri + üç uygulamalı cihaz matrisi
  (WhatsApp, Chrome, Notlar) tek güvence.
- **Doküman:** `AGENTS.md` düzen ve davranış kuralları değişir (şerit satırı,
  composing, kaydırma); her fazla aynı commit'te güncellenir.

## Kabul testi

- FULL düzende şerit görünür; compact düzende ve parola alanında görünmez.
- "teh" → "the" düzeltmesi boşlukta uygulanır; geri silme önce ham kelimeye
  döner, sonra karakter siler.
- İmleç kelimenin dışına taşınınca composing kapanır, hayalet kelime kalmaz.
- Sonraki kelime önerisi bağlama göre değişir; emoji önerisi kelimeyle
  eşleşir ve dokununca commit eder.
- Kaydırma en iyi adayı yazar, alternatifleri şeritte gösterir; 30 ms altı.
- WhatsApp, Chrome, Notlar ve bir arama alanında çökme yok; cihazda
  `./gradlew assembleDebug` yeşil.
