# Structured Append

2〜16 シンボルの組を生成・結合します。公開 API の index は 1 始まりです。QR ヘッダー内部は 0 始まりで格納します。parity は元の UTF-8 / 生バイトの XOR であり、暗号認証ではありません。

```rust
use specqr::{Options, Ecc, structured_append as sa};
let options = Options { version: Some(1), ecc: Ecc::M, ..Options::default() };
let set = sa::generate(
    "Rust Structured Append 日本語 12345678901234567890",
    &options, 16, &sa::DiagnosticOptions::default(),
)?;
assert!(set.total() >= 2);
for (i, qr) in set.symbols().iter().enumerate() {
    assert_eq!(qr.segments()[0].index(), Some(i as u8 + 1));
}
# Ok::<(), specqr::Error>(())
```

## 入力

- `generate(text, options, max_symbols, diagnostics)`
- `generate_bytes(bytes, options, max_symbols, diagnostics)`
- `generate_segments(segments, options, max_symbols, diagnostics)`
- `calculate_parity(text)`、`calculate_bytes_parity(bytes)`、`calculate_segments_parity(segments)`

通常文字列は Unicode scalar 境界で分割します。生バイトは byte 境界です。手動 numeric/alphanumeric/kanji セグメントは不可分、手動 byte セグメントは適切な入力境界で分割できます。指定した手動モードを勝手に変更しません。バージョンを小さい方から探索し、容量内の最長 prefix を決定的に選びます。各候補バージョンで SA ヘッダー込みの 1 シンボルに収まる場合、その候補をスキップします。要求範囲内に適格な 2〜16 シンボルの分割がない場合は拒否します。小さいバージョンで適格な分割が見つかれば、より大きなバージョンで 1 シンボルに収まる場合でもその分割を返します。不要な分割を避けたい場合は通常の生成 API を使い、特殊な用途では明示的な低水準 SA ヘッダーを使用してください。

SA は ECI、FNC1、GS1、ECC boost と組み合わせません。ヘッダーは SA API 自身が作成します。手動 SA では mode/optimization の再指定を許可せず、呼び出し側のモードを保持します。ヘッダーを単独で手動制御する場合は一般 `generate_segments` を使います。

`SaResult` は `symbols/total/parity/input_length/byte_length/diagnostics` の getter を持ちます。`DiagnosticOptions::default()` は要約、`::full()` は split unit と拡張シンボル診断を含みます。診断の詳細度は符号化結果を変えません。

## マージ

```rust
use specqr::structured_append::{self as sa, Part, PartData};
let text = "ABC日本語";
let parity = sa::calculate_parity(text)?;
let parts = [
    Part::new(2, 2, parity, PartData::Text("日本語".into()))?,
    Part::new(1, 2, parity, PartData::Text("ABC".into()))?,
];
let merged = sa::merge(&parts)?;
assert_eq!(merged.text(), Some(text));
# Ok::<(), specqr::Error>(())
```

`merge` は任意順の完全な組を index 順に並べ、重複、欠落、total/parity 不一致、文字列/バイト混在、合計サイズ超過、最終 XOR 不一致を拒否します。入力 Part も所有された不変値です。検証は組の整合性であり、異なる内容が同じ 8-bit parity を持つ可能性はあります。

デコーダーは含みません。外部デコーダーが ECI/GS1/SA の raw bytes や metadata をどのように返すか確認してください。表示用文字列を元バイトの代用にすると、文字コード変換により parity が変わることがあります。
