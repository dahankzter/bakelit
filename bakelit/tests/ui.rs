#![cfg(all(feature = "json", feature = "yaml"))]

#[test]
fn ui() {
    // trybuild builds each case in <target>/tests/trybuild/bakelit, which is the
    // CARGO_MANIFEST_DIR bake! reads from, so the data files go there.
    let target = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .parent()
        .unwrap();
    let data = target.join("tests/trybuild/bakelit/ui-data");
    std::fs::create_dir_all(&data).unwrap();
    for entry in std::fs::read_dir("tests/ui").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e != "rs" && e != "stderr") {
            std::fs::copy(&path, data.join(path.file_name().unwrap())).unwrap();
        }
    }
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
