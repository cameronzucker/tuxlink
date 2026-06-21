use crate::search::types::{FilterKey, FilterValue, QuerySpec, ReadState, SortOrder};

/// Compose a `QuerySpec` into `(sql, params)`. The SQL joins `messages_fts`
/// to `messages_meta` only when free-text is present; otherwise scans
/// `messages_meta` directly.
///
/// LIMIT and OFFSET are embedded as integer literals (not bound params) because
/// they derive from `u32` page fields — no user-controlled string content — so
/// embedding them is safe and keeps `params` free of non-filter entries, which
/// simplifies caller code and test assertions.
pub fn compose(spec: &QuerySpec) -> (String, Vec<SqlParam>) {
    let mut params: Vec<SqlParam> = Vec::new();
    let mut where_clauses: Vec<String> = Vec::new();

    // FTS join when free-text is present
    let (from_clause, fts_where) = match &spec.free_text {
        Some(text) if !text.trim().is_empty() => {
            params.push(SqlParam::Text(text.clone()));
            (
                "messages_fts AS f JOIN messages_meta AS m ON m.mid = f.mid".to_string(),
                Some(format!("messages_fts MATCH ?{}", params.len())),
            )
        }
        _ => ("messages_meta AS m".to_string(), None),
    };
    if let Some(c) = fts_where {
        where_clauses.push(c);
    }

    // tuxlink-wl7n: track whether the query pins a SPECIFIC folder. A default /
    // cross-folder ("all", or no folder filter) search must NOT surface trashed
    // messages — Delete is a discard, so the Deleted folder is excluded unless
    // the operator explicitly browses it (FOLDER:deleted). See the append below.
    let mut has_specific_folder = false;

    for (key, value) in &spec.filters {
        match (key, value) {
            (FilterKey::Folder, FilterValue::Folder(f)) if f != "all" => {
                params.push(SqlParam::Text(f.clone()));
                where_clauses.push(format!("m.folder = ?{}", params.len()));
                has_specific_folder = true;
            }
            (FilterKey::From, FilterValue::Addr(a)) => {
                params.push(SqlParam::Text(format!("%{}%", a)));
                where_clauses.push(format!("m.from_addr LIKE ?{}", params.len()));
            }
            (FilterKey::To, FilterValue::Addr(a)) => {
                params.push(SqlParam::Text(format!("%{}%", a)));
                where_clauses.push(format!("m.to_addrs LIKE ?{}", params.len()));
            }
            (FilterKey::FormType, FilterValue::FormType(ft)) => {
                params.push(SqlParam::Text(ft.clone()));
                where_clauses.push(format!("m.form_type = ?{}", params.len()));
            }
            (FilterKey::HasForm, FilterValue::Bool(true)) => {
                where_clauses.push("m.form_type IS NOT NULL".into());
            }
            (FilterKey::HasForm, FilterValue::Bool(false)) => {
                where_clauses.push("m.form_type IS NULL".into());
            }
            (FilterKey::HasAttach, FilterValue::Bool(b)) => {
                params.push(SqlParam::Int(*b as i64));
                where_clauses.push(format!("m.has_attachments = ?{}", params.len()));
            }
            (FilterKey::ReadState, FilterValue::ReadState(rs)) => {
                let v = matches!(rs, ReadState::Unread) as i64;
                params.push(SqlParam::Int(v));
                where_clauses.push(format!("m.unread = ?{}", params.len()));
            }
            (FilterKey::Transport, FilterValue::Transport(t)) => {
                params.push(SqlParam::Text(t.clone()));
                where_clauses.push(format!("m.transport_used = ?{}", params.len()));
            }
            (FilterKey::DateRange, FilterValue::DateRange { from, to }) => {
                if let Some(f) = from {
                    params.push(SqlParam::Int(*f));
                    where_clauses.push(format!(
                        "COALESCE(m.date_received, m.date_sent) >= ?{}",
                        params.len()
                    ));
                }
                if let Some(t) = to {
                    params.push(SqlParam::Int(*t));
                    where_clauses.push(format!(
                        "COALESCE(m.date_received, m.date_sent) <= ?{}",
                        params.len()
                    ));
                }
            }
            _ => {} // unknown / mismatched (FilterKey, FilterValue) — defensively ignored
        }
    }

    // tuxlink-wl7n: exclude the Deleted (Trash) folder from any non-folder-pinned
    // search. `'deleted'` is a hardcoded constant (not user input), so embedding it
    // as a literal is safe and keeps `params` to genuine filter values.
    if !has_specific_folder {
        where_clauses.push("m.folder != 'deleted'".into());
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };

    let order = match spec.sort {
        SortOrder::DateDesc => "ORDER BY COALESCE(m.date_received, m.date_sent) DESC",
        SortOrder::DateAsc => "ORDER BY COALESCE(m.date_received, m.date_sent) ASC",
    };

    // LIMIT and OFFSET embedded as literals: both are u32 page fields (not
    // user-supplied strings), so embedding is safe and avoids adding non-filter
    // entries to `params` which would break the `params.is_empty()` assertion
    // in the no-filter test case.
    let sql = format!(
        "SELECT m.mid, m.folder, m.subject, m.from_addr, m.to_addrs, m.cc_addrs, \
                m.date_sent, m.date_received, m.unread, m.form_type, \
                m.has_attachments, m.attachment_count, m.transport_used, \
                m.direction, m.message_size, m.routing_path, m.identity_tag \
         FROM {from_clause}{where_sql} {order} LIMIT {} OFFSET {}",
        spec.page.page_size, spec.page.offset,
    );

    (sql, params)
}

#[derive(Debug, Clone, PartialEq)]
pub enum SqlParam {
    Text(String),
    Int(i64),
    Null,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn empty_spec() -> QuerySpec {
        QuerySpec::default()
    }

    #[test]
    fn compose_no_filters_no_freetext_lists_all_except_trash() {
        let spec = empty_spec();
        let (sql, params) = compose(&spec);
        assert!(sql.contains("FROM messages_meta"), "got: {sql}");
        assert!(!sql.contains("MATCH"), "got: {sql}");
        // tuxlink-wl7n: an unfiltered search excludes the Deleted folder — and the
        // exclusion is a literal, so it adds no bound param.
        assert!(sql.contains("m.folder != 'deleted'"), "got: {sql}");
        assert!(params.is_empty());
        assert!(sql.contains("ORDER BY"));
    }

    #[test]
    fn compose_freetext_joins_fts() {
        let spec = QuerySpec {
            free_text: Some("damage".into()),
            ..QuerySpec::default()
        };
        let (sql, params) = compose(&spec);
        assert!(sql.contains("messages_fts MATCH"), "got: {sql}");
        assert!(params
            .iter()
            .any(|p| matches!(p, SqlParam::Text(s) if s == "damage")));
    }

    #[test]
    fn compose_from_chip_adds_where_clause() {
        let mut filters = BTreeMap::new();
        filters.insert(FilterKey::From, FilterValue::Addr("KX5DD".into()));
        let spec = QuerySpec {
            filters,
            ..QuerySpec::default()
        };
        let (sql, params) = compose(&spec);
        assert!(sql.contains("from_addr LIKE"), "got: {sql}");
        assert!(params
            .iter()
            .any(|p| matches!(p, SqlParam::Text(s) if s == "%KX5DD%")));
    }

    #[test]
    fn compose_folder_all_pins_no_folder_but_excludes_trash() {
        let mut filters = BTreeMap::new();
        filters.insert(FilterKey::Folder, FilterValue::Folder("all".into()));
        let spec = QuerySpec {
            filters,
            ..QuerySpec::default()
        };
        let (sql, _params) = compose(&spec);
        // FOLDER:all does not pin a specific folder (no equality constraint)...
        assert!(
            !sql.contains("m.folder ="),
            "FOLDER:all should not pin a folder: {sql}"
        );
        // ...but tuxlink-wl7n still excludes the Deleted folder from the results.
        assert!(
            sql.contains("m.folder != 'deleted'"),
            "FOLDER:all should still exclude Trash: {sql}"
        );
    }

    // tuxlink-wl7n: explicitly browsing the Deleted folder (FOLDER:deleted) pins
    // it and must NOT also apply the default Trash-exclusion — otherwise an
    // in-Trash search would always return nothing.
    #[test]
    fn compose_folder_deleted_includes_trash() {
        let mut filters = BTreeMap::new();
        filters.insert(FilterKey::Folder, FilterValue::Folder("deleted".into()));
        let spec = QuerySpec {
            filters,
            ..QuerySpec::default()
        };
        let (sql, params) = compose(&spec);
        assert!(sql.contains("m.folder = ?"), "got: {sql}");
        assert!(
            !sql.contains("m.folder != 'deleted'"),
            "FOLDER:deleted must not self-exclude: {sql}"
        );
        assert!(params
            .iter()
            .any(|p| matches!(p, SqlParam::Text(s) if s == "deleted")));
    }

    #[test]
    fn compose_combined_chip_set_emits_all_clauses() {
        let mut filters = BTreeMap::new();
        filters.insert(FilterKey::From, FilterValue::Addr("KX5DD".into()));
        filters.insert(FilterKey::FormType, FilterValue::FormType("ICS-213".into()));
        filters.insert(FilterKey::ReadState, FilterValue::ReadState(ReadState::Unread));
        filters.insert(FilterKey::HasAttach, FilterValue::Bool(true));
        filters.insert(
            FilterKey::DateRange,
            FilterValue::DateRange {
                from: Some(1_700_000_000),
                to: Some(1_710_000_000),
            },
        );
        filters.insert(FilterKey::Transport, FilterValue::Transport("packet".into()));
        let spec = QuerySpec {
            free_text: Some("damage".into()),
            filters,
            sort: SortOrder::DateDesc,
            page: Default::default(),
        };
        let (sql, params) = compose(&spec);
        assert!(sql.contains("messages_fts MATCH"));
        assert!(sql.contains("from_addr LIKE"));
        assert!(sql.contains("form_type ="));
        assert!(sql.contains("unread ="));
        assert!(sql.contains("has_attachments ="));
        assert!(sql.contains("transport_used ="));
        assert!(sql.contains(">=")); // date range from
        assert!(sql.contains("<=")); // date range to
        assert!(params.len() >= 7);
    }
}
