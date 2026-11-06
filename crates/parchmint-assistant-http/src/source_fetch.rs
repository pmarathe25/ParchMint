//! Bounded retrieval of an explicitly selected public research source.

use std::{
    fmt,
    io::Read,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

use reqwest::{Url, blocking::Client, header, redirect::Policy};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;
const DNS_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESOLVERS: usize = 2;
const CANCELLATION_POLL: Duration = Duration::from_millis(50);

#[derive(Clone, PartialEq, Eq)]
pub struct FetchedResearchOriginal {
    pub final_url: String,
    pub filename: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl fmt::Debug for FetchedResearchOriginal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FetchedResearchOriginal")
            .field("media_type", &self.media_type)
            .field("byte_length", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFetchError {
    InvalidAddress,
    UnsafeDestination,
    Unavailable,
    UnsupportedContent,
    TooLarge,
    TooManyRedirects,
    Cancelled,
}

impl fmt::Display for SourceFetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAddress => {
                "The source address must be a public HTTPS URL without credentials."
            }
            Self::UnsafeDestination => "The source address resolves to a non-public destination.",
            Self::Unavailable => "The source could not be downloaded.",
            Self::UnsupportedContent => {
                "The source is not a supported uncompressed PDF or text document."
            }
            Self::TooLarge => "The original source exceeds the 64 MiB limit.",
            Self::TooManyRedirects => "The source redirected too many times.",
            Self::Cancelled => "Source download stopped.",
        })
    }
}

impl std::error::Error for SourceFetchError {}

/// Downloads only after the author has selected a source. Credentials, project
/// passages, proxies, cookies, and referrers are never sent with this request.
pub fn fetch_research_original(
    url: &str,
    cancelled: &AtomicBool,
) -> Result<FetchedResearchOriginal, SourceFetchError> {
    fetch_with(url, cancelled, &SystemResolver, &ReqwestFetcher)
}

trait Resolver {
    fn resolve(
        &self,
        host: &str,
        port: u16,
        cancelled: &AtomicBool,
    ) -> Result<Vec<SocketAddr>, SourceFetchError>;
}

struct SystemResolver;
impl Resolver for SystemResolver {
    fn resolve(
        &self,
        host: &str,
        port: u16,
        cancelled: &AtomicBool,
    ) -> Result<Vec<SocketAddr>, SourceFetchError> {
        static ACTIVE: OnceLock<Arc<AtomicUsize>> = OnceLock::new();
        let active = ACTIVE.get_or_init(|| Arc::new(AtomicUsize::new(0))).clone();
        let host = host.to_owned();
        resolve_with_deadline(active, cancelled, DNS_TIMEOUT, move || {
            (host.as_str(), port)
                .to_socket_addrs()
                .map(|iter| iter.collect())
                .map_err(|_| SourceFetchError::Unavailable)
        })
    }
}

// OS DNS calls cannot be interrupted on every platform. Keep their number
// bounded even after the caller times out or cancels; a slot is released only
// when the underlying lookup actually returns.
struct ResolverSlot(Arc<AtomicUsize>);

impl Drop for ResolverSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn resolve_with_deadline(
    active: Arc<AtomicUsize>,
    cancelled: &AtomicBool,
    timeout: Duration,
    lookup: impl FnOnce() -> Result<Vec<SocketAddr>, SourceFetchError> + Send + 'static,
) -> Result<Vec<SocketAddr>, SourceFetchError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(SourceFetchError::Cancelled);
    }
    active
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < MAX_RESOLVERS).then_some(count + 1)
        })
        .map_err(|_| SourceFetchError::Unavailable)?;
    let slot = ResolverSlot(active);
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("parchmint-research-dns".into())
        .spawn(move || {
            let _slot = slot;
            let _ = sender.send(lookup());
        })
        .map_err(|_| SourceFetchError::Unavailable)?;

    let deadline = Instant::now() + timeout;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(SourceFetchError::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(SourceFetchError::Unavailable);
        }
        match receiver.recv_timeout(remaining.min(CANCELLATION_POLL)) {
            Ok(result) => return result,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(SourceFetchError::Unavailable),
        }
    }
}

struct FetchResponse {
    status: u16,
    location: Option<String>,
    media_type: Option<String>,
    bytes: Vec<u8>,
}

trait Fetcher {
    fn get(
        &self,
        url: &Url,
        addresses: &[SocketAddr],
        cancelled: &AtomicBool,
    ) -> Result<FetchResponse, SourceFetchError>;
}

struct ReqwestFetcher;
impl Fetcher for ReqwestFetcher {
    fn get(
        &self,
        url: &Url,
        addresses: &[SocketAddr],
        cancelled: &AtomicBool,
    ) -> Result<FetchResponse, SourceFetchError> {
        let host = url.host_str().ok_or(SourceFetchError::InvalidAddress)?;
        let client = Client::builder()
            .https_only(true)
            .no_proxy()
            .referer(false)
            .redirect(Policy::none())
            .resolve_to_addrs(host, addresses)
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(45))
            .build()
            .map_err(|_| SourceFetchError::Unavailable)?;
        let mut response = client
            .get(url.clone())
            .header(header::ACCEPT_ENCODING, "identity")
            .header(header::USER_AGENT, "ParchMint research source")
            .send()
            .map_err(|_| SourceFetchError::Unavailable)?;
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if (300..400).contains(&status) {
            return Ok(FetchResponse {
                status,
                location,
                media_type: None,
                bytes: Vec::new(),
            });
        }
        if status != 200 {
            return Err(SourceFetchError::Unavailable);
        }
        if response.headers().contains_key(header::CONTENT_RANGE) {
            return Err(SourceFetchError::UnsupportedContent);
        }
        if response
            .headers()
            .get(header::CONTENT_ENCODING)
            .is_some_and(|value| !value.as_bytes().eq_ignore_ascii_case(b"identity"))
        {
            return Err(SourceFetchError::UnsupportedContent);
        }
        let media_type = response_content_type(response.headers());
        if !media_type.as_deref().is_some_and(|kind| {
            supported_media_type(essence(kind)) || essence(kind) == "application/octet-stream"
        }) {
            return Err(SourceFetchError::UnsupportedContent);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_BYTES as u64)
        {
            return Err(SourceFetchError::TooLarge);
        }
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 16 * 1024];
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err(SourceFetchError::Cancelled);
            }
            let remaining = MAX_BYTES + 1 - bytes.len();
            let count_limit = remaining.min(chunk.len());
            let count = response
                .read(&mut chunk[..count_limit])
                .map_err(|_| SourceFetchError::Unavailable)?;
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..count]);
            if bytes.len() > MAX_BYTES {
                return Err(SourceFetchError::TooLarge);
            }
        }
        Ok(FetchResponse {
            status,
            location,
            media_type,
            bytes,
        })
    }
}

fn fetch_with(
    url: &str,
    cancelled: &AtomicBool,
    resolver: &dyn Resolver,
    fetcher: &dyn Fetcher,
) -> Result<FetchedResearchOriginal, SourceFetchError> {
    let mut url = Url::parse(url).map_err(|_| SourceFetchError::InvalidAddress)?;
    for redirects in 0..=MAX_REDIRECTS {
        if cancelled.load(Ordering::Acquire) {
            return Err(SourceFetchError::Cancelled);
        }
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.host().is_none()
        {
            return Err(SourceFetchError::InvalidAddress);
        }
        url.set_fragment(None);
        let port = url
            .port_or_known_default()
            .ok_or(SourceFetchError::InvalidAddress)?;
        let host = url.host_str().ok_or(SourceFetchError::InvalidAddress)?;
        if host.ends_with('.') {
            return Err(SourceFetchError::InvalidAddress);
        }
        let literal = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>();
        let addresses = match literal {
            Ok(ip) => vec![SocketAddr::new(ip, port)],
            Err(_) => resolver.resolve(host, port, cancelled)?,
        };
        if addresses.is_empty() || addresses.iter().any(|address| !public_ip(address.ip())) {
            return Err(SourceFetchError::UnsafeDestination);
        }
        let response = fetcher.get(&url, &addresses, cancelled)?;
        if matches!(response.status, 301 | 302 | 303 | 307 | 308) {
            if redirects == MAX_REDIRECTS {
                return Err(SourceFetchError::TooManyRedirects);
            }
            url = url
                .join(
                    response
                        .location
                        .as_deref()
                        .ok_or(SourceFetchError::Unavailable)?,
                )
                .map_err(|_| SourceFetchError::InvalidAddress)?;
            continue;
        }
        if response.status != 200 {
            return Err(SourceFetchError::Unavailable);
        }
        let mut media_type = response
            .media_type
            .as_deref()
            .and_then(normalized_content_type)
            .filter(|kind| {
                supported_media_type(essence(kind)) || essence(kind) == "application/octet-stream"
            })
            .ok_or(SourceFetchError::UnsupportedContent)?;
        if response.bytes.len() > MAX_BYTES {
            return Err(SourceFetchError::TooLarge);
        }
        let looks_like_pdf = response.bytes[..response.bytes.len().min(1024)]
            .windows(5)
            .any(|window| window == b"%PDF-");
        if media_type == "application/pdf" || media_type == "application/octet-stream" {
            if !looks_like_pdf {
                return Err(SourceFetchError::UnsupportedContent);
            }
            media_type = "application/pdf".into();
        }
        let filename = url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .filter(|name| {
                !name.is_empty() && name.len() <= 255 && !name.chars().any(char::is_control)
            })
            .unwrap_or(match essence(&media_type) {
                "application/pdf" => "source.pdf",
                "text/html" | "application/xhtml+xml" => "source.html",
                _ => "source.txt",
            })
            .to_owned();
        return Ok(FetchedResearchOriginal {
            final_url: url.into(),
            filename,
            media_type,
            bytes: response.bytes,
        });
    }
    Err(SourceFetchError::TooManyRedirects)
}

fn supported_media_type(value: &str) -> bool {
    matches!(
        value,
        "application/pdf" | "text/plain" | "text/html" | "text/markdown" | "application/xhtml+xml"
    )
}

fn essence(value: &str) -> &str {
    value.split(';').next().unwrap_or(value).trim()
}

fn response_content_type(headers: &header::HeaderMap) -> Option<String> {
    let mut values = headers.get_all(header::CONTENT_TYPE).iter();
    let first = normalized_content_type(values.next()?.to_str().ok()?)?;
    if values.next().is_some()
        && !matches!(
            essence(&first),
            "application/pdf" | "application/octet-stream"
        )
    {
        // Conflicting repeated Content-Type fields cannot establish a reliable
        // text encoding. Preserve the original but do not interpret its text.
        return Some(format!("{}; charset=unknown", essence(&first)));
    }
    Some(first)
}

/// Retain only bounded charset parameters: unrelated response headers and
/// arbitrary MIME parameters are not needed as source provenance.
fn normalized_content_type(value: &str) -> Option<String> {
    let kind = essence(value).to_ascii_lowercase();
    if !supported_media_type(&kind) && kind != "application/octet-stream" {
        return None;
    }
    if matches!(
        kind.as_str(),
        "application/pdf" | "application/octet-stream"
    ) {
        return Some(kind);
    }
    let unknown = || format!("{kind}; charset=unknown");
    if value.len() > 1024 {
        return Some(unknown());
    }
    let Some(parsed) = parchmint_content_type::parse(value) else {
        return Some(unknown());
    };
    let mut output = kind.clone();
    let mut count = 0;
    for label in &parsed.charsets {
        count += 1;
        let label = label.as_str();
        if count > 2
            || label.is_empty()
            || label.len() > 64
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Some(unknown());
        }
        output.push_str("; charset=");
        output.push_str(&label.to_ascii_lowercase());
    }
    Some(output)
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113))
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4() {
        return public_v4(v4);
    }
    let s = ip.segments();
    (s[0] & 0xe000) == 0x2000
        && s[0] != 0x2002
        && !(s[0] == 0x2001 && (s[1] <= 0x01ff || s[1] == 0x0db8))
        && !(s[0] == 0x3fff && (s[1] & 0xf000) == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, sync::Mutex};

    struct FakeResolver(Vec<IpAddr>);
    impl Resolver for FakeResolver {
        fn resolve(
            &self,
            _host: &str,
            port: u16,
            _cancelled: &AtomicBool,
        ) -> Result<Vec<SocketAddr>, SourceFetchError> {
            Ok(self.0.iter().map(|ip| SocketAddr::new(*ip, port)).collect())
        }
    }
    #[derive(Default)]
    struct FakeFetcher {
        calls: Mutex<Vec<String>>,
        responses: Mutex<VecDeque<FetchResponse>>,
    }
    impl Fetcher for FakeFetcher {
        fn get(
            &self,
            url: &Url,
            addresses: &[SocketAddr],
            _cancelled: &AtomicBool,
        ) -> Result<FetchResponse, SourceFetchError> {
            assert!(addresses.iter().all(|address| public_ip(address.ip())));
            self.calls.lock().unwrap().push(url.to_string());
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(SourceFetchError::Unavailable)
        }
    }
    fn response(
        status: u16,
        location: Option<&str>,
        media_type: Option<&str>,
        bytes: &[u8],
    ) -> FetchResponse {
        FetchResponse {
            status,
            location: location.map(str::to_owned),
            media_type: media_type.map(str::to_owned),
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn completed_dns_lookup_returns_its_addresses() {
        let address = "8.8.8.8:443".parse().unwrap();
        assert_eq!(
            resolve_with_deadline(
                Arc::new(AtomicUsize::new(0)),
                &AtomicBool::new(false),
                Duration::from_secs(2),
                move || Ok(vec![address]),
            ),
            Ok(vec![address])
        );
    }

    #[test]
    fn stalled_dns_times_out_without_unbounded_resolver_threads() {
        let active = Arc::new(AtomicUsize::new(0));
        let mut releases = Vec::new();
        for _ in 0..MAX_RESOLVERS {
            let (release, blocked) = mpsc::channel::<()>();
            releases.push(release);
            assert_eq!(
                resolve_with_deadline(
                    active.clone(),
                    &AtomicBool::new(false),
                    Duration::from_millis(30),
                    move || {
                        let _ = blocked.recv();
                        Ok(Vec::new())
                    },
                ),
                Err(SourceFetchError::Unavailable)
            );
        }
        assert_eq!(active.load(Ordering::Acquire), MAX_RESOLVERS);
        let started = Arc::new(AtomicBool::new(false));
        let called = started.clone();
        assert_eq!(
            resolve_with_deadline(
                active.clone(),
                &AtomicBool::new(false),
                Duration::from_secs(1),
                move || {
                    called.store(true, Ordering::Release);
                    Ok(Vec::new())
                },
            ),
            Err(SourceFetchError::Unavailable)
        );
        assert!(!started.load(Ordering::Acquire));
        for release in releases {
            release.send(()).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while active.load(Ordering::Acquire) != 0 && Instant::now() < deadline {
            thread::yield_now();
        }
        assert_eq!(active.load(Ordering::Acquire), 0);
    }

    #[test]
    fn cancellation_returns_while_dns_is_still_blocked() {
        let active = Arc::new(AtomicUsize::new(0));
        let cancelled = AtomicBool::new(false);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        thread::scope(|scope| {
            let cancel_flag = &cancelled;
            scope.spawn(move || {
                started_rx.recv().unwrap();
                cancel_flag.store(true, Ordering::Release);
            });
            assert_eq!(
                resolve_with_deadline(
                    active.clone(),
                    &cancelled,
                    Duration::from_secs(2),
                    move || {
                        started_tx.send(()).unwrap();
                        let _ = release_rx.recv();
                        Ok(Vec::new())
                    }
                ),
                Err(SourceFetchError::Cancelled)
            );
        });
        assert_eq!(active.load(Ordering::Acquire), 1);
        release_tx.send(()).unwrap();
    }

    #[test]
    fn public_address_policy_excludes_local_reserved_and_tunneled_destinations() {
        for value in [
            "127.0.0.1",
            "10.2.3.4",
            "100.64.1.1",
            "169.254.169.254",
            "192.0.2.1",
            "198.18.0.1",
            "224.0.0.1",
            "::1",
            "fe80::1",
            "fc00::1",
            "::ffff:127.0.0.1",
            "2002:7f00:1::1",
            "2001:db8::1",
        ] {
            assert!(!public_ip(value.parse().unwrap()), "{value}");
        }
        for value in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
        ] {
            assert!(public_ip(value.parse().unwrap()), "{value}");
        }
    }

    #[test]
    fn source_content_type_preserves_only_bounded_charset_declarations() {
        assert_eq!(
            normalized_content_type("Text/HTML; Charset=\"UTF-8\"; other=private").as_deref(),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(
            normalized_content_type("text/html; charset=utf-8; charset=windows-1252").as_deref(),
            Some("text/html; charset=utf-8; charset=windows-1252")
        );
        assert_eq!(
            normalized_content_type("text/html; charset=\"unfinished").as_deref(),
            Some("text/html; charset=unknown")
        );
        assert_eq!(
            normalized_content_type("application/pdf; charset=ignored").as_deref(),
            Some("application/pdf")
        );
        let bytes = b"<p>\xc3\xa9</p>";
        let fetcher = FakeFetcher::default();
        fetcher.responses.lock().unwrap().push_back(response(
            200,
            None,
            Some("text/html; charset=windows-1252"),
            bytes,
        ));
        let saved = fetch_with(
            "https://example.test/",
            &AtomicBool::new(false),
            &FakeResolver(vec!["8.8.8.8".parse().unwrap()]),
            &fetcher,
        )
        .unwrap();
        assert_eq!(saved.media_type, "text/html; charset=windows-1252");
        assert_eq!(saved.filename, "source.html");
        assert_eq!(saved.bytes, bytes);
        let mut headers = header::HeaderMap::new();
        headers.append(
            header::CONTENT_TYPE,
            "text/html; charset=utf-8".parse().unwrap(),
        );
        headers.append(
            header::CONTENT_TYPE,
            "text/html; charset=windows-1252".parse().unwrap(),
        );
        assert_eq!(
            response_content_type(&headers).as_deref(),
            Some("text/html; charset=unknown")
        );
    }

    #[test]
    fn follows_only_revalidated_public_https_redirects_and_keeps_exact_bytes() {
        let resolver = FakeResolver(vec!["8.8.8.8".parse().unwrap()]);
        let fetcher = FakeFetcher::default();
        fetcher.responses.lock().unwrap().extend([
            response(
                302,
                Some("https://archive.example/source.pdf#page=2"),
                None,
                b"",
            ),
            response(
                200,
                None,
                Some("application/pdf"),
                b"%PDF-1.7 original\r\n\0\xff",
            ),
        ]);
        let result = fetch_with(
            "https://example.test/start",
            &AtomicBool::new(false),
            &resolver,
            &fetcher,
        )
        .unwrap();
        assert_eq!(result.final_url, "https://archive.example/source.pdf");
        assert_eq!(result.filename, "source.pdf");
        assert_eq!(result.bytes, b"%PDF-1.7 original\r\n\0\xff");
        assert_eq!(fetcher.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn refuses_private_dns_credentials_and_redirects_before_fetching_them() {
        let private = FakeResolver(vec![
            "8.8.8.8".parse().unwrap(),
            "10.1.2.3".parse().unwrap(),
        ]);
        let fetcher = FakeFetcher::default();
        assert_eq!(
            fetch_with(
                "https://example.test/source",
                &AtomicBool::new(false),
                &private,
                &fetcher
            ),
            Err(SourceFetchError::UnsafeDestination)
        );
        assert!(fetcher.calls.lock().unwrap().is_empty());
        assert_eq!(
            fetch_with(
                "https://user:secret@example.test/source",
                &AtomicBool::new(false),
                &private,
                &fetcher
            ),
            Err(SourceFetchError::InvalidAddress)
        );
        assert_eq!(
            fetch_with(
                "http://example.test/source",
                &AtomicBool::new(false),
                &private,
                &fetcher
            ),
            Err(SourceFetchError::InvalidAddress)
        );
        let public = FakeResolver(vec!["8.8.8.8".parse().unwrap()]);
        fetcher.responses.lock().unwrap().push_back(response(
            302,
            Some("https://127.0.0.1/admin"),
            None,
            b"",
        ));
        assert_eq!(
            fetch_with(
                "https://example.test/source",
                &AtomicBool::new(false),
                &public,
                &fetcher
            ),
            Err(SourceFetchError::UnsafeDestination)
        );
        assert_eq!(fetcher.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn refuses_unsupported_types_excessive_redirects_and_cancelled_fetches() {
        let resolver = FakeResolver(vec!["8.8.8.8".parse().unwrap()]);
        let fetcher = FakeFetcher::default();
        fetcher.responses.lock().unwrap().push_back(response(
            200,
            None,
            Some("application/x-executable"),
            b"data",
        ));
        assert_eq!(
            fetch_with(
                "https://example.test/source",
                &AtomicBool::new(false),
                &resolver,
                &fetcher
            ),
            Err(SourceFetchError::UnsupportedContent)
        );
        fetcher.responses.lock().unwrap().push_back(response(
            200,
            None,
            Some("application/pdf"),
            b"<html>Not a PDF</html>",
        ));
        assert_eq!(
            fetch_with(
                "https://example.test/source",
                &AtomicBool::new(false),
                &resolver,
                &fetcher
            ),
            Err(SourceFetchError::UnsupportedContent)
        );
        fetcher.responses.lock().unwrap().push_back(response(
            206,
            None,
            Some("application/pdf"),
            b"partial",
        ));
        assert_eq!(
            fetch_with(
                "https://example.test/source",
                &AtomicBool::new(false),
                &resolver,
                &fetcher
            ),
            Err(SourceFetchError::Unavailable)
        );
        fetcher
            .responses
            .lock()
            .unwrap()
            .extend((0..=MAX_REDIRECTS).map(|_| response(302, Some("/again"), None, b"")));
        assert_eq!(
            fetch_with(
                "https://example.test/source",
                &AtomicBool::new(false),
                &resolver,
                &fetcher
            ),
            Err(SourceFetchError::TooManyRedirects)
        );
        let calls = fetcher.calls.lock().unwrap().len();
        assert_eq!(
            fetch_with(
                "https://example.test/source",
                &AtomicBool::new(true),
                &resolver,
                &fetcher
            ),
            Err(SourceFetchError::Cancelled)
        );
        assert_eq!(fetcher.calls.lock().unwrap().len(), calls);
    }

    #[test]
    fn recognizes_pdf_served_as_octet_stream_without_rewriting_it() {
        let resolver = FakeResolver(vec!["8.8.8.8".parse().unwrap()]);
        let fetcher = FakeFetcher::default();
        let bytes = b"%PDF-1.7\r\noriginal";
        fetcher.responses.lock().unwrap().push_back(response(
            200,
            None,
            Some("application/octet-stream"),
            bytes,
        ));
        let fetched = fetch_with(
            "https://example.test/download",
            &AtomicBool::new(false),
            &resolver,
            &fetcher,
        )
        .unwrap();
        assert_eq!(fetched.media_type, "application/pdf");
        assert_eq!(fetched.bytes, bytes);
    }
}
