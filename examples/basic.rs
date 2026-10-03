use specqr::{Options, generate};
fn main() -> specqr::Result<()> {
    let qr = generate("Hello, 日本語", &Options::default())?;
    println!(
        "version={} size={} mask={}",
        qr.version(),
        qr.size(),
        qr.mask()
    );
    println!("{}", qr.to_svg()?);
    Ok(())
}
