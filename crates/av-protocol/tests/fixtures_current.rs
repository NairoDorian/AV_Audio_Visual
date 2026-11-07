//! The committed fixtures under `test/fixtures/wire/` must equal what the generator produces
//! now. On failure, run `bun run fixtures` and review the diff: a fixture change is a wire
//! change.

use std::path::PathBuf;

#[test]
fn committed_wire_fixtures_are_current() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures/wire");
    let cases = av_protocol::fixtures::cases();
    for case in &cases {
        let path = dir.join(format!("{}.bin", case.name));
        let on_disk = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; run `bun run fixtures`", path.display()));
        assert_eq!(
            on_disk, case.bytes,
            "{} drifted; run `bun run fixtures`",
            case.name
        );
    }
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).expect("manifest.json");
    assert_eq!(
        manifest.replace("\r\n", "\n"),
        av_protocol::fixtures::manifest(&cases),
        "manifest.json drifted; run `bun run fixtures`"
    );
}

#[test]
fn committed_settings_fixtures_are_current() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures/settings");
    for (name, expected) in av_protocol::fixtures::settings_files() {
        let on_disk = std::fs::read_to_string(dir.join(name))
            .unwrap_or_else(|e| panic!("{name}: {e}; run `bun run fixtures`"));
        assert_eq!(
            on_disk.replace("\r\n", "\n"),
            expected,
            "{name} drifted; run `bun run fixtures`"
        );
    }
}
