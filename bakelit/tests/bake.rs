#![cfg(feature = "json")]

bakelit::bake! {
    pub static APP: App = "tests/data/app.json";
    const EDGES: Edges = "tests/data/edges.json";
    static ODD: &[(&str, u8)] = "tests/data/dir with spaces/größe.json";

    #[derive(Debug, Clone, Copy, PartialEq)]
    #[bake(rename_all = "kebab-case")]
    pub struct App {
        pub name: &'static str,
        pub port: u16,
        pub ratio: f32,
        pub servers: &'static [Server],
        pub log_level: Level,
        pub limits: &'static [(&'static str, u32)],
        pub shape: Shape,
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Server {
        pub host: &'static str,
        #[bake(rename = "max-conns")]
        pub max_conns: Option<u32>,
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    #[bake(rename_all = "lowercase")]
    pub enum Level { Debug, Info }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Shape { Circle { r: f64 }, Dot }

    struct Edges { u64_max: u64, i64_min: i64, u128_max: u128, i128_min: i128, f32: f32 }
}

#[test]
fn app() {
    assert_eq!(APP.name, "demo");
    assert_eq!(APP.port, 8080);
    assert_eq!(APP.ratio, 0.75);
    assert_eq!(APP.servers.len(), 2);
    assert_eq!(
        APP.servers[0],
        Server {
            host: "a.example",
            max_conns: Some(10)
        }
    );
    assert_eq!(APP.servers[1].max_conns, None);
    assert_eq!(APP.log_level, Level::Info);
    assert_eq!(APP.limits, &[("cpu", 2), ("mem", 512)]);
    assert_eq!(APP.shape, Shape::Circle { r: 1.5 });
    assert_ne!(APP.shape, Shape::Dot);
}

#[test]
fn edges_and_odd_paths() {
    assert_eq!(EDGES.u64_max, u64::MAX);
    assert_eq!(EDGES.i64_min, i64::MIN);
    assert_eq!(EDGES.u128_max, u128::MAX);
    assert_eq!(EDGES.i128_min, i128::MIN);
    assert_eq!(EDGES.f32, 0.1f32);
    assert_eq!(ODD, &[("größe", 1)]);
}
