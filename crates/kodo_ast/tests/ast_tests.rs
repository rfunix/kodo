//! Tests for core kodo_ast types: Span, NodeId, NodeIdGen, FixPatch.

use kodo_ast::{FixPatch, NodeId, NodeIdGen, Span};
use proptest::prelude::*;

// ── Span tests ──────────────────────────────────────────────────────

#[test]
fn span_new_creates_correct_span() {
    let span = Span::new(10, 20);
    assert_eq!(span.start, 10);
    assert_eq!(span.end, 20);
}

#[test]
fn span_len_calculates_correctly() {
    let span = Span::new(5, 15);
    assert_eq!(span.len(), 10);
}

#[test]
fn span_len_zero() {
    let span = Span::new(42, 42);
    assert_eq!(span.len(), 0);
}

#[test]
fn span_is_empty_for_zero_length() {
    let span = Span::new(100, 100);
    assert!(span.is_empty());
}

#[test]
fn span_is_not_empty_for_nonzero_length() {
    let span = Span::new(0, 1);
    assert!(!span.is_empty());
}

#[test]
fn span_merge_disjoint() {
    let a = Span::new(5, 10);
    let b = Span::new(20, 30);
    let merged = a.merge(b);
    assert_eq!(merged.start, 5);
    assert_eq!(merged.end, 30);
}

#[test]
fn span_merge_overlapping() {
    let a = Span::new(5, 15);
    let b = Span::new(10, 20);
    let merged = a.merge(b);
    assert_eq!(merged.start, 5);
    assert_eq!(merged.end, 20);
}

#[test]
fn span_merge_adjacent() {
    let a = Span::new(0, 5);
    let b = Span::new(5, 10);
    let merged = a.merge(b);
    assert_eq!(merged.start, 0);
    assert_eq!(merged.end, 10);
}

#[test]
fn span_merge_same_span() {
    let a = Span::new(3, 7);
    let merged = a.merge(a);
    assert_eq!(merged.start, 3);
    assert_eq!(merged.end, 7);
}

#[test]
fn span_merge_reversed_order() {
    let a = Span::new(20, 30);
    let b = Span::new(5, 10);
    let merged = a.merge(b);
    assert_eq!(merged.start, 5);
    assert_eq!(merged.end, 30);
}

#[test]
fn span_merge_contained() {
    let outer = Span::new(0, 100);
    let inner = Span::new(10, 20);
    let merged = outer.merge(inner);
    assert_eq!(merged.start, 0);
    assert_eq!(merged.end, 100);
}

// ── NodeIdGen tests ─────────────────────────────────────────────────

#[test]
fn node_id_gen_starts_at_zero() {
    let mut gen = NodeIdGen::new();
    assert_eq!(gen.next_id(), NodeId(0));
}

#[test]
fn node_id_gen_sequential() {
    let mut gen = NodeIdGen::new();
    assert_eq!(gen.next_id(), NodeId(0));
    assert_eq!(gen.next_id(), NodeId(1));
    assert_eq!(gen.next_id(), NodeId(2));
    assert_eq!(gen.next_id(), NodeId(3));
}

#[test]
fn node_id_gen_default_starts_at_zero() {
    let mut gen = NodeIdGen::default();
    assert_eq!(gen.next_id(), NodeId(0));
}

#[test]
fn node_id_equality() {
    assert_eq!(NodeId(5), NodeId(5));
    assert_ne!(NodeId(5), NodeId(6));
}

// ── FixPatch tests ──────────────────────────────────────────────────

#[test]
fn fix_patch_construction() {
    let patch = FixPatch {
        description: "add missing semicolon".to_string(),
        file: "main.ko".to_string(),
        start_offset: 42,
        end_offset: 42,
        replacement: ";".to_string(),
    };
    assert_eq!(patch.description, "add missing semicolon");
    assert_eq!(patch.file, "main.ko");
    assert_eq!(patch.start_offset, 42);
    assert_eq!(patch.end_offset, 42);
    assert_eq!(patch.replacement, ";");
}

#[test]
fn fix_patch_replacement_range() {
    let patch = FixPatch {
        description: "replace var with let".to_string(),
        file: "test.ko".to_string(),
        start_offset: 10,
        end_offset: 13,
        replacement: "let".to_string(),
    };
    assert_eq!(patch.end_offset - patch.start_offset, 3);
    assert_eq!(patch.replacement.len(), 3);
}

#[test]
fn fix_patch_empty_replacement_is_deletion() {
    let patch = FixPatch {
        description: "remove extra comma".to_string(),
        file: "test.ko".to_string(),
        start_offset: 20,
        end_offset: 21,
        replacement: String::new(),
    };
    assert!(patch.replacement.is_empty());
    assert_eq!(patch.end_offset - patch.start_offset, 1);
}

// ── Property-based tests ────────────────────────────────────────────

proptest! {
    /// Span::new always produces correct start/end.
    #[test]
    fn span_new_roundtrip(start in 0u32..10000, len in 0u32..10000) {
        let end = start.saturating_add(len);
        let span = Span::new(start, end);
        prop_assert_eq!(span.start, start);
        prop_assert_eq!(span.end, end);
        prop_assert_eq!(span.len(), end - start);
    }

    /// Span::is_empty iff len == 0.
    #[test]
    fn span_is_empty_iff_zero_length(start in 0u32..10000, end in 0u32..10000) {
        let (s, e) = if start <= end { (start, end) } else { (end, start) };
        let span = Span::new(s, e);
        prop_assert_eq!(span.is_empty(), s == e);
    }

    /// Merge is commutative: a.merge(b) == b.merge(a).
    #[test]
    fn span_merge_is_commutative(
        s1 in 0u32..10000, e1 in 0u32..10000,
        s2 in 0u32..10000, e2 in 0u32..10000,
    ) {
        let (s1, e1) = if s1 <= e1 { (s1, e1) } else { (e1, s1) };
        let (s2, e2) = if s2 <= e2 { (s2, e2) } else { (e2, s2) };
        let a = Span::new(s1, e1);
        let b = Span::new(s2, e2);
        let ab = a.merge(b);
        let ba = b.merge(a);
        prop_assert_eq!(ab, ba);
    }

    /// NodeIdGen always generates unique sequential IDs.
    #[test]
    fn node_id_gen_unique_sequential(count in 1usize..100) {
        let mut gen = NodeIdGen::new();
        let ids: Vec<NodeId> = (0..count).map(|_| gen.next_id()).collect();
        for (i, id) in ids.iter().enumerate() {
            prop_assert_eq!(*id, NodeId(i as u32));
        }
    }
}
