//! GraphQL schema browser model: flatten the schema into visible rows, and the
//! checkbox rules (checking a field checks its parents; unchecking removes its
//! children and any parent left empty). Ported from the iced app.
//!
//! Paths start at the root type name: `Query.users.id`, and an argument is
//! `Query.users.arg:first`, the form `generate_selection_set` expects.

use crabipie_core::GraphqlType;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct TreeRow {
    pub depth: u16,
    pub path: String,
    pub name: String,
    pub ty: String,
    pub expandable: bool,
    pub expanded: bool,
    pub selected: bool,
    pub is_arg: bool,
}

/// Visible rows below `cur` (only expanded nodes are descended into).
pub fn flatten(
    types: &[GraphqlType],
    cur: &GraphqlType,
    expanded: &HashSet<String>,
    selected: &HashSet<String>,
    search: &str,
    depth: u16,
    path: &str,
    out: &mut Vec<TreeRow>,
) {
    let Some(fields) = cur.fields.as_ref().or(cur.input_fields.as_ref()) else { return };
    let q = search.to_lowercase();
    for field in fields {
        if !q.is_empty() && !field.name.to_lowercase().contains(&q) {
            continue;
        }
        let base = field.field_type.get_base_name().map(|s| s.as_str()).unwrap_or("Unknown");
        let fpath = format!("{path}.{}", field.name);
        let target = types.iter().find(|t| t.name.as_deref() == Some(base));
        let has_children = target.map_or(false, |t| {
            t.fields.as_ref().map_or(false, |f| !f.is_empty()) || t.input_fields.as_ref().map_or(false, |f| !f.is_empty())
        });
        let expandable = has_children || !field.args.is_empty();
        let is_expanded = expanded.contains(&fpath);
        out.push(TreeRow {
            depth,
            path: fpath.clone(),
            name: field.name.clone(),
            ty: field.field_type.to_string(),
            expandable,
            expanded: is_expanded,
            selected: selected.contains(&fpath),
            is_arg: false,
        });
        if !is_expanded {
            continue;
        }
        for arg in &field.args {
            let apath = format!("{fpath}.arg:{}", arg.name);
            out.push(TreeRow {
                depth: depth + 1,
                path: apath.clone(),
                name: format!("arg: {}", arg.name),
                ty: format!("({})", arg.arg_type),
                expandable: false, // input-object arguments can't be built from checkboxes
                expanded: false,
                selected: selected.contains(&apath),
                is_arg: true,
            });
        }
        if let Some(t) = target {
            flatten(types, t, expanded, selected, search, depth + 1, &fpath, out);
        }
    }
}

/// Checkbox click on `path`.
pub fn toggle(selected: &mut HashSet<String>, manual: &mut HashSet<String>, path: &str) {
    if selected.contains(path) {
        selected.remove(path);
        manual.remove(path);
        let child_prefix = format!("{path}.");
        selected.retain(|p| !p.starts_with(&child_prefix));
        manual.retain(|p| !p.starts_with(&child_prefix));
        let mut parts: Vec<&str> = path.split('.').collect();
        while parts.len() > 1 {
            parts.pop();
            let parent = parts.join(".");
            let sibling_prefix = format!("{parent}.");
            let has_active_children = selected.iter().any(|p| p.starts_with(&sibling_prefix));
            if !has_active_children && !manual.contains(&parent) {
                selected.remove(&parent);
            } else {
                break;
            }
        }
    } else {
        manual.insert(path.to_string());
        let mut cur = String::new();
        for (i, part) in path.split('.').enumerate() {
            if i > 0 {
                cur.push('.');
            }
            cur.push_str(part);
            selected.insert(cur.clone());
        }
    }
}

#[cfg(test)]
pub const TEST_SCHEMA: &str = r#"{"data":{"__schema":{"queryType":{"name":"Query"},"types":[
 {"name":"Query","kind":"OBJECT","fields":[{"name":"users","type":{"name":null,"kind":"LIST","ofType":{"name":"User","kind":"OBJECT","ofType":null}},"args":[{"name":"first","type":{"name":"Int","kind":"SCALAR","ofType":null}}]}],"inputFields":null},
 {"name":"User","kind":"OBJECT","fields":[{"name":"id","type":{"name":null,"kind":"NON_NULL","ofType":{"name":"Int","kind":"SCALAR","ofType":null}},"args":[]},{"name":"name","type":{"name":"String","kind":"SCALAR","ofType":null},"args":[]}],"inputFields":null},
 {"name":"Int","kind":"SCALAR","fields":null,"inputFields":null},
 {"name":"String","kind":"SCALAR","fields":null,"inputFields":null}]}}}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_shows_args_then_children_and_filters_by_search() {
        let schema = crabipie_core::parse_graphql_schema(TEST_SCHEMA).unwrap();
        let root = schema.types.iter().find(|t| t.name.as_deref() == Some("Query")).unwrap();
        let mut expanded: HashSet<String> = HashSet::new();
        let none = HashSet::new();
        let mut rows = Vec::new();
        flatten(&schema.types, root, &expanded, &none, "", 1, "Query", &mut rows);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].expandable && !rows[0].expanded);

        expanded.insert("Query.users".into());
        rows.clear();
        flatten(&schema.types, root, &expanded, &none, "", 1, "Query", &mut rows);
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["users", "arg: first", "id", "name"]);
        assert_eq!(rows[2].ty, "Int!");

        rows.clear();
        flatten(&schema.types, root, &expanded, &none, "NAM", 1, "Query", &mut rows);
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["users", "arg: first", "name"], "search is case-insensitive and applies to children");
    }

    #[test]
    fn checking_selects_parents_and_unchecking_prunes_them() {
        let (mut sel, mut manual) = (HashSet::new(), HashSet::new());
        toggle(&mut sel, &mut manual, "Query.users.id");
        assert!(sel.contains("Query.users") && sel.contains("Query.users.id"));
        toggle(&mut sel, &mut manual, "Query.users.name");
        toggle(&mut sel, &mut manual, "Query.users.id");
        assert!(sel.contains("Query.users"), "name is still selected");
        toggle(&mut sel, &mut manual, "Query.users.name");
        assert!(!sel.contains("Query.users"), "empty parent is removed");
    }
}
