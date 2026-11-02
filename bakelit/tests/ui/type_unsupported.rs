bakelit::bake! {
    static APP: Server = "ui-data/server.json";
    struct Server { host: String, port: u16 }
}

fn main() {}
