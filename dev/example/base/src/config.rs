use crate::error::Error;

pub struct Config {
    pub bind: String,
    pub database_url: String,
    pub bank_url: String,
    pub bank_key: String,
    pub webhook_secret: String,
}

impl Config {
    pub fn from_env() -> Result<Config, Error> {
        let var = |name: &'static str| std::env::var(name).map_err(|_| Error::Config(name));
        Ok(Config {
            bind: std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:8080".into()),
            database_url: var("DATABASE_URL")?,
            bank_url: var("BANK_URL")?,
            bank_key: var("BANK_KEY")?,
            webhook_secret: var("WEBHOOK_SECRET")?,
        })
    }
}
