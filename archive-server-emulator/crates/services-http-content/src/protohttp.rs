//! Recognition of the DirtySDK **ProtoHttp** HTTP client.
//!
//! # Evidence
//!
//! * The user-agent format string lives in `FIFA22.exe` at file offset
//!   `0x7575C00` (section `.xtext`):
//!
//!   ```text
//!   ProtoHttp %d.%d/DS %d.%d.%d.%d.%d (Windows)
//!   ```
//!
//!   The same cluster (`0x7575C00`-`0x7575E00`) holds the HTTP method
//!   names, the `http` / `https` schemes, and the request template
//!   `%s %s%s HTTP/1.1`.
//!
//! * The value was observed live in `emulator_server.log` (the
//!   `/redirector/getServerInstance` request):
//!
//!   ```text
//!   User-Agent: ProtoHttp 1.3/DS 15.1.7.0.3 (Windows)
//!   ```
//!
//! # What a ProtoHttp client needs from a response
//!
//! ProtoHttp is a plain HTTP/1.1 client.  Its response parser only
//! recognises `content-length`, `transfer-encoding`, `chunked`,
//! `location`, `last-modified` and `connection` (file offsets
//! `0x7575CC0`-`0x7575D08`), so a response must:
//!
//! * be framed by an exact `Content-Length` — never chunked, and
//! * close the connection, because the client sends `Connection: Close`.
//!
//! See [`crate::HttpResponse::for_proto_http`].

/// The canonical ProtoHttp user-agent format string, verbatim from
/// `FIFA22.exe` (file offset `0x7575C00`).
pub const PROTO_HTTP_USER_AGENT_FORMAT: &str = "ProtoHttp %d.%d/DS %d.%d.%d.%d.%d (Windows)";

/// The `User-Agent` this emulator reports back to a ProtoHttp client.
///
/// **ASSUMPTION.** The client does not read a `User-Agent` *response*
/// header — the ProtoHttp response parser only looks at `content-length`,
/// `transfer-encoding`, `chunked`, `location`, `last-modified` and
/// `connection` (file offsets `0x7575CC0`-`0x7575D08`).  Sending one is
/// harmless, makes the ProtoHttp path observable on the wire, and keeps a
/// version-sensitive client from seeing an unknown stack.
pub const PROTO_HTTP_SERVER_USER_AGENT: &str = "ProtoHttp 1.3/DS 15.1.7.0.3 (Windows)";

/// A parsed ProtoHttp user agent.
///
/// The two version pairs are kept separate because they mean different
/// things: `http_version` is the version of the ProtoHttp *library* the
/// client was built against, and `ds_version` is the version of the
/// DirtySDK *stack* it is running on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtoHttpUserAgent {
    /// The ProtoHttp library version, e.g. `(1, 3)`.
    pub http_version: (u32, u32),
    /// The DirtySDK version, e.g. `[15, 1, 7, 0, 3]`.
    pub ds_version: [u32; 5],
    /// The platform token from the trailing parenthesised group, e.g.
    /// `Some("Windows")`.  Absent when the client omits it.
    pub platform: Option<String>,
}

impl ProtoHttpUserAgent {
    /// Parses a ProtoHttp user agent.
    ///
    /// Accepts the format FIFA 22 emits — `ProtoHttp 1.3/DS 15.1.7.0.3
    /// (Windows)` — and the same string without the trailing platform
    /// token, which is the form the open-ds2 connection analysis shows.
    ///
    /// Anything that is not a ProtoHttp user agent yields `None`.
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        let rest = raw.strip_prefix("ProtoHttp ")?;

        let (http_part, ds_part) = rest.split_once('/')?;
        let http_version = parse_version_pair(http_part)?;

        let ds_part = ds_part.trim();
        let ds_part = ds_part.strip_prefix("DS")?;
        let ds_part = ds_part.trim_start();

        // The platform token, if present, is the trailing parenthesised
        // group: `ProtoHttp 1.3/DS 15.1.7.0.3 (Windows)`.
        let (numbers, platform) = match ds_part.find('(') {
            Some(open) => {
                let close = ds_part[open..]
                    .find(')')
                    .map(|i| open + i)
                    .unwrap_or(ds_part.len());
                let token = ds_part[open + 1..close].trim();
                let platform = if token.is_empty() {
                    None
                } else {
                    Some(token.to_string())
                };
                (ds_part[..open].trim(), platform)
            }
            None => (ds_part, None),
        };

        let ds_version = parse_version_list(numbers, 5)?;

        Some(Self {
            http_version,
            ds_version,
            platform,
        })
    }

    /// The DirtySDK version as a dotted string, e.g. `15.1.7.0.3`.
    pub fn ds_version_string(&self) -> String {
        join_version(&self.ds_version)
    }

    /// True when the client reported a Windows platform.
    pub fn is_windows(&self) -> bool {
        matches!(self.platform.as_deref(), Some(p) if p.eq_ignore_ascii_case("windows"))
    }

    /// Renders the user agent back into its canonical textual form.
    pub fn to_user_agent(&self) -> String {
        match &self.platform {
            Some(platform) => format!(
                "ProtoHttp {}.{}/DS {} ({})",
                self.http_version.0,
                self.http_version.1,
                self.ds_version_string(),
                platform
            ),
            None => format!(
                "ProtoHttp {}.{}/DS {}",
                self.http_version.0,
                self.http_version.1,
                self.ds_version_string()
            ),
        }
    }
}

/// Parses a ProtoHttp user agent, or returns `None`.
pub fn parse_proto_http_user_agent(raw: &str) -> Option<ProtoHttpUserAgent> {
    ProtoHttpUserAgent::parse(raw)
}

/// Parses `major.minor`.
fn parse_version_pair(text: &str) -> Option<(u32, u32)> {
    let text = text.trim();
    let (major, minor) = text.split_once('.')?;
    Some((major.trim().parse().ok()?, minor.trim().parse().ok()?))
}

/// Parses exactly `count` dot-separated numbers.
fn parse_version_list(text: &str, count: usize) -> Option<[u32; 5]> {
    let parts: Vec<&str> = text.trim().split('.').map(str::trim).collect();
    if parts.len() != count {
        return None;
    }
    let mut out = [0u32; 5];
    for (slot, part) in out.iter_mut().zip(parts.iter()) {
        *slot = part.parse().ok()?;
    }
    Some(out)
}

/// Joins a version list into `a.b.c.d.e`.
fn join_version(parts: &[u32]) -> String {
    parts
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_user_agent_fifa_22_sends() {
        let ua = ProtoHttpUserAgent::parse("ProtoHttp 1.3/DS 15.1.7.0.3 (Windows)")
            .expect("the live FIFA 22 user agent must parse");
        assert_eq!(ua.http_version, (1, 3));
        assert_eq!(ua.ds_version, [15, 1, 7, 0, 3]);
        assert_eq!(ua.platform.as_deref(), Some("Windows"));
        assert!(ua.is_windows());
        assert_eq!(ua.ds_version_string(), "15.1.7.0.3");
    }

    #[test]
    fn parses_the_user_agent_without_a_platform_token() {
        let ua = ProtoHttpUserAgent::parse("ProtoHttp 1.3/DS 15.1.7.0.3")
            .expect("the platform token is optional");
        assert_eq!(ua.http_version, (1, 3));
        assert_eq!(ua.ds_version, [15, 1, 7, 0, 3]);
        assert_eq!(ua.platform, None);
        assert!(!ua.is_windows());
    }

    #[test]
    fn rejects_user_agents_that_are_not_proto_http() {
        for raw in [
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
            "ProtoHttp",
            "ProtoHttp 1.3",
            "",
        ] {
            assert!(
                ProtoHttpUserAgent::parse(raw).is_none(),
                "{raw:?} must not be recognised as a ProtoHttp user agent"
            );
        }
    }

    #[test]
    fn round_trips_through_its_canonical_form() {
        for raw in [
            "ProtoHttp 1.3/DS 15.1.7.0.3 (Windows)",
            "ProtoHttp 1.3/DS 15.1.7.0.3",
        ] {
            let ua = ProtoHttpUserAgent::parse(raw).expect("must parse");
            assert_eq!(ua.to_user_agent(), raw);
        }
    }
}
