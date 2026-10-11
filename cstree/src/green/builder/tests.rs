extern crate std;

use super::*;
use alloc::vec;

fn token(cache: &mut NodeCache<'_>, text: &str) -> GreenElement {
    GreenToken::new(GreenTokenData {
        kind:     RawSyntaxKind(0),
        text:     Some(cache.intern(text)),
        text_len: TextSize::of(text),
    })
    .into()
}

// Inject a matching child hash independently of the hash algorithm or interner.
fn forced(cache: &mut NodeCache<'_>, mut children: Vec<GreenElement>) -> GreenNode {
    assert!(children.len() <= CHILDREN_CACHE_THRESHOLD);
    let len = children.iter().map(GreenElement::text_len).sum();
    cache.get_cached_node(RawSyntaxKind(1), children.drain(..), len, 42)
}

#[test]
fn collisions_preserve_token_order_and_original_cache_entry() {
    let mut cache = NodeCache::new();
    let a = token(&mut cache, "a");
    let b = token(&mut cache, "b");
    let first = forced(&mut cache, vec![a.clone(), b.clone()]);
    let collision = forced(&mut cache, vec![b.clone(), a.clone()]);
    assert!(collision.children().eq([b.as_ref(), a.as_ref()]));
    assert_ne!(first.data.as_ptr(), collision.data.as_ptr());
    assert_eq!(cache.nodes.len(), 1);
    let repeated = forced(&mut cache, vec![a, b]);
    assert_eq!(first.data.as_ptr(), repeated.data.as_ptr());
}

#[test]
fn collisions_check_child_count_and_node_token_variant() {
    let mut cache = NodeCache::new();
    let ab = token(&mut cache, "ab");
    let a = token(&mut cache, "a");
    let b = token(&mut cache, "b");
    let first = forced(&mut cache, vec![ab.clone()]);
    let two = forced(&mut cache, vec![a, b]);
    assert_eq!(two.children().len(), 2);
    let child = GreenNode::new(RawSyntaxKind(0), [ab]);
    let node = forced(&mut cache, vec![child.clone().into()]);
    assert_eq!(
        node.children().next().unwrap().into_node().unwrap().data.as_ptr(),
        child.data.as_ptr()
    );
    assert_ne!(first.data.as_ptr(), node.data.as_ptr());
}

#[test]
fn equal_tokens_and_identical_child_nodes_still_share() {
    let mut cache = NodeCache::new();
    let a = token(&mut cache, "a");
    let first = forced(&mut cache, vec![a]);
    // A fresh token allocation with equal data must still hit.
    let a = token(&mut cache, "a");
    let repeated = forced(&mut cache, vec![a]);
    assert_eq!(first.data.as_ptr(), repeated.data.as_ptr());
    // Use a separate cache to avoid the deliberately forced leaf/root collision.
    let mut parents = NodeCache::new();
    let parent = forced(&mut parents, vec![first.clone().into()]);
    let repeated = forced(&mut parents, vec![first.into()]);
    assert_eq!(parent.data.as_ptr(), repeated.data.as_ptr());
}

#[test]
fn distinct_child_nodes_with_equal_headers_are_not_substituted() {
    let mut cache = NodeCache::new();
    let a = token(&mut cache, "a");
    let b = token(&mut cache, "b");
    let left = GreenNode::new_with_len_and_hash(RawSyntaxKind(0), [a], 1.into(), 7);
    let right = GreenNode::new_with_len_and_hash(RawSyntaxKind(0), [b], 1.into(), 7);
    let first = forced(&mut cache, vec![left.into()]);
    let second = forced(&mut cache, vec![right.clone().into()]);
    assert_ne!(first.data.as_ptr(), second.data.as_ptr());
    assert_eq!(
        second.children().next().unwrap().into_node().unwrap().data.as_ptr(),
        right.data.as_ptr()
    );
}

#[test]
fn deep_equal_children_take_a_conservative_miss_without_recursing() {
    std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(|| {
            let mut cache = NodeCache::new();
            let mut left = vec![GreenNode::new(RawSyntaxKind(0), [])];
            let mut right = vec![GreenNode::new(RawSyntaxKind(0), [])];
            for _ in 0..20_000 {
                left.push(GreenNode::new(RawSyntaxKind(0), [left.last().unwrap().clone().into()]));
                right.push(GreenNode::new(RawSyntaxKind(0), [right.last().unwrap().clone().into()]));
            }
            let first = forced(&mut cache, vec![left.last().unwrap().clone().into()]);
            let second = forced(&mut cache, vec![right.last().unwrap().clone().into()]);
            assert_ne!(first.data.as_ptr(), second.data.as_ptr());
            assert_eq!(
                second.children().next().unwrap().into_node().unwrap().data.as_ptr(),
                right.last().unwrap().data.as_ptr()
            );
            drop((first, second, cache));
            // Retain every level until its parent is dropped: test comparison, not
            // the separate issue of recursive destruction of an unshared chain.
            while let Some(node) = left.pop() {
                drop(node);
            }
            while let Some(node) = right.pop() {
                drop(node);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
