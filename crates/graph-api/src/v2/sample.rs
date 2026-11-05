//! What the stand-ins offer and take, the open one here and the hosted
//! service's own in memory, so both fake runs alike: the kinds of work,
//! agents and models on offer, the skill that makes a run come out better,
//! and the provider keys they take.

use super::{
    AgentOffer, Credential, ExampleCheck, ExampleFile, ExampleTask, KindOffer, ModelOffer, Offer,
};

/// The skill that makes a stand-in's runs come out better, as the runner's
/// stand-in agent does.
pub const EXPERT: &str = "standin-expert";

/// What the stand-ins offer.
pub fn the_offer() -> Offer {
    let model = |model: &str, name: &str| {
        let (provider, provider_name) = if model.starts_with("gpt") {
            ("openai", "OpenAI")
        } else {
            ("anthropic", "Anthropic")
        };
        ModelOffer {
            model: model.into(),
            name: name.into(),
            provider: provider.into(),
            provider_name: provider_name.into(),
        }
    };
    Offer {
        kinds: vec![
            KindOffer {
                kind: "rust".into(),
                name: "Rust".into(),
                about:
                    "Command-line tools, libraries, and fixing bugs in code that already exists."
                        .into(),
                tasks: 12,
                scored: "Hidden tests check each task, and a fixed judge scores what tests can’t."
                    .into(),
                bank: "rust-1".into(),
                examples: vec![greet_name()],
            },
            KindOffer {
                kind: "words".into(),
                name: "Words".into(),
                about: "The words in an app: buttons, errors, settings and empty screens.".into(),
                tasks: 10,
                scored: "A fixed judge scores each task against what it looks for.".into(),
                bank: "words-1".into(),
                examples: Vec::new(),
            },
        ],
        agents: vec![
            AgentOffer {
                agent: "claude-code".into(),
                name: "Claude Code".into(),
                models: vec![
                    model("claude-sonnet-5-6", "Sonnet 5.6"),
                    model("claude-opus-5-5", "Opus 5.5"),
                ],
                credentials: vec![Credential::ApiKey, Credential::Subscription],
            },
            AgentOffer {
                agent: "codex".into(),
                name: "Codex".into(),
                models: vec![model("gpt-5.5", "GPT-5.5")],
                credentials: vec![Credential::ApiKey],
            },
        ],
    }
}

/// A retired Rust task, shown as an example of what the live ones are like.
fn greet_name() -> ExampleTask {
    let check = |kind: &str, about: &str| ExampleCheck {
        kind: kind.into(),
        about: about.into(),
    };
    let file = |path: &str, text: &str| ExampleFile {
        path: path.into(),
        text: text.into(),
    };
    ExampleTask {
        name: "greet-name".into(),
        area: "cli".into(),
        instruction: "Add a --name option to greet, so `greet --name Sam` prints \"Hello, Sam.\" With no name, it still prints \"Hello.\"".into(),
        checks: vec![
            check("test", "`greet --name Sam` prints \"Hello, Sam.\""),
            check("test", "`greet` with no name still prints \"Hello.\""),
            check("static", "No unwrap outside tests."),
            check("judge", "The option's help says what it's for."),
        ],
        files: vec![
            file(
                "Cargo.toml",
                "[package]\nname = \"greet\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            file("src/main.rs", "fn main() {\n    println!(\"Hello.\");\n}\n"),
            file("tests/cli.rs", "// The hidden tests.\n"),
        ],
    }
}

/// Whether a stand-in takes a provider's key: only a test's, starting
/// `sk-test`. The key itself is never kept.
pub fn takes_key(key: &str) -> bool {
    key.starts_with("sk-test")
}
