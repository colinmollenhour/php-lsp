use mir_analyzer::AnalysisSession;
use mir_analyzer::db::{Fqcn, find_class_like};

use crate::types::type_map::ClassMembers;

/// Look up class members for a built-in PHP class by querying phpstorm-stubs
/// through the mir-analyzer session. Returns `None` when `fqcn` is not a
/// known built-in class.
///
/// Replaces the old hardcoded `stubs.rs` lookup with a live query against the
/// phpstorm-stubs embedded in mir-analyzer.
pub fn stub_class_members(session: &AnalysisSession, fqcn: &str) -> Option<ClassMembers> {
    let normalized = fqcn.strip_prefix('\\').unwrap_or(fqcn);
    mir_analyzer::stub_path_for_class(normalized)?;
    session.read(|db| {
        let key = Fqcn::from_str(db, normalized);
        let class_like = find_class_like(db, key)?;
        let mut members = ClassMembers::default();
        members.found = true;
        for (name, method) in class_like.own_methods() {
            members.methods.push((name.to_string(), method.is_static));
        }
        if let Some(props) = class_like.own_properties() {
            for (name, prop) in props {
                members.properties.push((name.to_string(), prop.is_static));
            }
        }
        for name in class_like.own_constants().keys() {
            members.constants.push(name.to_string());
        }
        members.parent = class_like.parent().map(|p| p.to_string());
        Some(members)
    })
}
