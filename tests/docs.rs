use specqr::{Ecc, Options, gs1, structured_append as sa};
#[test]
fn documentation_examples_execute() -> specqr::Result<()> {
    let qr = specqr::generate(
        "Hello, 日本語",
        &Options {
            ecc: Ecc::Q,
            ..Default::default()
        },
    )?;
    assert!(qr.to_svg()?.starts_with("<svg"));
    assert!(qr.to_png()?.starts_with(&[137, 80, 78, 71]));
    let elements = [
        gs1::Element::new("01", "09506000134352"),
        gs1::Element::new("10", "LOT-123"),
    ];
    let data = gs1::to_element_string(&elements)?;
    let qr = specqr::generate(
        &data,
        &Options {
            gs1: true,
            ..Default::default()
        },
    )?;
    assert_eq!(
        qr.diagnostics()
            .get("gs1")
            .and_then(specqr::json::Value::as_bool),
        Some(true)
    );
    let url = gs1::create_digital_link(&elements, "https://id.gs1.org")?;
    assert_eq!(gs1::parse_digital_link(&url)?.elements().len(), 2);
    let options = Options {
        version: Some(1),
        ecc: Ecc::M,
        ..Default::default()
    };
    let set = sa::generate(
        "Rust Structured Append 日本語 12345678901234567890",
        &options,
        16,
        &sa::DiagnosticOptions::default(),
    )?;
    assert!(set.total() >= 2);
    for (i, qr) in set.symbols().iter().enumerate() {
        assert_eq!(qr.segments()[0].index(), Some(i as u8 + 1));
    }
    let parity = sa::calculate_parity("ABC日本語")?;
    let parts = [
        sa::Part::new(2, 2, parity, sa::PartData::Text("日本語".into()))?,
        sa::Part::new(1, 2, parity, sa::PartData::Text("ABC".into()))?,
    ];
    assert_eq!(sa::merge(&parts)?.text(), Some("ABC日本語"));
    Ok(())
}
