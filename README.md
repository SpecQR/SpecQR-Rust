# SpecQR Rust

A dependency-free, from-scratch QR Code Model 2 encoder in safe Rust. Supports optimized mixed segments, GS1 helpers, Structured Append, SVG, and portable PNG. Rust 2024; MSRV 1.85.1. No FFI, external crates, platform encoders, or runtime downloads.

日本語を中心とする Rust 版です。既存の SpecQR アルゴリズム・API 契約を基に、Rust の所有権、列挙型、`Result`、読み取り専用スライスに合わせて実装しています。他言語版の実行や第三者 QR ライブラリへの委譲はありません。

## 状態

`0.1.0-rc.1`。GitHub 公開版であり、crates.io には公開していません。GS1 の完全認証、全 WHATWG URL / UTS #46 準拠、`no_std`、デコーダー機能を主張しません。[検証方法と制限](docs/verification.md)、[GS1 の適用範囲](docs/gs1.md)を確認してください。

## 最短の利用例

Rust 1.85.1 以上と Cargo を使用します。Rust 2024 の初期安定版 1.85.0 に対する doctest 修正を含む 1.85.1 を最小バージョンとしています。[Rust 公式リリース](https://blog.rust-lang.org/2025/03/18/Rust-1.85.1/)

```toml
[dependencies]
specqr = { git = "https://github.com/SpecQR/SpecQR-Rust" }
```

再現性が必要な利用では `rev` に確認済みコミットを指定してください。

```rust
use specqr::{generate, Ecc, Options};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options { ecc: Ecc::Q, ..Options::default() };
    let qr = generate("Hello, 日本語", &options)?;
    std::fs::write("qr.svg", qr.to_svg()?)?;
    std::fs::write("qr.png", qr.to_png()?)?;
    println!("version={} mask={}", qr.version(), qr.mask());
    Ok(())
}
```

## CLI

```sh
cargo install --git https://github.com/SpecQR/SpecQR-Rust --locked --bin specqr
specqr "Hello, 日本語" --format svg --output qr.svg
specqr --hex 00ff8081 --format png --output binary.png
specqr "01234567890123456789" --ecc H --estimate
specqr --segments examples/segments.json --format json
specqr --help
```

ローカル checkout では `cargo run --offline --bin specqr -- "Hello"`。入力を省略すると標準入力から UTF-8 を読みます。PNG を標準出力へ出す場合はバイナリ出力として扱ってください。`--package-version` はパッケージ版、`--version` は QR バージョンの指定です。

## 主な機能

- QR Code Model 2: バージョン 1〜40、L/M/Q/H、8 種のマスク、自動最小バージョン・ECC 強化
- 数字、英数字、UTF-8、任意バイト、固定表による漢字、最小ビット混在セグメント、手動境界
- ECI、FNC1 第 1/第 2 位置、Structured Append 制御
- 容量計算・生成前見積もり・マスク評価・描画/印刷上の診断
- 50 個の bounded GS1 AI メタデータ、要素文字列、チェックディジット、Digital Link
- 2〜16 シンボルの Structured Append 分割・パリティ・順序復元・マージ検証
- SVG、RGBA ピクセル、PNG、SVG/PNG data URL。PNG は DEFLATE stored blocks・CRC32・Adler32 を自前実装
- `#![forbid(unsafe_code)]`、依存クレート 0、型付きエラー、所有された不変結果、`Send + Sync`

## 設計と制約

通常の入力設定には公開フィールド付き `Options` を使い、生成結果は変更可能な内部配列を公開しません。借用スライスを通じて読み取り、必要な場合だけ呼び出し側で複製できます。`&str` は Rust により UTF-8 が保証されます。不正 UTF-8 を保持したいデータには `generate_bytes` を使います。

高水準 FNC1 入力のリテラル `%` は byte モードで安全に保持します。強制 alphanumeric との組み合わせはエラーです。低水準の手動 FNC1 セグメントでは、呼び出し側が `%` の GS1 エスケープ責任を持ちます。

入力・セグメント数・画像面積・SVG 出力には上限があります。範囲チェックと容量の事前検査を行いますが、OS のメモリ枯渇やプロセス終了を回復する保証はありません。QR の ECC は悪意ある改変の認証ではありません。保存したシンボルを実際の印刷・カメラ条件でも検証してください。

## ドキュメント

- [API・設定・エラー](docs/api.md)
- [GS1 / Digital Link と URL の境界](docs/gs1.md)
- [Structured Append](docs/structured-append.md)
- [検証・再現方法](docs/verification.md)
- [貢献](CONTRIBUTING.md) / [安全性](SECURITY.md)

## ビルド・テスト

```sh
cargo test --offline --all-targets --all-features
cargo test --offline --release --all-targets --all-features
cargo test --offline --doc
cargo clippy --offline --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo doc --offline --no-deps
python tools/verify_package.py
```

ライブラリ・CLI・Rust テストは Rust 標準ライブラリだけで動作します。独立検証の Node/Python/ZXing は開発時専用で、依存クレートには含めません。`conformance` feature の検証用実行ファイルは既定ビルド・CLI インストールには含まれません。

## ライセンス

Rust のオリジナルコードは MIT。[LICENSE](LICENSE)、[NOTICE](NOTICE)。生成された Unicode プロファイルデータには [Unicode License v3](LICENSE-UNICODE) が適用され、パッケージ全体の SPDX 表現は `MIT AND Unicode-3.0` です。QR パラメータと漢字対応表の出典・検証参照は NOTICE と検証文書に記載しています。
