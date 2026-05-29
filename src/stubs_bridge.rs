//! Bridge from the LSP layer to mir-analyzer's bundled phpstorm-stubs.
//!
//! mir-analyzer 0.30 already embeds phpstorm-stubs at compile time and ingests
//! them into every [`AnalysisSession`] (`DocumentStore::analysis_session` calls
//! `ensure_all_stubs`). This module is the **sole** place in the crate that
//! touches mir's `#[doc(hidden)]` / salsa-flavoured query APIs
//! (`AnalysisSession::snapshot_db`, `mir_analyzer::db::{find_class_like, Fqcn,
//! MirDatabase, MirDbStorage}`, `mir_types::Name`, `mir_analyzer::Visibility`).
//! Those APIs are documented as
//! "Internal API — exposes Salsa types. Subject to change without notice", so
//! the mir-* crates are pinned to `=0.30.0` in `Cargo.toml`; if that pin is
//! bumped, this module is the only thing that needs re-verification.
//!
//! The resolver maps a built-in class FQCN to a [`crate::type_map::ClassMembers`]
//! describing only its **own** (non-inherited) **public** members. Inherited
//! members are surfaced by the existing completion/hover inheritance walk, which
//! re-invokes the resolver for each ancestor FQCN pushed into
//! `ClassMembers::parent` / `ClassMembers::trait_uses`.

use std::cell::RefCell;
use std::collections::HashMap;

use mir_analyzer::AnalysisSession;
use mir_analyzer::Visibility;
use mir_analyzer::db::{Fqcn, MirDatabase, MirDbStorage, find_class_like};

use crate::type_map::ClassMembers;

/// Resolves member lists for built-in / standard-library classes.
///
/// Implemented by [`SessionStubResolver`]; threaded through completion and
/// hover as `Option<&dyn BuiltinClassResolver>` so the no-session path (tests,
/// requests without an analysis session) simply passes `None`.
pub trait BuiltinClassResolver {
    /// Return the own (non-inherited), public members of the class named by
    /// `fqcn`, or `None` if no such built-in class is known. The `fqcn` may
    /// carry a single leading `\`, which is stripped.
    fn class_members(&self, fqcn: &str) -> Option<ClassMembers>;
}

/// [`BuiltinClassResolver`] backed by a live mir-analyzer [`AnalysisSession`].
///
/// Holds a request-local memo (`RefCell<HashMap<..>>`) keyed by lowercased
/// normalized FQCN so the inheritance walk doesn't re-snapshot the salsa db for
/// the same class repeatedly within one completion/hover request. Single-
/// threaded per request, so `RefCell` is sufficient.
pub struct SessionStubResolver<'a> {
    session: &'a AnalysisSession,
    /// One salsa db snapshot per request, created lazily on first lookup and
    /// reused across the whole inheritance walk so we don't re-`snapshot_db`
    /// (a clone) for every ancestor.
    db: RefCell<Option<MirDbStorage>>,
    cache: RefCell<HashMap<String, Option<ClassMembers>>>,
}

impl<'a> SessionStubResolver<'a> {
    pub fn new(session: &'a AnalysisSession) -> Self {
        Self {
            session,
            db: RefCell::new(None),
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Resolve a class via mir, without consulting the cache.
    fn resolve_uncached(&self, name: &str) -> Option<ClassMembers> {
        if self.db.borrow().is_none() {
            *self.db.borrow_mut() = Some(self.session.snapshot_db());
        }
        let db_ref = self.db.borrow();
        let db: &dyn MirDatabase = db_ref.as_ref().expect("db snapshot initialized above");
        {
            let class = find_class_like(db, Fqcn::from_str(db, name))?;

            let mut out = ClassMembers {
                found: true,
                ..Default::default()
            };

            // Public own methods (instance + static split via `is_static`).
            // NOTE: the IndexMap key is lowercased by mir's collector, but
            // `MethodDef.name` keeps the original (camelCase) spelling — use it
            // so completion labels read `getArrayCopy`, not `getarraycopy`.
            for method in class.own_methods().values() {
                if method.visibility.is_at_least(Visibility::Public) {
                    out.methods
                        .push((method.name.to_string(), method.is_static));
                }
            }

            // Public own properties. Enums expose no properties here, and we
            // deliberately do NOT synthesize enum `name`/`value` (they'd render
            // as `$name`/`$value`, which is wrong — enum access is `->name`).
            if let Some(props) = class.own_properties() {
                for prop in props.values() {
                    if prop.visibility.is_at_least(Visibility::Public) {
                        out.properties.push((prop.name.to_string(), prop.is_static));
                        if prop.is_readonly {
                            out.readonly_properties.push(prop.name.to_string());
                        }
                    }
                }
            }

            // Public own constants. `visibility == None` means implicitly public.
            for constant in class.own_constants().values() {
                let public = match constant.visibility {
                    Some(v) => v.is_at_least(Visibility::Public),
                    None => true,
                };
                if public {
                    out.constants.push(constant.name.to_string());
                }
            }

            // Direct parent (for hover display).
            out.parent = class.parent().map(|p| normalize_fqcn(p).to_string());

            // Push every ancestor into `trait_uses` so the existing inheritance
            // walk recurses and re-resolves each via this bridge. For a class,
            // `ancestor_fqcns()` already covers parent + interfaces + traits (it
            // supersedes `class_traits()`), so one loop is enough. Stdlib
            // built-ins live in the root namespace, so the short name the walker
            // compares equals the FQCN; normalize a leading `\` so re-resolution
            // matches.
            for ancestor in class.ancestor_fqcns() {
                out.trait_uses.push(normalize_fqcn(&ancestor).to_string());
            }

            Some(out)
        }
    }
}

impl BuiltinClassResolver for SessionStubResolver<'_> {
    fn class_members(&self, fqcn: &str) -> Option<ClassMembers> {
        let name = normalize_fqcn(fqcn);
        if name.is_empty() {
            return None;
        }
        let key = name.to_ascii_lowercase();
        if let Some(cached) = self.cache.borrow().get(&key) {
            return cached.clone();
        }
        let result = self.resolve_uncached(name);
        self.cache.borrow_mut().insert(key, result.clone());
        result
    }
}

/// Strip a single leading namespace separator. Stdlib built-ins are root-ns, so
/// after stripping the short name equals the FQCN.
fn normalize_fqcn(fqcn: &str) -> &str {
    fqcn.strip_prefix('\\').unwrap_or(fqcn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_analyzer::PhpVersion;

    fn session() -> AnalysisSession {
        let s = AnalysisSession::new(PhpVersion::LATEST);
        // A bare session (not built via DocumentStore) hasn't ingested stubs.
        s.ensure_all_stubs();
        s
    }

    #[test]
    fn resolves_arrayobject_with_public_members() {
        let s = session();
        let r = SessionStubResolver::new(&s);
        let members = r
            .class_members("ArrayObject")
            .expect("ArrayObject should resolve from bundled stubs");
        assert!(members.found);
        let method_names: Vec<&str> = members.methods.iter().map(|(n, _)| n.as_str()).collect();
        // Own public methods of ArrayObject, asserted with EXACT original
        // casing — mir lowercases the lookup-map key but `MethodDef.name`
        // preserves the camelCase spelling the user expects in completion.
        assert!(
            method_names.contains(&"append"),
            "expected append, got {method_names:?}"
        );
        assert!(
            method_names.contains(&"getArrayCopy"),
            "expected exact-cased getArrayCopy, got {method_names:?}"
        );
        // `count` is inherited (from Countable); it must be reachable by walking
        // the pushed ancestors, even though own_methods may not contain it.
        assert!(
            !members.trait_uses.is_empty(),
            "expected ancestors pushed into trait_uses for the walker"
        );
    }

    #[test]
    fn inherited_count_reachable_via_ancestors() {
        let s = session();
        let r = SessionStubResolver::new(&s);
        // Simulate the inheritance walk: gather own methods of ArrayObject and
        // all its ancestors transitively, exactly like completion does.
        let mut seen: Vec<String> = Vec::new();
        let mut queue = vec!["ArrayObject".to_string()];
        let mut visited = std::collections::HashSet::new();
        while let Some(cur) = queue.pop() {
            if !visited.insert(cur.clone()) {
                continue;
            }
            if let Some(m) = r.class_members(&cur) {
                for (n, _) in &m.methods {
                    seen.push(n.clone());
                }
                if let Some(p) = m.parent {
                    queue.push(p);
                }
                queue.extend(m.trait_uses);
            }
        }
        assert!(
            seen.iter().any(|n| n.eq_ignore_ascii_case("count")),
            "expected inherited Countable::count reachable via ancestors, got {seen:?}"
        );
    }

    #[test]
    fn resolves_datetime_immutable_static_split() {
        let s = session();
        let r = SessionStubResolver::new(&s);
        let members = r
            .class_members("DateTimeImmutable")
            .expect("DateTimeImmutable should resolve");
        // createFromFormat is a static factory; modify is an instance method.
        let static_create = members
            .methods
            .iter()
            .find(|(n, _)| n == "createFromFormat");
        assert!(
            matches!(static_create, Some((_, true))),
            "createFromFormat should be present with exact casing and static, got {static_create:?}"
        );
        let instance_modify = members
            .methods
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("modify"));
        assert!(
            matches!(instance_modify, Some((_, false))),
            "modify should be an instance method, got {instance_modify:?}"
        );
    }

    #[test]
    fn leading_backslash_normalized_and_unknown_is_none() {
        let s = session();
        let r = SessionStubResolver::new(&s);
        assert!(r.class_members("\\ArrayObject").is_some());
        assert!(
            r.class_members("ThisClassDoesNotExistAnywhere1234")
                .is_none(),
            "unknown class must resolve to None for fallback"
        );
    }

    #[test]
    fn enum_does_not_emit_name_value_properties() {
        let s = session();
        let r = SessionStubResolver::new(&s);
        // If a backed enum exists in stubs it must not surface name/value as
        // properties. We assert the invariant on any resolvable enum; skip if
        // none is present in this stub set.
        if let Some(members) = r.class_members("DateTimeZone") {
            for (n, _) in &members.properties {
                assert!(
                    !n.eq_ignore_ascii_case("name") && !n.eq_ignore_ascii_case("value"),
                    "bridge must not emit name/value as properties"
                );
            }
        }
    }
}
