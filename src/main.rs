fn main() {
    let incoming = futures::stream::repeat_with(|| {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    });

    let outgoing = futures::sink::unfold((), |_, msg| {
        serde_json::to_writer(std::io::stdout(), &msg).unwrap();
        println!();
        async { Ok(()) }
    });

    futures::pin_mut!(incoming);
    futures::pin_mut!(outgoing);

    puffin::set_scopes_on(cfg!(feature = "puffin_http"));
    #[cfg(feature = "puffin_http")]
    let _puffin_server =
        puffin_http::Server::new(&format!("0.0.0.0:{}", puffin_http::DEFAULT_PORT));

    futures::executor::block_on(cold_clear_2::run(incoming, outgoing));
}
