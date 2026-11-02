#[cfg(feature = "kdl")]
mod kdl {
    bakelit::bake! {
        static APP: App = "tests/data/app.kdl";
        static INFERRED: Inferred = "tests/data/app.kdl";

        struct App { name: &'static str, port: u16, tags: &'static [&'static str], server: &'static [Server], db: Db }
        #[bake(rename_all = "kebab-case")]
        struct Server { host: &'static str, max_conns: Option<u32> }
        struct Db { name: &'static str, pool: u8 }
    }

    #[test]
    fn app() {
        assert_eq!(APP.name, "kdl");
        assert_eq!(APP.port, 8080);
        assert_eq!(APP.tags, &["a", "b"]);
        assert_eq!(APP.server.len(), 2);
        assert_eq!(APP.server[0].max_conns, Some(10));
        assert_eq!(APP.server[1].host, "b.example");
        assert_eq!(APP.db.pool, 4);
        assert_eq!(APP.db.name, INFERRED.db.name);
        assert_eq!(INFERRED.server[1].max_conns, None);
    }
}

#[cfg(all(feature = "frontmatter", feature = "yaml"))]
mod markdown {
    bakelit::bake! {
        static POST: Post = "tests/data/post.md";
        static INFERRED: Inferred = "tests/data/post.md";

        struct Post { title: &'static str, draft: bool, tags: &'static [&'static str], #[bake(body)] body: &'static str }
    }

    #[test]
    fn post() {
        assert_eq!(POST.title, "Hello");
        assert!(!POST.draft);
        assert_eq!(POST.tags, &["rust", "macros"]);
        assert_eq!(POST.body, "# Hello\n\nBaked at compile time.\n");
        assert_eq!(INFERRED.body, POST.body);
        assert_eq!(INFERRED.title, "Hello");
    }
}
