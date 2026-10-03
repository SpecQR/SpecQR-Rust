# Rust API

## 生成と見積もり

すべて `specqr::Result<T>` を返します。

- `generate(&str, &Options) -> QrCode`
- `generate_bytes(&[u8], &Options) -> QrCode`
- `generate_segments(&[Segment], &Options) -> QrCode`
- `estimate(&str, &Options) -> Plan`
- `estimate_bytes(&[u8], &Options) -> Plan`
- `analyze_segments(&[Segment], &Options) -> Plan`
- `get_capacity(version, ecc, mode, control_bits) -> Capacity`

`Plan::ok()` が false の見積もりも正常な戻り値です。固定版で収まらない場合はその版番号を保持し、自動範囲で収まらない場合は `version() == None` になります。`capacity_version()` は比較に使った版です。生成時は `DataTooLong` を返します。見積もりでは ECC・マトリックス・マスク探索を実行しません。

```rust
use specqr::{Options, Ecc, Mode};
let c = specqr::get_capacity(1, Ecc::L, Some(Mode::Numeric), 0)?;
assert_eq!(c.maximum(), Some(41));
let p = specqr::estimate("1234567890", &Options::default())?;
assert!(p.ok());
# Ok::<(), specqr::Error>(())
```

## Options

`Options::default()` は M、自動モード、自動版 1〜40、自動マスク、セグメント最適化有効、ECC 強化なし、余白 4、倍率 8、黒/白です。

公開設定フィールド:

- `ecc: Ecc` (`L/M/Q/H`)
- `mode: Option<Mode>` (`None` が auto)
- `version: Option<u8>`、`min_version/max_version: u8`
- `mask: Option<u8>` (0〜7)
- `optimize_segments: bool`、`boost_ecc: bool`
- `eci: Option<u32>` (0〜999999)
- `gs1: bool`、`fnc1_second: Option<String>`、`structured_append: Option<Segment>`
- `render: render::RenderOptions` (`margin/scale: u32`、`foreground/background: String`)
- `print_dpi: Option<f64>`

設定は各公開エントリーで検証します。ECI、GS1、FNC1 second、Structured Append の高水準指定は同時使用できません。手動 ECI の反復切り替えは可能ですが、他の制御グループとの混在には対応しません。固定 `version` は min/max 範囲より優先されます。

## Segment

`numeric/alphanumeric/utf8/kanji(&str)`、`bytes(&[u8])`、`eci(u32)`、`fnc1_second(&str)`、`structured_append(index,total,parity)` は検証付きです。`fnc1()` はデータを持たないヘッダーを返します。

```rust
use specqr::{generate_segments, Options, Segment};
let qr = generate_segments(&[
    Segment::eci(26)?,
    Segment::utf8("é / 日本語 / 😀")?,
], &Options::default())?;
assert_eq!(qr.segments()[0].assignment(), Some(26));
# Ok::<(), specqr::Error>(())
```

漢字対応は環境の locale/iconv に依存しません。Unicode→Shift_JIS の 6,953 項目を固定し、対応しない文字は明示的な漢字モードでエラー、自動モードでは UTF-8 byte で符号化します。自動最適化の同点は少ないセグメント数、それも同じなら数字・英数字・漢字・byte の安定順序で選びます。UTF-8 の byte 数と Unicode scalar 数を区別します。

## QrCode と描画

`matrix() -> &[Vec<bool>]`、`module(x,y) -> Result<bool>`、`version/size/mask/ecc`、`data_codewords/codewords/error_correction_codewords`、`segments/diagnostics/options` を読み取れます。結果は所有され、共有参照で内部を変更できません。

`to_svg()`、`to_png()`、`to_pixels()`、`to_svg_data_url()`、`to_png_data_url()` は生成時の描画設定を使用します。別設定には `render::to_png(qr.matrix(), &render_options)` などを使います。PNG は RGBA 8 bit、フィルター 0、非圧縮 DEFLATE です。圧縮率より可搬性と再現性を優先します。ラスター色は `#RGB/#RGBA/#RRGGBB/#RRGGBBAA`、black、white、transparent。SVG では XML 安全な CSS 色文字列も使えます。任意 CSS 色はコントラスト計算できません。

## 診断

`json::Value` の読み取り専用参照で返します。選択版/ECC/モード、セグメント bit 数、FNC1/ECI/SA メタデータ、余白、色、印刷寸法、警告を含みます。生成時はマスクごとの penalty・codeword 数も含みます。診断は規格適合認証や読取保証ではありません。

JSON codec は CLI と診断用の小さな標準ライブラリ実装です。数値は有限 `f64`、入力/出力 4 MiB、深さ 64、項目 1,000,000 が上限です。重複キー、不正エスケープ、単独 surrogate は拒否します。整数の正確性が必要な一般的な 64-bit データ交換には使用しないでください。検証専用 driver は実画像を運べる別の出力上限を持ちます。

## エラーと上限

`Error::kind()` は非網羅 `ErrorCode`、`code()` は安定文字列、`message()` は説明です。主なカテゴリは InvalidInput、InvalidMode、InvalidVersion、InvalidEcc、InvalidEci、InvalidGs1、InvalidStructuredAppend、InvalidColor、DataTooLong、ResourceLimit。

- 文字入力: 1,000,000 Unicode scalars、byte 入力: 1,000,000 bytes
- 手動セグメント: 16,384、合計 payload units: 1,000,000
- 1 シンボル最適化: 最大 7,089 scalars、bit materialization: 23,648 bits
- ラスターピクセル: 4 Mi-pixels、辺 2,048 以下
- SVG: 保守的な 8 MiB 出力上限、data URL: 32 MiB
- マトリックス公開描画入力: 1〜177 の正方形

不正 UTF-8 は `&str` に入らないため、CLI/JSON 入力境界で拒否します。整数設定の負数や範囲外は文字列/JSON 境界で拒否します。Rust の型で不可能な入力に対して例外互換を偽装しません。
