//! Dialect-aware SQL text utilities: statement splitting, identifier quoting and
//! generated browse queries. Pure functions only — no I/O.

mod split;

pub use split::{Span, split_statements, statement_at, utf16_to_byte_offset};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Postgres,
    Sqlite,
}

impl From<snaptable_core::model::Engine> for Dialect {
    fn from(e: snaptable_core::model::Engine) -> Self {
        match e {
            snaptable_core::model::Engine::Postgres => Dialect::Postgres,
            snaptable_core::model::Engine::Sqlite => Dialect::Sqlite,
        }
    }
}

/// Quote an identifier for the dialect, doubling embedded quote characters.
pub fn quote_ident(_dialect: Dialect, ident: &str) -> String {
    // Both PostgreSQL and SQLite use ANSI double quotes.
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// `schema.table`, quoted. SQLite's default schema `main` is kept explicit, which is valid.
pub fn qualified(dialect: Dialect, schema: &str, table: &str) -> String {
    format!(
        "{}.{}",
        quote_ident(dialect, schema),
        quote_ident(dialect, table)
    )
}

pub struct BrowseQuery<'a> {
    pub schema: &'a str,
    pub table: &'a str,
    /// Explicit sort chosen by the user.
    pub order_by: Option<(&'a str, bool)>,
    /// Fallback stable ordering (primary key columns) when no explicit sort is set.
    pub stable_order: &'a [String],
    pub filter: Option<&'a str>,
    pub limit: u32,
    pub offset: u32,
}

/// Build the page query used by the table browser.
pub fn browse_sql(dialect: Dialect, q: &BrowseQuery<'_>) -> String {
    let mut sql = format!("SELECT * FROM {}", qualified(dialect, q.schema, q.table));
    if let Some(filter) = q.filter.map(str::trim).filter(|f| !f.is_empty()) {
        sql.push_str(&format!(" WHERE ({filter})"));
    }
    let mut order: Vec<String> = Vec::new();
    if let Some((col, desc)) = q.order_by {
        order.push(format!(
            "{} {}",
            quote_ident(dialect, col),
            if desc { "DESC" } else { "ASC" }
        ));
    }
    for pk in q.stable_order {
        if q.order_by.map(|(c, _)| c) != Some(pk.as_str()) {
            order.push(quote_ident(dialect, pk));
        }
    }
    if !order.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&order.join(", "));
    }
    sql.push_str(&format!(" LIMIT {} OFFSET {}", q.limit, q.offset));
    sql
}

/// `SELECT count(*)` for the same table + filter as [`browse_sql`].
pub fn count_sql(dialect: Dialect, schema: &str, table: &str, filter: Option<&str>) -> String {
    let mut sql = format!("SELECT count(*) FROM {}", qualified(dialect, schema, table));
    if let Some(filter) = filter.map(str::trim).filter(|f| !f.is_empty()) {
        sql.push_str(&format!(" WHERE ({filter})"));
    }
    sql
}

/// Heuristic: does this statement produce a result set? Used to decide whether to
/// describe an empty result for its column headers.
pub fn looks_like_query(stmt: &str) -> bool {
    let first = stmt
        .trim_start_matches(|c: char| c.is_whitespace() || c == '(')
        .split(|c: char| !c.is_ascii_alphabetic())
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        first.as_str(),
        "select" | "with" | "values" | "table" | "show" | "explain" | "pragma"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_identifiers() {
        assert_eq!(quote_ident(Dialect::Postgres, "users"), "\"users\"");
        assert_eq!(quote_ident(Dialect::Postgres, "we\"ird"), "\"we\"\"ird\"");
    }

    #[test]
    fn browse_with_pk_fallback() {
        let pk = vec!["id".to_string()];
        let sql = browse_sql(
            Dialect::Postgres,
            &BrowseQuery {
                schema: "public",
                table: "users",
                order_by: Some(("name", true)),
                stable_order: &pk,
                filter: Some(" age > 3 "),
                limit: 100,
                offset: 200,
            },
        );
        assert_eq!(
            sql,
            "SELECT * FROM \"public\".\"users\" WHERE (age > 3) ORDER BY \"name\" DESC, \"id\" LIMIT 100 OFFSET 200"
        );
    }

    #[test]
    fn browse_does_not_duplicate_pk_order() {
        let pk = vec!["id".to_string()];
        let sql = browse_sql(
            Dialect::Sqlite,
            &BrowseQuery {
                schema: "main",
                table: "t",
                order_by: Some(("id", false)),
                stable_order: &pk,
                filter: Some("  "),
                limit: 10,
                offset: 0,
            },
        );
        assert_eq!(
            sql,
            "SELECT * FROM \"main\".\"t\" ORDER BY \"id\" ASC LIMIT 10 OFFSET 0"
        );
    }

    #[test]
    fn query_detection() {
        assert!(looks_like_query("  select 1"));
        assert!(looks_like_query("(SELECT 1) UNION (SELECT 2)"));
        assert!(looks_like_query("WITH x AS (select 1) select * from x"));
        assert!(!looks_like_query("update t set a = 1"));
        assert!(!looks_like_query("create table t (id int)"));
    }
}
