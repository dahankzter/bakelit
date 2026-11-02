#[cfg(feature = "jsonc")]
mod jsonc {
    bakelit::bake! {
        static APP: App = "tests/data/app.jsonc";
        struct App { name: &'static str, ports: &'static [u16] }
    }

    #[test]
    fn app() {
        assert_eq!(APP.name, "jsonc");
        assert_eq!(APP.ports, &[80, 443]);
    }
}

#[cfg(feature = "json5")]
mod json5 {
    bakelit::bake! {
        static APP: App = "tests/data/app.json5";
        struct App { name: &'static str, mask: u8, ratio: f32, big: f64, limit: f64, note: &'static str }
    }

    #[test]
    fn app() {
        assert_eq!(APP.name, "json5");
        assert_eq!(APP.mask, 255);
        assert_eq!(APP.ratio, 0.5);
        assert_eq!(APP.big, 1000.0);
        assert_eq!(APP.limit, f64::INFINITY);
        assert_eq!(APP.note, "line continued");
    }
}

#[cfg(feature = "jsonl")]
mod jsonl {
    bakelit::bake! {
        static ROWS: &[Row] = "tests/data/rows.jsonl";
    }

    #[test]
    fn rows() {
        assert_eq!(ROWS.len(), 2);
        assert_eq!(ROWS[1].k, "b");
        assert_eq!(ROWS[1].v, 2);
    }
}
