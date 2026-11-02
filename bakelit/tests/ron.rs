#![cfg(feature = "ron")]

bakelit::bake! {
    static APP: App = "tests/data/app.ron";

    #[derive(Debug)]
    struct App {
        name: &'static str,
        level: Level,
        shapes: &'static [Shape],
        origin: Option<(i32, i32)>,
        initial: char,
        big: u128,
        small: i128,
        lookup: &'static [(&'static str, u8)],
    }
    #[derive(Debug, PartialEq)]
    enum Level { Info, Warn }
    #[derive(Debug, PartialEq)]
    enum Shape { Circle { r: f64 }, Pair(u8, u8), Wrap(u8), Dot }
}

#[test]
fn app() {
    assert_eq!(APP.name, "ron");
    assert_eq!(APP.level, Level::Warn);
    assert_ne!(APP.level, Level::Info);
    assert_eq!(
        APP.shapes,
        &[
            Shape::Circle { r: 1.5 },
            Shape::Pair(1, 2),
            Shape::Wrap(5),
            Shape::Dot
        ]
    );
    assert_eq!(APP.origin, Some((0, -1)));
    assert_eq!(APP.initial, 'x');
    assert_eq!(APP.big, u128::MAX);
    assert_eq!(APP.small, i128::MIN);
    assert_eq!(APP.lookup, &[("a", 1), ("b", 2)]);
}
