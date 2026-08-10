//! Integration tests for CAS, HAMT, branching, and persistence.

use epochs_core::{merge_base, DagStore, DiskStore, HamtOp, Hash, MemCas, PersistentHamt};

fn temp_repo(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(name)
}

#[test]
fn hamt_insert_get_and_collision() {
    let mut cas = MemCas::new();
    let root = PersistentHamt::insert(&mut cas, None, b"a", b"1").expect("insert a");
    let root = PersistentHamt::insert(&mut cas, Some(root), b"b", b"2").expect("insert b");

    assert_eq!(
        PersistentHamt::get(&mut cas, Some(root), b"a").expect("get"),
        Some(b"1".to_vec())
    );
    assert_eq!(
        PersistentHamt::get(&mut cas, Some(root), b"b").expect("get"),
        Some(b"2".to_vec())
    );
}

#[test]
fn branching_time_travel() {
    let dir = temp_repo("epochs_branching_test");
    let _ = std::fs::remove_dir_all(&dir);

    let (mut store, root_hash) = DiskStore::init(&dir, "main", "genesis").expect("init");

    let hamt_v1 = store
        .commit(
            vec![root_hash],
            Some(Hash::ZERO),
            &[
                HamtOp::Put {
                    key: b"agent_id".to_vec(),
                    value: b"agent_007".to_vec(),
                },
                HamtOp::Put {
                    key: b"status".to_vec(),
                    value: b"idle".to_vec(),
                },
            ],
            "Initial state",
        )
        .expect("commit v1");

    let commit_v1 = store.get_commit(&hamt_v1).expect("load v1");
    let root_v1 = commit_v1.root_hamt;

    let hamt_alpha = store
        .commit(
            vec![hamt_v1],
            Some(root_v1),
            &[HamtOp::Put {
                key: b"status".to_vec(),
                value: b"running".to_vec(),
            }],
            "Alpha branch",
        )
        .expect("alpha");
    let commit_alpha = store.get_commit(&hamt_alpha).expect("load alpha");

    let hamt_beta = store
        .commit(
            vec![hamt_v1],
            Some(root_v1),
            &[HamtOp::Put {
                key: b"status".to_vec(),
                value: b"failed".to_vec(),
            }],
            "Beta branch",
        )
        .expect("beta");
    let commit_beta = store.get_commit(&hamt_beta).expect("load beta");

    assert_eq!(
        store.get(root_v1, b"status").expect("get"),
        Some(b"idle".to_vec())
    );
    assert_eq!(
        store.get(commit_alpha.root_hamt, b"status").expect("get"),
        Some(b"running".to_vec())
    );
    assert_eq!(
        store.get(commit_beta.root_hamt, b"status").expect("get"),
        Some(b"failed".to_vec())
    );

    assert_eq!(
        store.get(commit_alpha.root_hamt, b"agent_id").expect("get"),
        Some(b"agent_007".to_vec())
    );

    assert_ne!(hamt_alpha, hamt_beta);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn hamt_structural_sharing() {
    let dir = temp_repo("epochs_sharing_test");
    let _ = std::fs::remove_dir_all(&dir);

    let (mut store, root_hash) = DiskStore::init(&dir, "main", "genesis").expect("init");

    let v1 = store
        .commit(
            vec![root_hash],
            Some(Hash::ZERO),
            &[
                HamtOp::Put {
                    key: b"agent_id".to_vec(),
                    value: b"agent_007".to_vec(),
                },
                HamtOp::Put {
                    key: b"status".to_vec(),
                    value: b"idle".to_vec(),
                },
            ],
            "v1",
        )
        .expect("v1");

    let root_v1 = store.get_commit(&v1).expect("load").root_hamt;

    let alpha = store
        .commit(
            vec![v1],
            Some(root_v1),
            &[HamtOp::Put {
                key: b"status".to_vec(),
                value: b"running".to_vec(),
            }],
            "alpha",
        )
        .expect("alpha");

    let beta = store
        .commit(
            vec![v1],
            Some(root_v1),
            &[HamtOp::Put {
                key: b"status".to_vec(),
                value: b"failed".to_vec(),
            }],
            "beta",
        )
        .expect("beta");

    let root_alpha = store.get_commit(&alpha).expect("load").root_hamt;
    let root_beta = store.get_commit(&beta).expect("load").root_hamt;

    let leaf_alpha = store
        .hamt_leaf_hash(root_alpha, b"agent_id")
        .expect("leaf")
        .expect("exists");
    let leaf_beta = store
        .hamt_leaf_hash(root_beta, b"agent_id")
        .expect("leaf")
        .expect("exists");

    assert_eq!(leaf_alpha, leaf_beta);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn repo_persistence() {
    let dir = temp_repo("epochs_persist_test");
    let _ = std::fs::remove_dir_all(&dir);

    let (mut store, commit) = DiskStore::init(&dir, "main", "genesis").expect("init");
    store
        .commit(
            vec![commit],
            Some(Hash::ZERO),
            &[HamtOp::Put {
                key: b"k".to_vec(),
                value: b"v".to_vec(),
            }],
            "second",
        )
        .expect("commit");
    drop(store);

    let mut store2 = DiskStore::open(&dir).expect("reopen");
    let loaded = store2.checkout("main").expect("checkout");
    assert_eq!(loaded.message, "genesis");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn merge_base_finds_root() {
    let dir = temp_repo("epochs_merge_base_test");
    let _ = std::fs::remove_dir_all(&dir);

    let (mut store, root) = DiskStore::init(&dir, "main", "genesis").expect("init");

    let main_a = store
        .commit(vec![root], Some(Hash::ZERO), &[], "main-a")
        .expect("main-a");
    let feature = store
        .commit(vec![root], Some(Hash::ZERO), &[], "feature")
        .expect("feature");

    let base = merge_base(&mut store, main_a, feature).expect("merge base");
    assert_eq!(base, Some(root));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dag_topology_multi_branch() {
    let dir = temp_repo("epochs_dag_topology_test");
    let _ = std::fs::remove_dir_all(&dir);

    let (mut store, root) = DiskStore::init(&dir, "main", "genesis").expect("init");

    store
        .create_branch("feature", root)
        .expect("branch feature");

    let main_a = store
        .commit(
            vec![root],
            Some(Hash::ZERO),
            &[HamtOp::Put {
                key: b"branch".to_vec(),
                value: b"main".to_vec(),
            }],
            "main-1",
        )
        .expect("main-a");
    store.update_branch("main", main_a).expect("advance main");

    let feature_hash = store
        .commit(
            vec![root],
            Some(Hash::ZERO),
            &[HamtOp::Put {
                key: b"branch".to_vec(),
                value: b"feature".to_vec(),
            }],
            "feature-1",
        )
        .expect("feature");
    store
        .update_branch("feature", feature_hash)
        .expect("advance feature");

    let main_a_root = store.get_commit(&main_a).expect("load").root_hamt;
    let main_b = store
        .commit(
            vec![main_a],
            Some(main_a_root),
            &[HamtOp::Put {
                key: b"branch".to_vec(),
                value: b"main-2".to_vec(),
            }],
            "main-2",
        )
        .expect("main-b");

    let main_a_commit = store.get_commit(&main_a).expect("load");
    let main_b_commit = store.get_commit(&main_b).expect("load");
    let feature_commit = store.get_commit(&feature_hash).expect("load");

    assert_eq!(main_a_commit.parents, vec![root]);
    assert_eq!(main_b_commit.parents, vec![main_a]);
    assert_eq!(feature_commit.parents, vec![root]);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn disk_get_object_returns_commit_payload() {
    let dir = temp_repo(&format!("epochs_get_object_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let (mut store, root) = DiskStore::init(&dir, "main", "genesis").expect("init");
    let (ty, payload) = store.get_object(&root).expect("get_object");
    assert_eq!(ty, epochs_core::RecordType::Commit);
    assert!(!payload.is_empty());

    let missing = Hash::from_hex(&"ab".repeat(32)).expect("hash");
    assert!(store.get_object(&missing).is_err());

    std::fs::remove_dir_all(&dir).ok();
}
