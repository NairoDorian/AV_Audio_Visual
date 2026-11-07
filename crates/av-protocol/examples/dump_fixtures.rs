//! Writes the wire fixtures: `cargo run -p av-protocol --example dump_fixtures -- <dir>`.
//! Run through `bun run fixtures`, which passes `test/fixtures/wire`.

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("test/fixtures/wire"));
    std::fs::create_dir_all(&dir)?;
    let cases = av_protocol::fixtures::cases();
    for case in &cases {
        std::fs::write(dir.join(format!("{}.bin", case.name)), &case.bytes)?;
    }
    std::fs::write(
        dir.join("manifest.json"),
        av_protocol::fixtures::manifest(&cases),
    )?;
    println!("wrote {} wire fixtures to {}", cases.len(), dir.display());
    Ok(())
}
