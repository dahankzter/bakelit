bakelit::bake! {
    static APP: Server = "ui-data/server.json";
    struct Server { host: &'static str, port: u16 }
}

fn main() {}
