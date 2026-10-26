use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;

use super::*;

const ICO: &[u8] = &[0, 0, 1, 0, 1, 0, 16, 16];

struct Fixed(SocketAddr);

#[async_trait]
impl Resolve for Fixed {
    async fn lookup(&self, _host: &str, _port: u16) -> Vec<SocketAddr> {
        vec![self.0]
    }
}

#[derive(Default)]
struct Opened(Mutex<Vec<String>>);

#[async_trait]
impl crate::artifact_pages::PageOpener for Opened {
    async fn open(&self, url: &str) -> Result<(), AppError> {
        self.0.lock().expect("opened").push(url.to_owned());
        Ok(())
    }
}

async fn serve(hits: Arc<AtomicUsize>) -> SocketAddr {
    let counted = |hits: Arc<AtomicUsize>| {
        move || {
            hits.fetch_add(1, Ordering::SeqCst);
            async {}
        }
    };
    let app = Router::new()
        .route(
            "/favicon.ico",
            get({
                let hit = counted(Arc::clone(&hits));
                move || async move {
                    hit().await;
                    (StatusCode::FOUND, [(header::LOCATION, "/real.ico")]).into_response()
                }
            }),
        )
        .route(
            "/real.ico",
            get({
                let hit = counted(Arc::clone(&hits));
                move || async move {
                    hit().await;
                    ([(header::CONTENT_TYPE, "text/plain")], ICO).into_response()
                }
            }),
        )
        .route("/page.html", get(|| async { "<html></html>" }))
        .route(
            "/huge.png",
            get(|| async {
                let mut body = vec![0x89, b'P', b'N', b'G'];
                body.resize(SITE_ICON_BYTES_MAX + 10, 0);
                body
            }),
        )
        .route("/missing.png", get(|| async { StatusCode::NOT_FOUND }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds");
    let address = listener.local_addr().expect("an address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    address
}

fn service(address: SocketAddr, permit: Permit) -> SiteService {
    SiteService::new(SiteServiceOptions {
        pages: None,
        resolver: Arc::new(Fixed(address)),
        permit,
    })
}

fn input(port: u16, icon: Option<&str>) -> SiteIconInput {
    SiteIconInput {
        url: format!("http://site.test:{port}/news/today"),
        icon: icon.map(|path| format!("http://cdn.test:{port}{path}")),
    }
}

#[test]
fn only_public_addresses_pass_the_guard() {
    for public in ["93.184.216.34", "1.1.1.1", "2606:4700::1111"] {
        assert!(public_address(public.parse().expect("an ip")), "{public}");
    }
    for private in [
        "127.0.0.1",
        "10.0.0.5",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "0.0.0.0",
        "198.18.0.1",
        "255.255.255.255",
        "::1",
        "fc00::1",
        "fe80::1",
        "::ffff:127.0.0.1",
        "2001:db8::1",
    ] {
        assert!(
            !public_address(private.parse().expect("an ip")),
            "{private}"
        );
    }
}

#[test]
fn icons_are_recognised_by_their_bytes_and_tried_vendor_first() {
    assert_eq!(sniff_icon(ICO), Some("image/x-icon"));
    assert_eq!(sniff_icon(&[0x89, b'P', b'N', b'G', 0]), Some("image/png"));
    assert_eq!(sniff_icon(b"RIFF0000WEBPVP8 "), Some("image/webp"));
    assert_eq!(sniff_icon(b"<svg onload=x>"), None);
    let candidates: Vec<String> = icon_candidates(&SiteIconInput {
        url: "https://example.com/a/b?c=1".to_owned(),
        icon: Some("https://cdn.example/logo.png".to_owned()),
    })
    .iter()
    .map(ToString::to_string)
    .collect();
    assert_eq!(
        candidates,
        [
            "https://cdn.example/logo.png",
            "https://example.com/favicon.ico"
        ]
    );
}

#[tokio::test]
async fn a_web_address_opens_in_the_integrated_browser_and_nothing_else_does() {
    let opened = Arc::new(Opened::default());
    let sites = SiteService::new(SiteServiceOptions {
        pages: Some(Arc::clone(&opened) as SharedPageOpener),
        ..SiteServiceOptions::default()
    });
    sites
        .open(&OpenSiteInput {
            url: " https://example.com/rates ".to_owned(),
        })
        .await
        .expect("opens");
    for url in ["file:///C:/x", "https://user:secret@example.com/"] {
        assert!(
            sites
                .open(&OpenSiteInput {
                    url: url.to_owned()
                })
                .await
                .is_err(),
            "{url}"
        );
    }
    assert_eq!(
        *opened.0.lock().expect("opened"),
        ["https://example.com/rates"]
    );
    let closed = SiteService::new(SiteServiceOptions::default());
    assert_eq!(
        closed
            .open(&OpenSiteInput {
                url: "https://example.com".to_owned()
            })
            .await
            .expect_err("no browser")
            .code,
        AppErrorCode::NotImplemented
    );
}

#[tokio::test]
async fn an_icon_follows_redirects_falls_back_to_the_favicon_and_is_cached() {
    let hits = Arc::new(AtomicUsize::new(0));
    let address = serve(Arc::clone(&hits)).await;
    let sites = service(address, |_| true);
    let icon = sites
        .icon(&input(address.port(), Some("/missing.png")))
        .await
        .expect("asks")
        .expect("an icon");
    assert_eq!(icon.mime, "image/x-icon");
    assert_eq!(STANDARD.decode(&icon.data).expect("base64"), ICO);
    assert_eq!(hits.load(Ordering::SeqCst), 2);
    sites
        .icon(&input(address.port(), Some("/missing.png")))
        .await
        .expect("asks again");
    assert_eq!(hits.load(Ordering::SeqCst), 2);
    for path in ["/page.html", "/huge.png"] {
        let only = SiteIconInput {
            url: format!("http://other.test:{}{path}", address.port()),
            icon: Some(format!("http://cdn.test:{}{path}", address.port())),
        };
        let found = service(address, |_| true)
            .fetch(Url::parse(only.icon.as_deref().unwrap_or_default()).expect("a url"))
            .await;
        assert_eq!(found, None, "{path}");
    }
}

#[tokio::test]
async fn an_icon_is_never_fetched_from_a_private_address() {
    let hits = Arc::new(AtomicUsize::new(0));
    let address = serve(Arc::clone(&hits)).await;
    let sites = service(address, public_address);
    assert_eq!(
        sites
            .icon(&input(address.port(), None))
            .await
            .expect("asks"),
        None
    );
    let local = SiteIconInput {
        url: format!("http://localhost:{}/", address.port()),
        icon: None,
    };
    assert_eq!(
        service(address, |_| true).icon(&local).await.expect("asks"),
        None
    );
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}
