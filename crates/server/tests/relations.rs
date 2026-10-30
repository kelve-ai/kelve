//! How an idea relates, as kelve keeps it: what this kelve proposed stands
//! until the shared graph serves how the idea itself relates, however long
//! the graph takes to review it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::kelve;
use kelve_core::ideas::IdeaKind;
use kelve_core::relations::{RelationKind, RelationState};
use kelve_graph_api::stand_in::StandIn;
use kelve_graph_api::{Change, Changeset, IdeaRelations, Node, Rel};
use kelve_server::pipeline::relations;
use kelve_server::state::Shared;

/// The stand-in graph takes a relation, as a person's kelve sends one.
async fn relate(graph: &StandIn, from: &str, to: &str, why: &str, key: &str) {
    let cs = Changeset {
        agent: "test".into(),
        comment: String::new(),
        changes: vec![Change::Relate {
            from: from.into(),
            kind: RelationKind::RestsOn,
            to: to.into(),
            why: why.into(),
            rank: Some(1),
            model: Some("test-model".into()),
        }],
        written_by: None,
    };
    graph.client(true).apply(&cs, key).await.unwrap();
}

/// What kelve proposed for an idea: that it rests on a database
/// transaction. Kept as if more than an hour ago, so the graph is asked.
async fn proposed(app: &Shared, key: &str) {
    let relations = IdeaRelations {
        down: vec![Rel {
            idea: Node {
                id: "new:database transaction".into(),
                name: "Database transaction".into(),
                foundation: false,
                depth: None,
            },
            why: "It takes the lock's lifetime from the transaction.".into(),
            state: RelationState::Proposed,
            rank: Some(1),
        }],
        ..IdeaRelations::default()
    };
    let mut conn = app.db.acquire().await.unwrap();
    kelve_store::relations::proposed(
        &mut conn,
        key,
        &serde_json::to_string(&relations).unwrap(),
        "How can two programs agree that only one goes on?",
        "[]",
    )
    .await
    .unwrap();
    stale(app).await;
}

/// Everything kept as if fetched long ago.
async fn stale(app: &Shared) {
    let mut conn = app.db.acquire().await.unwrap();
    sqlx::query("update idea_relations set fetched_at = 0")
        .execute(&mut *conn)
        .await
        .unwrap();
}

fn rests_on(known: &relations::Known) -> Vec<&str> {
    known
        .relations
        .down
        .iter()
        .map(|r| r.idea.name.as_str())
        .collect()
}

#[tokio::test]
async fn a_proposal_stands_until_the_graph_serves_how_the_idea_itself_relates() {
    let graph = StandIn::start().await.unwrap();
    let locks = graph.add(
        "Advisory locks",
        "A lock a program names, and the database keeps.",
        IdeaKind::Concept,
    );
    let ledger = graph.add(
        "Spending ledger",
        "A record of what was spent, reserved before and settled after.",
        IdeaKind::Concept,
    );
    // The graph knows something rests on it, and nothing of its own.
    relate(
        &graph,
        &ledger,
        &locks,
        "It takes a lock before it reserves.",
        "one",
    )
    .await;
    let app = kelve(Some(&graph)).await;
    proposed(&app, &locks).await;

    // Asked again, the graph serves only what rests on it: the proposal
    // stands, with that added, so the idea stays traced.
    let known = relations::of(&app, &locks).await.unwrap().expect("known");
    assert!(known.proposed, "still kelve's proposal");
    assert_eq!(rests_on(&known), ["Database transaction"]);
    assert_eq!(known.relations.up.count, 1, "with what rests on it");
    assert!(relations::has_own(&known.relations));
    // And kept so, as fetched now, for the hour.
    let again = relations::of(&app, &locks).await.unwrap().expect("known");
    assert_eq!(rests_on(&again), ["Database transaction"]);
    assert_eq!(again.relations.up.count, 1);
    // Many at once, as the overview asks, the same.
    stale(&app).await;
    let many = relations::many(&app, std::slice::from_ref(&locks))
        .await
        .unwrap();
    assert_eq!(rests_on(&many[&locks]), ["Database transaction"]);
    assert!(many[&locks].proposed);

    // Once the graph serves how the idea itself relates, that's what's
    // shown.
    let tx = graph.add(
        "Database transaction",
        "Work a database does all at once or not at all.",
        IdeaKind::Concept,
    );
    relate(
        &graph,
        &locks,
        &tx,
        "A transaction lock is let go at commit.",
        "two",
    )
    .await;
    stale(&app).await;
    let served = relations::of(&app, &locks).await.unwrap().expect("known");
    assert!(!served.proposed, "the graph's own now");
    assert_eq!(rests_on(&served), ["Database transaction"]);
    assert_eq!(served.relations.down[0].idea.id, tx, "by the graph's ID");
}

#[tokio::test]
async fn relations_followed_go_to_the_graph_once_without_the_account_and_come_back_first() {
    use kelve_core::relations::RelationKind::RestsOn;
    let g = StandIn::start().await.unwrap();
    let jitter = g.add("Jitter", "A random share of a wait.", IdeaKind::Concept);
    let random = g.add("Randomness", "Choosing unpredictably.", IdeaKind::Concept);
    let time = g.add("Time", "What clocks measure.", IdeaKind::Concept);
    g.relate(&jitter, RestsOn, &random, "It picks at random.");
    g.relate(&jitter, RestsOn, &time, "It's a wait.");
    let app = common::kelve(Some(&g)).await;
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::follows::follow(&mut conn, &jitter, "restsOn", &time)
            .await
            .unwrap();
    }
    assert_eq!(kelve_server::follows::send_due(&app).await.unwrap(), 1);
    assert_eq!(
        kelve_server::follows::send_due(&app).await.unwrap(),
        0,
        "once"
    );
    let down: Vec<String> = g
        .client(false)
        .relations(std::slice::from_ref(&jitter))
        .await
        .unwrap()
        .relations[&jitter]
        .down
        .iter()
        .map(|r| r.idea.name.clone())
        .collect();
    assert_eq!(down, ["Time", "Randomness"], "the followed first");
}

#[tokio::test]
async fn an_idea_only_others_relate_to_is_traced_and_keeps_what_they_say() {
    use RelationKind::{RestsOn, UsedWith};
    let graph = StandIn::start().await.unwrap();
    let jitter = graph.add("Jitter", "A random share of a wait.", IdeaKind::Concept);
    let backoff = graph.add(
        "Exponential backoff",
        "Each wait longer than the last.",
        IdeaKind::Concept,
    );
    let random = graph.add("Randomness", "Choosing unpredictably.", IdeaKind::Concept);
    // The graph knows only that backoff is used with jitter: it goes both
    // ways, so it says nothing of what jitter itself rests on.
    graph.relate(
        &backoff,
        UsedWith,
        &jitter,
        "Jitter picks under backoff's ceiling.",
    );
    let app = kelve(Some(&graph)).await;
    let idea = kelve_store::entries::Idea {
        key: jitter.clone(),
        name: "Jitter".into(),
        definition: "A random share of a wait.".into(),
        kind: "concept".into(),
        became: None,
    };
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::entries::keep_idea(&mut conn, &idea)
            .await
            .unwrap();
    }
    let known = relations::of(&app, &jitter).await.unwrap().expect("known");
    assert!(!relations::has_own(&known.relations));
    assert!(
        relations::needed(&app, &idea).await.unwrap(),
        "so the agent is asked"
    );

    // Proposed here, it stands, and keeps what it's used with.
    let proposal = IdeaRelations {
        down: vec![Rel {
            idea: Node {
                id: random.clone(),
                name: "Randomness".into(),
                foundation: false,
                depth: None,
            },
            why: "It picks at random.".into(),
            state: RelationState::Proposed,
            rank: Some(1),
        }],
        ..IdeaRelations::default()
    };
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::relations::proposed(
            &mut conn,
            &jitter,
            &serde_json::to_string(&proposal).unwrap(),
            "Why wait a random time?",
            "[]",
        )
        .await
        .unwrap();
    }
    stale(&app).await;
    let known = relations::of(&app, &jitter).await.unwrap().expect("known");
    assert!(known.proposed);
    assert_eq!(rests_on(&known), ["Randomness"]);
    assert_eq!(
        known.relations.used_with[0].idea.name,
        "Exponential backoff"
    );
    assert!(!relations::needed(&app, &idea).await.unwrap(), "asked once");

    // An idea whose own rests-on the graph serves isn't asked about.
    graph.relate(&jitter, RestsOn, &random, "It picks at random.");
    stale(&app).await;
    let served = relations::of(&app, &jitter).await.unwrap().expect("known");
    assert!(!served.proposed && relations::has_own(&served.relations));
}
