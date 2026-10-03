# Contributing

変更前に README と docs/verification.md を確認してください。Rust 1.85.1 と current stable、Windows/Linux/macOS を対象にします。通常ビルド・テストは Cargo オフラインで完結し、依存クレートを追加しません。

1. `cargo test --offline --all-targets --all-features`
2. `cargo test --offline --release --all-targets --all-features`
3. `cargo test --offline --doc`
4. `cargo clippy --offline --all-targets --all-features -- -D warnings`
5. `cargo fmt --all -- --check`
6. `RUSTDOCFLAGS="-D warnings" cargo doc --offline --no-deps`
7. `python tools/verify_package.py`

コア、最適化、URL、SA、PNG の変更では対応する独立検証も実行してください。期待値を増やすだけで差分を許容しないでください。URL profile の差分は入力・主体・参照出力を特定した完全一致で固定し、同数の誤差でも負の対照が失敗することを確認します。

公開結果は再現コマンド、実行したコンパイラー、対象 source fingerprint、合否/未実行の区別を含めます。未実行の Miri、sanitizer、32-bit 実行を合格扱いしません。
