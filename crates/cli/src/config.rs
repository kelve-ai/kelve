//! Where the command line finds kelve: its address, and the secret kelve
//! shares with it (`~/.config/kelve`).

use anyhow::bail;
use kelve_server::config::{read_secret, secret_path};

/// Where the command line reaches kelve, unless `KELVE_URL` says otherwise.
pub const DEFAULT_URL: &str = "http://localhost:4600";

/// What to do when there's no secret yet.
pub const NO_SECRET: &str =
    "Run `kelve open` once; starting kelve makes the secret this command uses.";

/// The secret, or what to do to make one.
pub fn secret() -> anyhow::Result<String> {
    match read_secret(&secret_path()) {
        Ok(Some(secret)) => Ok(secret),
        Ok(None) => bail!(NO_SECRET),
        Err(e) => Err(e.into()),
    }
}

/// kelve's address, as kelve works it out: `KELVE_URL`, else localhost on
/// the port in `KELVE_BIND`, else `http://localhost:4600`.
pub fn url() -> String {
    url_from(
        std::env::var("KELVE_URL").ok(),
        std::env::var("KELVE_BIND").ok(),
    )
}

fn url_from(env: Option<String>, bind: Option<String>) -> String {
    if let Some(url) = env.and_then(|u| parse_url(&u).ok()) {
        return url;
    }
    bind.and_then(|b| b.trim().parse::<std::net::SocketAddr>().ok())
        .map_or_else(
            || DEFAULT_URL.to_string(),
            |b| format!("http://localhost:{}", b.port()),
        )
}

/// Where people open kelve, as the command line tells them: `KELVE_URL`,
/// else `kelve.localhost` on kelve's port. The command line itself reaches
/// kelve at `localhost` (`url`), which every system resolves; outside a
/// browser, not every one resolves `kelve.localhost`.
pub fn address() -> String {
    address_from(
        std::env::var("KELVE_URL").ok(),
        std::env::var("KELVE_BIND").ok(),
    )
}

fn address_from(env: Option<String>, bind: Option<String>) -> String {
    if let Some(url) = env.and_then(|u| parse_url(&u).ok()) {
        return url;
    }
    let port = bind
        .and_then(|b| b.trim().parse::<std::net::SocketAddr>().ok())
        .map_or(4600, |b| b.port());
    format!("http://kelve.localhost:{port}")
}

/// An http or https address, without a trailing slash.
pub fn parse_url(given: &str) -> anyhow::Result<String> {
    let given = given.trim().trim_end_matches('/');
    match url::Url::parse(given) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() => {
            Ok(given.to_string())
        }
        _ => bail!("{given} isn't an address kelve can use. Give one like {DEFAULT_URL}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_wins_when_its_an_address() {
        assert_eq!(
            url_from(Some("http://localhost:4800/".into()), None),
            "http://localhost:4800"
        );
        assert_eq!(url_from(Some("not an address".into()), None), DEFAULT_URL);
        assert_eq!(url_from(None, None), DEFAULT_URL);
        assert_eq!(
            url_from(None, Some("127.0.0.1:4610".into())),
            "http://localhost:4610",
            "as kelve works it out from where it listens"
        );
    }

    #[test]
    fn people_open_kelve_at_its_own_name_unless_told_otherwise() {
        assert_eq!(address_from(None, None), "http://kelve.localhost:4600");
        assert_eq!(
            address_from(None, Some("127.0.0.1:4610".into())),
            "http://kelve.localhost:4610"
        );
        assert_eq!(
            address_from(Some("https://kelve.example.com/".into()), None),
            "https://kelve.example.com"
        );
    }

    #[test]
    fn only_web_addresses_are_taken() {
        assert_eq!(
            parse_url(" https://kelve.example.com/ ").expect("taken"),
            "https://kelve.example.com"
        );
        assert!(parse_url("localhost:4600").is_err());
        assert!(parse_url("ftp://localhost").is_err());
    }
}
