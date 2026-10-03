# 検証と適用範囲

## 主体の識別

検証対象は Rust のソースから独立にビルドした実行ファイルです。外部 oracle が Rust の代わりに QR を生成する構成ではありません。各検証プロセスは nonce、PID、実行ファイル SHA-256 を返し、外部 harness が再計算します。入力ごとの応答件数、変更前後のソース fingerprint、依存グラフを照合します。

基準: [SpecQR JavaScript](https://github.com/SpecQR/SpecQR/tree/15ad15e5c770ea0e39072f8f88b2733018f02ffd)、`v3.0.0-rc.2`。既存ユーザー所有版の契約・安全修正を参照しつつ、Rust の符号化・URL・PNG 処理は独立に実装しています。Nayuki と 3 種デコーダーは開発専用です。

## 検証層

1. Rust の型/単体/統合テスト: 全 ECC/版、GF 乗算、RS、最適化の独立探索、canonical 漢字表、各種 malformed/resource/Unicode/control 入力
2. MSRV 1.85.1 と current stable 1.99.0 の debug/release、Clippy warning-free、rustfmt、rustdoc、doc examples
3. 実 Rust プロセスと pinned JavaScript/Nayuki の全マトリックス・data/ECC codeword 完全一致
4. GS1 5,610 ケース / 15,690 操作を Node 24.19.0 と 24.21.0 で別々に評価
5. Structured Append の全 diagnostics、split offsets/units、merge のデータ/エラー完全一致
6. 実 PNG のすべての RGBA/quiet-zone pixel を検査してから jsQR、ZXing-C++、ZXing-Java で独立デコード
7. 独立 reviewer による direct API property/fuzz、資源使用・URL相互運用性・パッケージ検査
8. Cargo package から新規ディレクトリへ展開し、空の Cargo 状態でテスト・CLI インストール・下流利用
9. 公開された正確な commit を対象に Windows/Linux/macOS × MSRV/stable CI、および公開 clone からの利用確認

実施結果は [verification-summary.json](verification-summary.json) に記録します。CI の結果は [GitHub Actions](https://github.com/SpecQR/SpecQR-Rust/actions) で正確な commit に対応するものを確認してください。

## Exact conformance の規模

- 公開 API: 3,028 マトリックス、うち 2,400 が独立 Nayuki とも一致
- 固定全組: 40 versions × 4 ECC × 8 masks = 1,280
- raw pattern: 4,320 マトリックスと全 mask penalty
- 65,536 GF products、RS degree 1〜255
- 640 容量、1,920 境界見積もり
- SA 22 sets / 112 symbols、全 total 2〜16 の shuffled text/binary merge 30 ケース、malformed merge 11 ケース
- shared-process concurrent matrices 256、typed malformed 18、主体の誤りを検出する 9 基本負の対照
- jsQR: matrix 232 + actual PNG 232
- ZXing-C++: matrix 706 + PNG 750、SA headers 135、FNC1 second 152 indicators × 2 routes
- ZXing-Java: matrix 446 + PNG 446、SA headers 135、破損 32 シンボルで各 3 codewords の訂正を厳密確認

QR 内容が同じでも codeword・マスク・セグメント選択が違うケースは同一として数えません。独立 oracle が違う最適化方針を採るときは比較対象を明示します。canonical Kanji は Unicode の全 1,112,064 scalars に対する owner JS の対応集合と 13-bit 値を照合し、6,953 項目が完全一致しました。

## URL/IDNA 差分

元 JS 自身が Node 24.19 と 24.21 で異なる ACE domain 挙動を持つため、参照ランタイムを正確に固定しています。Rust の期待値は独立に実測・レビューした fixture です。旧言語版の結果や差分件数をそのまま合格基準にしていません。同じ差分件数でも値が変わった場合、参照 fixture が変わった場合、Rust catalog が壊れた場合に失敗する負の対照があります。

GS1 dot-only path values の保全、bounded Unicode hostname profile、不正 ACE の保守的拒否、Rust が表現できない単独 UTF-16 surrogate は明示的差分です。通常の ASCII URL や AI catalog/diagnostic の誤差まで広く免除しません。[GS1 文書](gs1.md)を参照してください。

## デコーダー制限の扱い

実 PNG を読み取る strict lane ではマトリックスへの代替や PURE_BARCODE fallback を使いません。ZXing-Java は strict scale 3、C++ は default scale 8 で評価します。Java の scale 8 の一部読み取り挙動は独立した同一 pixel の PNG control と照合する別診断で、失敗を成功件数に含めません。すべてのデコーダーに真の blank image 負の対照を適用します。

## 再現

Rust 標準テストとパッケージ検査は README のコマンドで実行できます。外部 oracle の正確なセットアップと実行順は [tools/verification/README.md](../tools/verification/README.md) にあります。依存デコーダーの wheel/JAR と npm lock は hash 固定です。開発専用 `conformance` feature に故障注入を限定し、一般 CLI とライブラリには故障注入経路を持ちません。

## 非主張・未実行

- ISO/GS1 認証、全 Unicode/WHATWG、全カメラ/印刷実機を証明するものではありません
- 32-bit は i686 compile-check。ローカル 32-bit 実行は行っていません
- Miri と Rust sanitizer は未実行。導入済み stable/MSRV toolchain に該当 nightly 検証環境がありません
- `no_std` / embedded target の互換性を主張しません
- 有限 corpus/fuzz の成功は、あらゆる入力・任意の破損・OS メモリ枯渇の無害性を保証しません

初期の失敗や途中の結果は修正後の最終ソースの合格に置き換えず、最終 fingerprint で全該当検証を再実行します。
