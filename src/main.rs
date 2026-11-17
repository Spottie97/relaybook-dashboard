fn main() {
    if let Err(err) = relaybook_dashboard::serve() {
        eprintln!("relaybook-dashboard: {err:#}");
        std::process::exit(1);
    }
}
