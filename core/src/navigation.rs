use url::Url;

pub fn may_load(url: &Url, bundled_origin: &Url, proxy_port: u16) -> bool {
    url.origin() == bundled_origin.origin()
        || (url.scheme() == "http"
            && url.host_str() == Some("localhost")
            && url.port_or_known_default() == Some(proxy_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_bundled_page_and_the_proxy_load() {
        let bundled = Url::parse("http://tauri.localhost/").unwrap();
        let port = 49152;
        let cases = [
            ("http://tauri.localhost/".to_owned(), true),
            ("http://tauri.localhost/index.html".to_owned(), true),
            (format!("http://localhost:{port}/"), true),
            (
                format!("http://localhost:{port}/f0123456789ab/pair#v1.abc"),
                true,
            ),
            (format!("http://localhost:{port}/static/app.js"), true),
            (format!("http://127.0.0.1:{port}/"), false),
            (format!("http://localhost:{}/", port + 1), false),
            (format!("https://localhost:{port}/"), false),
            ("http://evil.test/".to_owned(), false),
            (format!("http://localhost:{port}@evil.test/"), false),
            (
                "file:///data/user/0/io.github.atqamz.handcompanion/files/page.html".to_owned(),
                false,
            ),
            ("handboard://c#v1.abc".to_owned(), false),
        ];
        for (url, allowed) in cases {
            let url = Url::parse(&url).unwrap();
            assert_eq!(may_load(&url, &bundled, port), allowed, "{url}");
        }
    }
}
