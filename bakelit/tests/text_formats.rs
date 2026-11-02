#[cfg(feature = "csv")]
mod csv {
    bakelit::bake! {
        static COUNTRIES: &[Country] = "tests/data/countries.csv";
        static INFERRED: &[Row] = "tests/data/countries.csv";
        static TSV: &[TsvRow] = "tests/data/countries.tsv" as tsv;

        struct Country { code: &'static str, name: &'static str, population: u64, area: f64, landlocked: bool }
    }

    #[test]
    fn countries() {
        assert_eq!(COUNTRIES.len(), 2);
        assert_eq!(COUNTRIES[1].code, "CH");
        assert_eq!(COUNTRIES[0].population, 10_600_000);
        assert_eq!(COUNTRIES[1].area, 41285.0);
        assert!(COUNTRIES[1].landlocked);
        assert_eq!(COUNTRIES[0].name, INFERRED[0].name);
        assert_eq!(INFERRED[0].population, 10_600_000i64);
        assert_eq!(INFERRED[0].area, 450295.5f64);
        assert!(!INFERRED[0].landlocked);
        assert_eq!(TSV[0].name, "Sweden");
    }
}

#[cfg(feature = "ini")]
mod ini {
    bakelit::bake! {
        static APP: App = "tests/data/app.ini";
        static INFERRED: Inferred = "tests/data/app.ini";
        struct App { name: &'static str, server: Server }
        struct Server { host: &'static str, port: u16 }
    }

    #[test]
    fn app() {
        assert_eq!(APP.name, "demo");
        assert_eq!(APP.server.port, 8080);
        assert_eq!(APP.server.host, INFERRED.server.host);
        assert_eq!(INFERRED.server.port, "8080");
    }
}

#[cfg(feature = "env")]
mod env {
    bakelit::bake! {
        static APP: App = "tests/data/app.env";
        static INFERRED: Inferred = "tests/data/app.env" as env;
        #[bake(rename_all = "SCREAMING_SNAKE_CASE")]
        struct App { app_name: &'static str, port: u16, greeting: &'static str, price: &'static str }
    }

    #[test]
    fn app() {
        assert_eq!(APP.app_name, "demo");
        assert_eq!(APP.port, 8080);
        assert_eq!(APP.greeting, "hello\nworld");
        assert_eq!(APP.price, "$5");
        assert_eq!(INFERRED.port, "8080");
    }
}

#[cfg(feature = "properties")]
mod properties {
    bakelit::bake! {
        static APP: &[(&str, &str)] = "tests/data/app.properties";
        static INFERRED: Inferred = "tests/data/app.properties";
    }

    #[test]
    fn app() {
        assert_eq!(APP, &[("app.name", "demo"), ("app.port", "8080")]);
        assert_eq!(INFERRED.app_name, "demo");
    }
}
