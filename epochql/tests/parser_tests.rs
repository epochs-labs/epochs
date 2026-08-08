//! EpochQL parser integration tests (grammar v1.0).

use epochql::ast::*;
use epochql::{parse, parse_script};

#[test]
fn parse_use_match_select() {
    let stmt = parse(
        r#"
        USE COMMIT "e8a31f"
        MATCH (n:State)
        SELECT n.payload
        ;
        "#,
    )
    .expect("parse");

    match stmt {
        Statement::Query(q) => {
            assert_eq!(
                q.context,
                Some(ContextClause {
                    target: TargetRef::Commit("e8a31f".into()),
                })
            );
            let m = q.match_clause.expect("match");
            assert_eq!(m.patterns.len(), 1);
            let PatternElement::Node(node) = &m.patterns[0].elements[0] else {
                panic!("expected node");
            };
            assert_eq!(node.variable.as_deref(), Some("n"));
            assert_eq!(node.label.as_deref(), Some("State"));
            let select = q.select.expect("select");
            assert_eq!(select.items.len(), 1);
        }
        other => panic!("expected query, got {other:?}"),
    }
}

#[test]
fn parse_ancestor_path_with_where() {
    let stmt = parse(
        r#"
        MATCH (child:Commit)<-[:PARENT*1..10]-(parent:Commit)
        WHERE child.hash = "f7b10a"
        SELECT parent.hash, parent.timestamp AS ts
        "#,
    )
    .expect("parse");

    let Statement::Query(q) = stmt else {
        panic!("expected query");
    };
    let pattern = &q.match_clause.expect("match").patterns[0];
    assert_eq!(pattern.elements.len(), 3);

    let PatternElement::Edge(edge) = &pattern.elements[1] else {
        panic!("expected edge");
    };
    assert_eq!(edge.direction, EdgeDirection::Incoming);
    assert_eq!(edge.edge_type, EdgeType::Parent);
    assert_eq!(
        edge.multiplier,
        Some(HopMultiplier::Range {
            min: Some(1),
            max: Some(10),
        })
    );

    let where_expr = q.where_clause.expect("where");
    assert!(matches!(
        where_expr,
        Expression::Binary {
            op: BinaryOp::Eq,
            ..
        }
    ));

    let select = q.select.expect("select");
    assert_eq!(select.items.len(), 2);
    assert_eq!(select.items[1].alias.as_deref(), Some("ts"));
}

#[test]
fn parse_bare_arrows() {
    let stmt = parse("MATCH (a)->(b)<-(c) SELECT a").expect("parse");
    let Statement::Query(q) = stmt else {
        panic!("expected query");
    };
    let elems = &q.match_clause.expect("match").patterns[0].elements;
    assert_eq!(elems.len(), 5);

    let PatternElement::Edge(e1) = &elems[1] else {
        panic!();
    };
    assert_eq!(e1.direction, EdgeDirection::Outgoing);
    assert_eq!(e1.edge_type, EdgeType::Child);

    let PatternElement::Edge(e2) = &elems[3] else {
        panic!();
    };
    assert_eq!(e2.direction, EdgeDirection::Incoming);
    assert_eq!(e2.edge_type, EdgeType::Parent);
}

#[test]
fn parse_traverse_merge_base() {
    let stmt = parse(
        r#"
        TRAVERSE MERGE_BASE(BRANCH "agent_thread_1", BRANCH agent_thread_2)
        SELECT hash, timestamp
        "#,
    )
    .expect("parse");

    let Statement::Query(q) = stmt else {
        panic!("expected query");
    };
    assert_eq!(
        q.traversal,
        Some(TraversalClause::MergeBase(
            TargetRef::Branch("agent_thread_1".into()),
            TargetRef::Branch("agent_thread_2".into()),
        ))
    );
}

#[test]
fn parse_commit_with_message_and_parents() {
    let stmt = parse(
        r#"
        COMMIT { memory: "User prefers dark mode", confidence: 0.95 }
        MESSAGE "Update user preferences"
        PARENTS [BRANCH main, BRANCH "hypothesis_a"]
        "#,
    )
    .expect("parse");

    let Statement::Version(VersionStatement::Commit(c)) = stmt else {
        panic!("expected commit");
    };
    assert_eq!(c.message.as_deref(), Some("Update user preferences"));
    assert_eq!(c.payload.len(), 2);
    assert!(matches!(
        c.payload.get("confidence"),
        Some(Expression::Float(_))
    ));
    let parents = c.parents.expect("parents");
    assert_eq!(parents.len(), 2);
}

#[test]
fn parse_branch_create_delete() {
    let create = parse(r#"CREATE BRANCH "sim_run_42" FROM BRANCH main"#).expect("parse");
    assert!(matches!(
        create,
        Statement::Version(VersionStatement::Branch(BranchStmt::Create { .. }))
    ));

    let delete = parse("DELETE BRANCH obsolete").expect("parse");
    assert!(matches!(
        delete,
        Statement::Version(VersionStatement::Branch(BranchStmt::Delete { .. }))
    ));
}

#[test]
fn parse_merge_and_diff() {
    let merge = parse(r#"MERGE BRANCH "plan_alpha" INTO BRANCH main STRATEGY FAST_FORWARD"#)
        .expect("parse");
    match merge {
        Statement::Version(VersionStatement::Merge(m)) => {
            assert_eq!(m.strategy, MergeStrategy::FastForward);
            assert_eq!(m.source, TargetRef::Branch("plan_alpha".into()));
            assert_eq!(m.into, TargetRef::Branch("main".into()));
        }
        other => panic!("unexpected {other:?}"),
    }

    let diff =
        parse(r#"DIFF BRANCH "main" AND BRANCH "sim_run_42" PATH "agent.context""#).expect("parse");
    match diff {
        Statement::Version(VersionStatement::Diff(d)) => {
            assert_eq!(d.path.as_deref(), Some("agent.context"));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn parse_checkout_head() {
    let stmt = parse("CHECKOUT HEAD").expect("parse");
    assert_eq!(
        stmt,
        Statement::Version(VersionStatement::Checkout(TargetRef::Head))
    );
}

#[test]
fn parse_script_multi_hypothesis() {
    let stmts = parse_script(
        r#"
        CREATE BRANCH plan_alpha FROM HEAD;
        CREATE BRANCH plan_beta FROM HEAD;
        USE BRANCH plan_alpha;
        COMMIT { action: "API_CALL", status: "success" } MESSAGE "Executed plan alpha";
        DIFF BRANCH plan_alpha AND BRANCH plan_beta;
        MERGE BRANCH plan_alpha INTO BRANCH main STRATEGY FAST_FORWARD;
        "#,
    )
    .expect("parse script");

    assert_eq!(stmts.len(), 6);
}

#[test]
fn parse_rejects_bad_hash() {
    let err = parse(r#"USE COMMIT "not-hex!""#).expect_err("should fail");
    assert!(err.message.contains("hexadecimal"));
}

#[test]
fn parse_rejects_empty_query() {
    let err = parse(";").expect_err("should fail");
    assert!(err.to_string().contains("expected"));
}

#[test]
fn parse_node_property_filter() {
    let stmt = parse(r#"MATCH (c:Commit {author: "agent_1"}) SELECT c"#).expect("parse");
    let Statement::Query(q) = stmt else {
        panic!();
    };
    let PatternElement::Node(node) = &q.match_clause.unwrap().patterns[0].elements[0] else {
        panic!();
    };
    assert_eq!(
        node.properties.get("author"),
        Some(&Expression::String("agent_1".into()))
    );
}

#[test]
fn parse_outgoing_typed_edge() {
    let stmt = parse("MATCH (a)-[:DESCENDANT*]->(b) SELECT a").expect("parse");
    let Statement::Query(q) = stmt else {
        panic!();
    };
    let PatternElement::Edge(edge) = &q.match_clause.unwrap().patterns[0].elements[1] else {
        panic!();
    };
    assert_eq!(edge.direction, EdgeDirection::Outgoing);
    assert_eq!(edge.edge_type, EdgeType::Descendant);
    assert_eq!(edge.multiplier, Some(HopMultiplier::Any));
}

#[test]
fn parse_line_comments() {
    let stmt = parse(
        r#"
        -- spin off a hypothesis
        CREATE BRANCH experiment FROM HEAD
        "#,
    )
    .expect("parse");
    assert!(matches!(
        stmt,
        Statement::Version(VersionStatement::Branch(_))
    ));
}
