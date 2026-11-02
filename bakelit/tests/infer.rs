#![cfg(feature = "json")]

bakelit::bake! {
    pub static DATA: Data = "tests/data/inferred.json";
    const ROWS: &[Row] = "tests/data/rows.json";
}

#[test]
fn inferred_types() {
    assert_eq!(DATA.title, "inferred");
    assert_eq!(DATA.version, 3i64);
    assert_eq!(DATA.servers[0].host, "a.example");
    assert_eq!(DATA.servers[0].weight, Some(0.5));
    assert_eq!(DATA.servers[1].weight, None);
    assert_eq!(DATA.servers[1].port, 8080);
    assert_eq!(DATA.servers[0].note, None);
    assert_eq!(DATA.servers[1].note, Some("backup"));
    assert_eq!(DATA.owner.tags, &["x", "y"]);
    let copy: DataOwner = DATA.owner;
    assert_eq!(copy, DATA.owner);
    assert_eq!(ROWS, &[Row { k: "a", v: 1 }, Row { k: "b", v: 2 }]);
}
