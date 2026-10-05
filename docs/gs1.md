# GS1 と Digital Link

本実装は SpecQR の bounded AI catalog と要素文字列のヘルパーです。GS1 General Specifications / Digital Link の全範囲や認証済み実装を意味しません。AI は 50 項目に限定し、数字・長さ・基本的な文字集合・一部チェックディジットを検証します。日付 AI は形式のみ、GLN は桁数/数字のみで、業務上の有効性を保証しません。

```rust
use specqr::{gs1, Options};
let elements = [
    gs1::Element::new("01", "09506000134352"),
    gs1::Element::new("10", "LOT-123"),
];
let data = gs1::to_element_string(&elements)?;
let qr = specqr::generate(&data, &Options { gs1: true, ..Options::default() })?;
assert!(qr.diagnostics().get("gs1").and_then(specqr::json::Value::as_bool).unwrap());
let url = gs1::create_digital_link(&elements, "https://id.gs1.org")?;
let parsed = gs1::parse_digital_link(&url)?;
assert_eq!(parsed.elements().len(), 2);
# Ok::<(), specqr::Error>(())
```

## API

- `get_supported_ais()`、`get_ai_info(ai)` は不変のカタログ情報
- `calculate_check_digit`、`append_gtin_check_digit`、`append_sscc_check_digit`、`validate_check_digit`
- `from_human_readable`、`to_human_readable`、`to_element_string`
- `parse_element_string`、`validate_element_string`、`validate_elements`
- `create_digital_link`、`parse_digital_link`、`validate_digital_link`、`normalize_digital_link`

オプション付き処理は `_with_options` 版を使います。`ValidationOptions` と `DigitalLinkOptions` は既定値と `with_*` で設定します。検証結果は boolean、構造化 errors/warnings、解析済み要素を返します。未知 query の preserve/reject、primary/path AI 指定、正規化モードなどを明示できます。カタログで対応しない AI を全 GS1 と同じように扱いません。

要素値は printable ASCII、括弧と GS を除く範囲です。固定/可変長と必要な FNC1 separator を検証します。末尾の可変長データ内に別 AI らしい文字列が含まれる曖昧性については、元の SpecQR の検出方針を保持します。

## URL 処理

URL を取得・DNS 解決することはありません。Rust 標準ライブラリから構築したローカル parser を使います。HTTP/HTTPS、ASCII authority、IPv4 の数値表記、IPv6、既定 port、credential escaping、base path、query、fragment を扱います。query はフォーム形式に合わせて `+` と percent を処理します。path の不正 percent / 不正 UTF-8 はエラーです。未知 query の replacement decoding は WHATWG URLSearchParams と同じ意味を意図しており、GS1 path payload の黙った修正には使いません。

GS1 の AI 値 `.` / `..` および percent 表記は通常 URL の dot segment とは区別して保持します。正規化/新規作成ではその値を query に移し、ブラウザーの path 正規化により GS1 データが消えることを防ぎます。この安全方針は元 JS の挙動と意図的に異なります。

## Unicode hostname の bounded profile

外部 IDNA クレートや OS の URL parser は使用しません。ASCII hostname を推奨します。Unicode hostname は小文字化、全角 ASCII、代替 dot、soft-hyphen の除去、RFC 3492 Punycode を扱う限定プロファイルです。完全な NFC、UTS #46、contextual/bidi 検証は実装していません。未対応の結合文字・format・互換正規化を要する文字などは保守的に拒否します。表は Unicode 15.0.0 に固定し、小文字化も同じ表を使うためコンパイラーの Unicode 更新に依存しません。Python 標準ライブラリの Unicode 15.0.0 から tools/generate_idna_profile.py で再現生成できます。[Unicode データライセンス](../LICENSE-UNICODE)を同梱しています。結合文字・format/control・未割当・separator・互換正規化で変化する文字・文脈 NFC を要する Hangul Jamo などを拒否し、RTL の混在にも保守的な制約を設けます。

従って、他の URL 実装とすべて同じ受理範囲にはなりません。不正/非 canonical ACE を成功として正規化しません。通常の ASCII・IPv4・IPv6 は別枠で検証し、Unicode 差分と混ぜて許容しません。Rust `String` では UTF-16 の単独 surrogate を表現できないため、該当 JSON 入力は境界で拒否します。

元 JS は Node 24.19 と 24.21 の Ada/WHATWG 更新で一部 ACE domain の受理が異なります。検証では両バージョンを個別固定し、Rust 自身の実測入力/出力との完全一致 fixture を使います。差分件数のみの allowlist ではありません。

## 上限と診断

GS1 の文字数/offset は元契約に合わせ UTF-16 code units、最大 1,000,000。要素/path/query component は 16,384 以下です。Unicode label と ACE label にも個別上限があります。Rust 固有の型付き引数により JavaScript の null/object 型の誤用は API で表現できません。エラー種別と意味を保持しつつ、型で不可能な入力の架空の互換性を主張しません。

## 互換性の読み方と呼び出し側の対応

最終の元 corpus では Node 24.19 に対して 15,647/15,690 操作が完全一致し、43 差分は dot 値保全 40 操作と Rust に表現できない単独 surrogate 3 操作です。Node 24.21 では 15,620/15,690 が一致し、同じ 43 操作に 9 種の malformed/edge ACE host × 3 操作の保守的拒否が加わります。これは診断文言だけの差分をまとめた件数ではなく、受理/拒否または payload/URL 値の相違を個別固定した結果です。通常の表現可能な URL、カタログ、検証 diagnostics の未知差分は許容しません。

別の 39 hostname variants / 156 operations の safety corpus は、正規化を必要とする有効な Unicode 入力でも本プロファイルが拒否する場合を明示します。拒否は完全な IDNA で無効という意味ではありません。全 scalar の accepted-output 安定性検査も、未対応入力の受理や任意文字列の完全サポートを証明しません。

可能なら通常の ASCII hostname を使ってください。事前に信頼できる完全な IDNA 実装で NFC/IDNA 処理した canonical A-label を渡す方法もありますが、この API は A-label を復号して同じプロファイルを検証するため、すべての有効な IDNA domain が通る保証はありません。単に Punycode 化して拒否を回避することはできません。未対応 domain を必要とする場合は、その処理を適切な URL ライブラリを持つアプリケーション側で行い、得られた完全 URL 文字列を一般の `generate` で QR 化できます。その場合に GS1 helper の検証済みという保証は付けないでください。

## URL serialization compatibility (2026-10-05)

base URL の空 fragment `#` は作成時に保持します。正規化は従来どおり空 fragment を除去し、非空 fragment は拒否します。 限定した URL 出力互換性の拡張であり、通常の QR 符号化・公開 API・runtime dependency は変更しません。既存の dot 値・NUL・IDNA・診断方針を保持します。[固定 corpus と再現手順](../tools/url-serialization/README.md) を参照してください。
