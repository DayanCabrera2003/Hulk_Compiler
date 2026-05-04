//! NodeIdGen / NodeId tests.

use super::*;

#[test]
fn node_id_gen_starts_from_zero_by_default() {
    let mut gen = NodeIdGen::new();
    assert_eq!(gen.next_id(), NodeId(0));
    assert_eq!(gen.next_id(), NodeId(1));
    assert_eq!(gen.next_id(), NodeId(2));
}

#[test]
fn node_id_gen_with_start_uses_the_given_offset() {
    let mut gen = NodeIdGen::with_start(100);
    assert_eq!(gen.next_id(), NodeId(100));
    assert_eq!(gen.next_id(), NodeId(101));
}

#[test]
fn node_id_gen_produces_unique_ids_for_many_nodes() {
    let mut gen = NodeIdGen::new();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..10_000 {
        let id = gen.next_id();
        assert!(seen.insert(id), "duplicate id: {id:?}");
    }
    assert_eq!(seen.len(), 10_000);
}

#[test]
#[should_panic(expected = "NodeIdGen overflowed u32 range")]
fn node_id_gen_panics_on_overflow() {
    let mut gen = NodeIdGen::with_start(u32::MAX);
    let _last = gen.next_id(); // consumes u32::MAX, next call overflows
    let _ = gen.next_id();
}

#[test]
fn node_id_gen_state_is_cloneable_independently() {
    let mut a = NodeIdGen::new();
    a.next_id();
    a.next_id();
    let mut b = a.clone();
    assert_eq!(a.next_id(), NodeId(2));
    assert_eq!(b.next_id(), NodeId(2));
    assert_eq!(a.next_id(), NodeId(3));
    // Generators are independent after cloning.
    assert_eq!(b.next_id(), NodeId(3));
}

#[test]
fn node_id_is_copy_and_hashable() {
    let id = NodeId(42);
    let copied = id;
    let _ = id;
    assert_eq!(copied, NodeId(42));

    let mut map = std::collections::HashMap::new();
    map.insert(NodeId(1), "one");
    map.insert(NodeId(2), "two");
    assert_eq!(map.get(&NodeId(1)), Some(&"one"));
}

#[test]
fn node_ids_are_orderable() {
    // The derived Ord must put smaller ids first — needed by any code that
    // sorts nodes by generation order.
    let mut ids = vec![NodeId(5), NodeId(1), NodeId(3)];
    ids.sort();
    assert_eq!(ids, vec![NodeId(1), NodeId(3), NodeId(5)]);
}
