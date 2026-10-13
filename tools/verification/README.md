# Development-only verification

The subject is a separately compiled, dependency-free Rust executable.

Python が比較処理を統括し、Node が固定した JavaScript/Nayuki の参照実装を実行し、3 種の独立デコーダーが検証対象の実際の行列と PNG を調べます。これらのツールやパッケージが候補の QR シンボルを生成することはありません。Cargo の `conformance` feature は Rust の開発用アダプターだけを有効にします。依存関係を追加せず、通常のインストール時には無効です。

## Reproduce

Rust 1.85.1 以降、Python 3.11+、Node、Java 17+（Java デコーダー用）を使用します。GS1 の監査には、正確に一致する公式の Node 24.19.0 と 24.21.0 のランタイムが必要です。メジャーバージョンだけでなく、Ada/ICU/Unicode のバージョンも確認します。

```sh
npm ci --prefix tools/verification --ignore-scripts
git clone https://github.com/SpecQR/SpecQR.git ../SpecQR-baseline
git -C ../SpecQR-baseline checkout 15ad15e5c770ea0e39072f8f88b2733018f02ffd
python3 tools/verification/verify_conformance.py --baseline ../SpecQR-baseline
python3 tools/verification/verify_structured_append.py --baseline ../SpecQR-baseline
python3 tools/verification/verify_jsqr.py
python3 tools/verification/verify_decoders.py --decoder cpp --dependency-dir .tools/zxing-cpp
python3 tools/verification/verify_decoders.py --decoder java --dependency-dir .tools/zxing-java
python3 tools/verification/verify_gs1.py --baseline ../SpecQR-baseline --node /path/node-24.19.0 --node-profile 24.19.0 --output artifacts/gs1-24.19.json
python3 tools/verification/verify_gs1.py --baseline ../SpecQR-baseline --node /path/node-24.21.0 --node-profile 24.21.0 --output artifacts/gs1-24.21.json
```

各候補プロセスの実行ファイルは、`cargo build --offline --locked --release
--features conformance --bin conformance` により一時保存先へビルドします。`--candidate` はソースツリー、`--cargo` は Cargo、`--binary` は指定したビルド済み開発用アダプターを選択します。最後のオプションでは、渡された成果物をソースからのビルドで置き換えません。クレートの利用側を検証するレーンでは、生成したクレートを新規ディレクトリーへ展開し、`--candidate` で選択します。

`SPECQR_CARGO` は既定の Cargo コマンドを指定します。`SPECQR_DEV_NODE_MODULES` では、`package.json` と `node_modules` を含む別の開発用パッケージディレクトリーを指定できます。使用前に ZXing-C++ の公式 PyPI wheel のハッシュと ZXing Java JAR の SHA-256 を確認します。`--no-install` には、検証済みのデコーダーファイルが既に存在している必要があります。npm、Python、Java のパッケージを `[dependencies]` や `[dev-dependencies]` に含めません。レポートの既定の保存先は `artifacts/` で、`--output` により外部の場所も選択できます。

## Exact coverage

- 固定した JS をその場で実行して比較する、公開 API の 3,028 行列。完全なパディング済みデータ、インターリーブ済み data/ECC codeword、version/ECC/mask、行列の全モジュールを対象とする SHA-256 を含みます。バージョン 1–40 × ECC × mask の全 1,280 組合せ、混在・手動モード、Unicode、漢字、バイナリー、自動最適化・マスク、ECI 境界、FNC1 の両位置、GS1、boost、決定的な fuzz を対象にします。
- 独立した Nayuki と比較する 2,400 行列。固定した version/ECC/mask の全組合せを含みます。独立実装によるセグメント分割やマスク選択の違いを、黙って同等として扱いません。
- 全マスクスコアを含む 4,320 raw-pattern 行列、65,536 GF 積、Reed–Solomon の generator/remainder の全次数 1–255。
- 容量の完全一致 640 件、境界見積もり 1,920 件、Structured Append の 22 セットとそれらに含まれる 112 行列、共有プロセスで並行処理する 256 行列、型付きエラーによる不正入力拒否 18 件。独立した SA レーンでは、全診断、分割の offsets/units、codeword を比較します。続いて、全 total 2–16 を対象に順序を入れ替えた文字列・バイナリーのマージ 30 ケースと、不正なマージ 11 ケースを、実際に実行した JavaScript と照合します。
- jsQR: 232 行列と 232 個の実 PNG の検出、全バージョン、64 個の ECI ヘッダー。jsQR は FNC1/SA に対応しないため、このレーンでそれらの成功は主張しません。
- ZXing-C++: 706 行列と 750 個の PNG の検出、135 個の SA ヘッダー、手動・高水準の両経路での合法な全 152 種の FNC1-second indicator、ECI の bytes/text、高水準の完全な 5 セット（44 シンボル）、独立した再結合。
- ZXing Java: 446 行列と 446 個の PNG の検出、全バージョン、135 個の SA ヘッダー、両経路で再結合する完全な 5 セット、破損した 32 シンボルのテスト。それぞれ正確に 3 codeword が訂正されることを要求します。

独立した検出を行う前に、Rust が生成した実 PNG の全 RGBA ピクセルと quiet-zone ピクセルを検査します。ZXing Java の strict PNG 検出は scale 3 を使用し、`PURE_BARCODE` や行列への代替を使いません。scale 8 の挙動は、同一ピクセルの独立 PNG を対照とする、明示的に分けた診断です。検出失敗を成功件数へ加算することはありません。C++ は既定の scale 8 を検査します。3 種すべてのデコーダーが、実際の空白画像を拒否する必要があります。

## Exact GS1 profiles

各 Node プロファイルでは、固定した JavaScript を実行し、全 5,610 ケース / 15,690 操作と照合します。範囲を限定した AI カタログの全体、診断、不正な percent escape、IPv4/IPv6、authority/path/query の処理、URL 作成を含みます。`gs1/reference-url-outcomes.json` は、正確に指定した両 Node ランタイムについて、全 54 件の IDNA/dot ケースを固定します。`gs1/rust-url-outcomes.json` は、レビュー済みの Rust の観測結果 55 件（同じ 54 ケースと UTF-16 でのみ表せる 1 入力）を別途記録します。dot 値を作成するケースの重複数は、元の corpus と同じに保ちます。Kotlin 候補の結果を Rust の期待値として取り込まず、IDNA/dot という広い分類全体を例外扱いしません。未知の差分、総数が変わらなくても値が変わった場合、古い fixture は監査を失敗させます。

Rust の URL 実装は、範囲を限定し、外部依存を持ちません。Unicode 15.0.0 に固定した小文字化・拒否テーブル、全角 ASCII と代替 dot の対応付け、soft-hyphen の除去、RFC3492 Punycode、厳密な ACE の往復変換検証を使用します。未対応の結合文字・format・未割当文字、互換正規化で変化する scalar、Hangul Jamo は拒否します。保守的な RTL 検査は、mixed letter/digit conventions を拒否します。別の 39 種の host 変異を含む corpus では、これらの明示的な方針に対して 78 request / 156 operation を検査し、その場で実行した Node との差分も記録します。完全な UTS46、NFC、全 Unicode に対するブラウザーとの URL 同等性は主張しません。GS1 の dot-only 値は意図的に保持し、query の値として出力します。Rust の文字列は対にならない UTF-16 surrogate を表現できないため、厳密な JSON 入力境界で corpus 内の該当する 1 入力を拒否し、置換しません。レポートには、この入力領域の違いを明示します。Node 24.21 が不正な ASCII ACE label を受理する挙動は Rust に引き継ぎません。

## Subject identity and mutation controls

レポートには、ソース・テストのハッシュ、対象の Rust プロセスの正確な PID/nonce/実行ファイル、独立して確認した実行ファイルの SHA-256、依存関係がないことを示す Cargo metadata、参照先の commit/source/corpus のハッシュ、デコーダー・パッケージの fingerprint を含みます。ソース、実行ファイル、依存関係、ツールが変わると、その実行は無効になります。応答件数を厳密に検査し、応答の切り詰めを合格として扱わないようにします。範囲を限定した `--suite public` または `internal` の結果には、不完全であることを明示します。

6 種類のアダプター故障は、最初に実際の Rust を実行した後、行列ハッシュ・データバイト・インターリーブ済み codeword の破損、終了、応答の省略、エラーの出力を行います。さらに 3 種類の故障で、型のない失敗を型付きの入力検証として数えられないことを確認します。SA とマージの故障では、metadata と再結合した payload を破損させます。GS1 の対照では、実際の Rust のカタログ出力、参照 URL データ、Rust の dot URL を破損させ、dot URL の集約上の差分分類は維持します。これらの対照は feature で有効化するアダプターだけに限定し、製品ライブラリーや通常の CLI には含めません。

これらは有限の合成入力による検査です。ISO/GS1 認証、印刷・カメラ試験、任意の破損や入力に対する保証ではありません。パッケージ、コンパイラー・OS の組合せ、新規環境でのオフライン利用側の検証は、別の検査です。

## Provenance

開発用 harness は、MIT ライセンスの SpecQR-Kotlin を基に調整したものです。その workflow は、SpecQR-Java `1f2bf277a582f0eb4e31c9975443c1a76552aa23`、SpecQR-Python `1deb5cf83ed7a93eeb44138e5980a963a00cdffb`、SpecQR-CPP `e91cd8fe4434fd6ef10d1126bb2fd17b0b19321f`、SpecQR-CSharp `057c4b3f25e52c4786a8f94c744ff884eedcecfa` に由来します。Java デコーダーは元の SpecQR-Swift の帰属表示を保持しています。これらはテスト workflow の派生元であり、検証対象の Rust は先行する他言語実装・エンコーダー・デコーダーをリンクしません。
