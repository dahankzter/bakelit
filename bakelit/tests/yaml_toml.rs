#[cfg(feature = "yaml")]
mod yaml {
    bakelit::bake! {
        static APP: App = "tests/data/app.yaml";
        static INFERRED: Inferred = "tests/data/app.yaml";

        #[derive(Debug, PartialEq)]
        struct App {
            name: &'static str,
            port: u16,
            enabled: bool,
            ratio: f32,
            tags: &'static [&'static str],
            servers: &'static [Server],
            fallback: Server,
            note: &'static str,
            level: Level,
        }
        #[derive(Debug, PartialEq)]
        #[bake(rename_all = "kebab-case")]
        struct Server { host: &'static str, max_conns: Option<u32> }
        #[derive(Debug, PartialEq)]
        #[bake(rename_all = "lowercase")]
        enum Level { Info, Warn }
    }

    #[test]
    fn app() {
        assert_eq!(APP.name, "yaml");
        assert_eq!(APP.port, 8080);
        assert!(APP.enabled);
        assert_eq!(APP.ratio, 0.5);
        assert_eq!(APP.tags, &["a", "b"]);
        assert_eq!(APP.servers[0].max_conns, Some(10));
        assert_eq!(APP.fallback, APP.servers[1]);
        assert_eq!(APP.note, "two\nlines\n");
        assert_eq!(APP.level, Level::Info);
        assert_ne!(APP.level, Level::Warn);
        assert_eq!(INFERRED.servers[1].max_conns, None);
        assert_eq!(INFERRED.fallback.host, "b.example");
    }
}

#[cfg(feature = "toml")]
mod toml {
    bakelit::bake! {
        static APP: App = "tests/data/app.toml";
        static INFERRED: Inferred = "tests/data/app.toml";

        struct App {
            name: &'static str,
            port: u16,
            released: &'static str,
            ratio: f64,
            limits: &'static [(&'static str, u8)],
            servers: &'static [Server],
        }
        #[bake(rename_all = "kebab-case")]
        struct Server { host: &'static str, max_conns: Option<u32> }
    }

    #[test]
    fn app() {
        assert_eq!(APP.name, "toml");
        assert_eq!(APP.port, 8080);
        assert_eq!(APP.released, "2026-11-02");
        assert_eq!(APP.ratio, 0.5);
        assert_eq!(APP.limits, &[("cpu", 2)]);
        assert_eq!(APP.servers[1].host, "b.example");
        assert_eq!(APP.servers[1].max_conns, None);
        assert_eq!(INFERRED.servers[0].max_conns, Some(10));
        assert_eq!(INFERRED.limits.cpu, 2);
    }
}
