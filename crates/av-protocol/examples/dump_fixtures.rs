//! Writes every cross-language fixture:
//! `cargo run -p av-protocol --example dump_fixtures -- <test/fixtures>`.
//! Run through `bun run fixtures`.

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("test/fixtures"));

    let wire = root.join("wire");
    std::fs::create_dir_all(&wire)?;
    let cases = av_protocol::fixtures::cases();
    for case in &cases {
        std::fs::write(wire.join(format!("{}.bin", case.name)), &case.bytes)?;
    }
    std::fs::write(
        wire.join("manifest.json"),
        av_protocol::fixtures::manifest(&cases),
    )?;

    let settings = root.join("settings");
    std::fs::create_dir_all(&settings)?;
    let files = av_protocol::fixtures::settings_files();
    for (name, text) in &files {
        std::fs::write(settings.join(name), text)?;
    }
    println!(
        "wrote {} wire and {} settings fixtures under {}",
        cases.len(),
        files.len(),
        root.display()
    );
    Ok(())
}
