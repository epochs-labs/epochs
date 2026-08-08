//! End-to-end EpochQL executor tests against DiskStore.

use epochql::{Engine, ExecResult};
use epochs_core::DiskStore;

fn temp_repo(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn execute_branch_commit_match_diff_merge() {
    let dir = temp_repo("epochs_exec_e2e");
    let (mut store, _) = DiskStore::init(&dir, "main", "genesis").expect("init");
    let mut engine = Engine::new(&mut store);

    let results = engine
        .execute(
            r#"
            CREATE BRANCH plan_alpha FROM HEAD;
            CHECKOUT BRANCH plan_alpha;
            COMMIT { status: "running", agent_id: "agent_007" } MESSAGE "alpha go";
            "#,
        )
        .expect("script");

    assert!(matches!(results[0], ExecResult::Mutation(_)));
    assert!(matches!(results[2], ExecResult::Mutation(_)));

    let q = engine
        .execute_one(r#"MATCH (c:Commit) WHERE c.status = "running" SELECT c.hash, c.agent_id"#)
        .expect("match");
    let ExecResult::Query(rows) = q else {
        panic!("expected query");
    };
    assert_eq!(rows.rows.len(), 1);
    assert_eq!(rows.rows[0][1], epochql::Value::String("agent_007".into()));

    engine
        .execute(
            r#"
            CHECKOUT BRANCH main;
            CREATE BRANCH plan_beta FROM HEAD;
            CHECKOUT BRANCH plan_beta;
            COMMIT { status: "failed" } MESSAGE "beta go";
            "#,
        )
        .expect("beta");

    let diff = engine
        .execute_one(r#"DIFF BRANCH plan_alpha AND BRANCH plan_beta"#)
        .expect("diff");
    let ExecResult::Mutation(m) = diff else {
        panic!("expected mutation-style diff");
    };
    assert!(m.summary.contains("diff"), "{}", m.summary);

    // Fast-forward main to plan_alpha (alpha is descendant of shared genesis via...
    // wait: plan_alpha forked from genesis, main is still at genesis.
    // plan_alpha tip is descendant of genesis = main tip. FF main <- plan_alpha works.
    engine
        .execute_one(r#"CHECKOUT BRANCH main"#)
        .expect("co main");
    let merge = engine
        .execute_one(r#"MERGE BRANCH plan_alpha INTO BRANCH main STRATEGY FAST_FORWARD"#)
        .expect("merge");
    assert!(matches!(merge, ExecResult::Mutation(_)));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn execute_traverse_merge_base() {
    let dir = temp_repo("epochs_exec_merge_base");
    let (mut store, _) = DiskStore::init(&dir, "main", "genesis").expect("init");
    let mut engine = Engine::new(&mut store);

    engine
        .execute(
            r#"
            CREATE BRANCH a FROM HEAD;
            CREATE BRANCH b FROM HEAD;
            CHECKOUT BRANCH a;
            COMMIT { side: "a" } MESSAGE "on a";
            CHECKOUT BRANCH b;
            COMMIT { side: "b" } MESSAGE "on b";
            "#,
        )
        .expect("branches");

    let result = engine
        .execute_one(r#"TRAVERSE MERGE_BASE(BRANCH a, BRANCH b) SELECT hash, message"#)
        .expect("merge base");

    let ExecResult::Query(q) = result else {
        panic!("expected query");
    };
    assert_eq!(q.rows.len(), 1);
    assert_eq!(q.rows[0][1], epochql::Value::String("genesis".into()));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn execute_parent_edge_match() {
    let dir = temp_repo("epochs_exec_parent_edge");
    let (mut store, _) = DiskStore::init(&dir, "main", "genesis").expect("init");
    let mut engine = Engine::new(&mut store);

    engine
        .execute_one(r#"COMMIT { n: "1" } MESSAGE "child""#)
        .expect("commit");

    let result = engine
        .execute_one(
            r#"
            MATCH (child:Commit)<-[:PARENT]-(parent:Commit)
            WHERE child.message = "child"
            SELECT parent.message
            "#,
        )
        .expect("match");

    let ExecResult::Query(q) = result else {
        panic!("expected query");
    };
    assert!(!q.rows.is_empty());
    assert_eq!(q.rows[0][0], epochql::Value::String("genesis".into()));

    std::fs::remove_dir_all(&dir).ok();
}
