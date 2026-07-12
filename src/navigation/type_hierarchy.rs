/// `textDocument/prepareTypeHierarchy`, `typeHierarchy/supertypes`, `typeHierarchy/subtypes`.
use std::collections::HashSet;
use std::sync::Arc;

use tower_lsp::lsp_types::{Position, SymbolKind, TypeHierarchyItem, Url};

use crate::text::zero_width_range;

fn make_item_from_index(
    name: &str,
    kind: SymbolKind,
    uri: &Url,
    start_line: u32,
) -> TypeHierarchyItem {
    let range = zero_width_range(start_line);
    TypeHierarchyItem {
        name: name.to_string(),
        kind,
        tags: None,
        detail: None,
        uri: uri.clone(),
        range,
        selection_range: range,
        data: None,
    }
}

/// Phase J — Prepare from the salsa-memoized workspace aggregate. Constant-time
/// name lookup via `classes_by_name` instead of walking every file's classes.
pub fn prepare_type_hierarchy_from_workspace(
    source: &str,
    wi: &crate::db::workspace_index::WorkspaceIndexData,
    position: Position,
) -> Option<TypeHierarchyItem> {
    use crate::index::file_index::ClassKind;
    use crate::text::word_at_position;
    let word = word_at_position(source, position)?;
    let refs = wi.classes_by_name.get(&word)?;
    let (uri, cls) = wi.at(*refs.first()?)?;
    let kind = match cls.kind {
        ClassKind::Class | ClassKind::Trait => SymbolKind::CLASS,
        ClassKind::Interface => SymbolKind::INTERFACE,
        ClassKind::Enum => SymbolKind::ENUM,
    };
    Some(make_item_from_index(&cls.name, kind, uri, cls.start_line))
}

/// Phase J — Supertypes via the aggregate. Collect parent/interface names from
/// every declaration of `item.name`, then resolve each name through
/// `classes_by_name`. O(definitions-of-item + parents) instead of O(files × classes).
pub fn supertypes_of_from_workspace(
    item: &TypeHierarchyItem,
    wi: &crate::db::workspace_index::WorkspaceIndexData,
) -> Vec<TypeHierarchyItem> {
    use crate::index::file_index::ClassKind;
    // Collect (super_name_as_written, file_index_for_that_class) pairs.
    let mut super_pairs: Vec<(Arc<str>, Option<&crate::index::file_index::FileIndex>)> = Vec::new();
    if let Some(refs) = wi.classes_by_name.get(&item.name) {
        for r in refs {
            if let Some((_, cls)) = wi.at(*r) {
                let file_idx = wi.files.get(r.file as usize).map(|(_, idx)| idx.as_ref());
                if let Some(p) = &cls.parent {
                    super_pairs.push((Arc::clone(p), file_idx));
                }
                for iface in &cls.implements {
                    super_pairs.push((Arc::clone(iface), file_idx));
                }
            }
        }
    }

    let mut result = Vec::new();
    // `wi.classes_by_name` is keyed by short name only, so distinct classes
    // sharing a name (e.g. two `BlogController`s in different namespaces)
    // each contribute their own `super_pairs` entries above; when both extend
    // the same parent, that parent would otherwise be pushed twice. Dedup on
    // FQN to collapse those repeats without narrowing which parents count.
    let mut seen_fqns: HashSet<Box<str>> = HashSet::new();
    for (name, file_idx) in super_pairs {
        // Direct lookup: class is named exactly as written.
        let canonical = if wi.classes_by_name.contains_key(name.as_ref()) {
            Some(name.as_ref().to_string())
        } else {
            // Resolve through the implementing file's use_imports.
            file_idx.and_then(|idx| {
                idx.use_imports
                    .iter()
                    .find(|(alias, _)| alias.as_ref() == name.as_ref())
                    .map(|(_, fqn)| crate::text::fqn_short_name(fqn).to_string())
            })
        };
        let Some(canonical_name) = canonical else {
            continue;
        };
        if let Some(refs) = wi.classes_by_name.get(&canonical_name)
            && let Some((uri, cls)) = refs.first().and_then(|r| wi.at(*r))
            && seen_fqns.insert(cls.fqn.clone())
        {
            let kind = match cls.kind {
                ClassKind::Class | ClassKind::Trait => SymbolKind::CLASS,
                ClassKind::Interface => SymbolKind::INTERFACE,
                ClassKind::Enum => SymbolKind::ENUM,
            };
            result.push(make_item_from_index(&cls.name, kind, uri, cls.start_line));
        }
    }
    result
}

/// Mir-backed variant of [`subtypes_of_from_workspace`].
///
/// `item_fqn` is the FQCN of the hierarchy item (e.g. `"App\\Animal"`),
/// resolved in the handler from the workspace index. `subtype_urls` is the
/// file set from `DocumentStore::class_subtype_urls`. When non-empty this
/// fixes aliased `extends` and FQN-qualified forms the raw-name map misses.
/// Falls back to [`subtypes_of_from_workspace`] when `subtype_urls` is empty.
pub fn subtypes_of_mir_backed(
    item: &TypeHierarchyItem,
    item_fqn: Option<&str>,
    wi: &crate::db::workspace_index::WorkspaceIndexData,
    subtype_urls: &[Url],
) -> Vec<TypeHierarchyItem> {
    if subtype_urls.is_empty() {
        return subtypes_of_from_workspace(item, wi);
    }
    use crate::index::file_index::ClassKind;
    use crate::navigation::implementation::{alias_resolves_to, name_matches};
    let url_set: HashSet<&Url> = subtype_urls.iter().collect();
    let mut result = Vec::new();
    for (uri, idx) in &wi.files {
        if !url_set.contains(uri) {
            continue;
        }
        for cls in &idx.classes {
            let extends_match = cls
                .parent
                .as_deref()
                .map(|p| {
                    name_matches(p, &item.name, item_fqn)
                        || item_fqn.is_some_and(|f| alias_resolves_to(p, f, &idx.use_imports))
                })
                .unwrap_or(false);
            let implements_match = cls.implements.iter().any(|iface| {
                name_matches(iface.as_ref(), &item.name, item_fqn)
                    || item_fqn
                        .is_some_and(|f| alias_resolves_to(iface.as_ref(), f, &idx.use_imports))
            });
            let uses_match = cls.traits.iter().any(|t| {
                name_matches(t.as_ref(), &item.name, item_fqn)
                    || item_fqn.is_some_and(|f| alias_resolves_to(t.as_ref(), f, &idx.use_imports))
            });
            if extends_match || implements_match || uses_match {
                let kind = match cls.kind {
                    ClassKind::Class | ClassKind::Trait => SymbolKind::CLASS,
                    ClassKind::Interface => SymbolKind::INTERFACE,
                    ClassKind::Enum => SymbolKind::ENUM,
                };
                result.push(make_item_from_index(&cls.name, kind, uri, cls.start_line));
            }
        }
    }
    result
}

/// Phase J — Subtypes via the pre-built `subtypes_of` reverse map. O(matches)
/// instead of O(files × classes).
pub fn subtypes_of_from_workspace(
    item: &TypeHierarchyItem,
    wi: &crate::db::workspace_index::WorkspaceIndexData,
) -> Vec<TypeHierarchyItem> {
    use crate::index::file_index::ClassKind;
    let Some(refs) = wi.subtypes_of.get(item.name.as_str()) else {
        return Vec::new();
    };
    refs.iter()
        .filter_map(|r| wi.at(*r))
        .map(|(uri, cls)| {
            let kind = match cls.kind {
                ClassKind::Class | ClassKind::Trait => SymbolKind::CLASS,
                ClassKind::Interface => SymbolKind::INTERFACE,
                ClassKind::Enum => SymbolKind::ENUM,
            };
            make_item_from_index(&cls.name, kind, uri, cls.start_line)
        })
        .collect()
}
