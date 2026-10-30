//! `kelve status`: whether kelve is running, and whether the secret it
//! shares with this command line is there and private.

use kelve_server::config::{read_secret, secret_path};

use std::time::Duration;

use crate::{client, config, style};

pub async fn status() -> anyhow::Result<()> {
    style::head("status");
    let address = config::address();
    if client::running(&config::url()).await {
        // Its version, and the newest out when that's newer, as it says.
        let view = match client::Kelve::new(Duration::from_secs(5)) {
            Ok(kelve) => kelve.send(kelve.get("/api/agent/update")).await.ok(),
            Err(_) => None,
        };
        match view.as_ref().and_then(|v| v["version"].as_str()) {
            Some(version) => style::line(&format!("kelve {version} is running at {address}.")),
            None => style::line(&format!("kelve is running at {address}.")),
        }
        if let Some(newer) = view.as_ref().and_then(|v| v["newer"]["version"].as_str()) {
            style::line(&format!(
                "kelve {newer} is out. Run `kelve update` to update, or ask your agent to."
            ));
        }
    } else {
        style::line(&format!(
            "kelve isn't running at {address}. `kelve open` starts it."
        ));
    }
    let path = secret_path();
    match read_secret(&path) {
        Ok(Some(_)) => style::quiet(&format!(
            "The secret kelve shares with this command line is in {}, and only you can read it.",
            path.display()
        )),
        Ok(None) => style::line(&format!("There's no secret yet. {}", config::NO_SECRET)),
        Err(e) => style::line(&e.to_string()),
    }
    Ok(())
}
