//! `SQLite` database builtins for the Kōdo runtime.
//!
//! Provides FFI-callable functions for opening `SQLite` databases, executing
//! SQL statements, and querying results. Uses opaque `i64` handles following
//! the same pattern as JSON and HTTP builtins.

use crate::helpers::write_string_out;

/// An open `SQLite` database connection.
struct DbHandle {
    /// The underlying rusqlite connection.
    conn: rusqlite::Connection,
}

/// Materialized query results with a cursor for row-by-row iteration.
///
/// Rows are fully materialized to avoid lifetime issues with `Statement`.
/// Each row is a vector of `rusqlite::types::Value`.
struct QueryResult {
    /// All rows from the query, materialized eagerly.
    rows: Vec<Vec<rusqlite::types::Value>>,
    /// Current cursor position (next row to read).
    cursor: usize,
}

/// Opens or creates a `SQLite` database at the given path.
///
/// Returns a non-zero handle on success, or 0 on error.
/// Use `":memory:"` for an in-memory database.
/// The handle must be freed with `kodo_db_close` when no longer needed.
///
/// # Safety
///
/// `path_ptr` must point to `path_len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_db_open(path_ptr: *const u8, path_len: usize) -> i64 {
    if path_ptr.is_null() {
        return 0;
    }
    // SAFETY: caller guarantees path_ptr/path_len form a valid UTF-8 slice.
    let path_bytes = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
    let Ok(path_str) = std::str::from_utf8(path_bytes) else {
        return 0;
    };
    let conn = if path_str == ":memory:" {
        rusqlite::Connection::open_in_memory()
    } else {
        rusqlite::Connection::open(path_str)
    };
    match conn {
        Ok(c) => {
            let handle = Box::new(DbHandle { conn: c });
            // SAFETY: intentionally leaks so caller manages via opaque handle.
            // Freed by `kodo_db_close`.
            Box::into_raw(handle) as i64
        }
        Err(_) => 0,
    }
}

/// Executes a SQL statement that does not return rows (CREATE, INSERT, UPDATE, DELETE).
///
/// Returns 0 on success, 1 on error.
///
/// # Safety
///
/// `db` must be a valid handle returned by `kodo_db_open`.
/// `sql_ptr` must point to `sql_len` valid UTF-8 bytes.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_db_execute(db: i64, sql_ptr: *const u8, sql_len: usize) -> i64 {
    if db == 0 || sql_ptr.is_null() {
        return 1;
    }
    // SAFETY: caller guarantees db is a valid handle from kodo_db_open.
    let handle = unsafe { &*(db as *const DbHandle) };
    // SAFETY: caller guarantees sql_ptr/sql_len form a valid UTF-8 slice.
    let sql_bytes = unsafe { std::slice::from_raw_parts(sql_ptr, sql_len) };
    let Ok(sql_str) = std::str::from_utf8(sql_bytes) else {
        return 1;
    };
    match handle.conn.execute_batch(sql_str) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// Executes a SQL query and returns a handle to the materialized results.
///
/// Returns a non-zero result handle on success, or 0 on error.
/// The handle must be freed with `kodo_db_result_free` when no longer needed.
///
/// # Safety
///
/// `db` must be a valid handle returned by `kodo_db_open`.
/// `sql_ptr` must point to `sql_len` valid UTF-8 bytes.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_db_query(db: i64, sql_ptr: *const u8, sql_len: usize) -> i64 {
    if db == 0 || sql_ptr.is_null() {
        return 0;
    }
    // SAFETY: caller guarantees db is a valid handle from kodo_db_open.
    let handle = unsafe { &*(db as *const DbHandle) };
    // SAFETY: caller guarantees sql_ptr/sql_len form a valid UTF-8 slice.
    let sql_bytes = unsafe { std::slice::from_raw_parts(sql_ptr, sql_len) };
    let Ok(sql_str) = std::str::from_utf8(sql_bytes) else {
        return 0;
    };
    let stmt = handle.conn.prepare(sql_str);
    let Ok(mut stmt) = stmt else {
        return 0;
    };
    let col_count = stmt.column_count();
    let row_result = stmt.query_map([], |row| {
        let mut values = Vec::with_capacity(col_count);
        for i in 0..col_count {
            let val: rusqlite::types::Value = row.get(i)?;
            values.push(val);
        }
        Ok(values)
    });
    let Ok(mapped_rows) = row_result else {
        return 0;
    };
    let rows: Vec<Vec<rusqlite::types::Value>> = mapped_rows.flatten().collect();
    let result = Box::new(QueryResult { rows, cursor: 0 });
    // SAFETY: intentionally leaks so caller manages via opaque handle.
    // Freed by `kodo_db_result_free`.
    Box::into_raw(result) as i64
}

/// Advances the result cursor to the next row.
///
/// Returns 1 if a row is available, 0 if no more rows.
///
/// # Safety
///
/// `result` must be a valid handle returned by `kodo_db_query`.
#[no_mangle]
pub unsafe extern "C" fn kodo_db_row_next(result: i64) -> i64 {
    if result == 0 {
        return 0;
    }
    // SAFETY: caller guarantees result is a valid handle from kodo_db_query.
    let qr = unsafe { &*(result as *const QueryResult) };
    i64::from(qr.cursor < qr.rows.len())
}

/// Reads a column value as a string from the current row.
///
/// After reading, advances the cursor if this is the last column read
/// (caller should call `kodo_db_row_next` to check availability).
///
/// # Safety
///
/// `result` must be a valid handle returned by `kodo_db_query`.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_db_row_get_string(
    result: i64,
    col: i64,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    if result == 0 || out_ptr.is_null() || out_len.is_null() {
        return;
    }
    // SAFETY: caller guarantees result is a valid handle from kodo_db_query.
    let qr = unsafe { &*(result as *const QueryResult) };
    if qr.cursor >= qr.rows.len() {
        // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe { write_string_out(String::new(), out_ptr, out_len) };
        return;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let col_idx = col as usize;
    let row = &qr.rows[qr.cursor];
    let s = if col_idx < row.len() {
        match &row[col_idx] {
            rusqlite::types::Value::Text(t) => t.clone(),
            rusqlite::types::Value::Integer(i) => format!("{i}"),
            rusqlite::types::Value::Real(f) => format!("{f}"),
            rusqlite::types::Value::Null => String::new(),
            rusqlite::types::Value::Blob(b) => String::from_utf8_lossy(b).into_owned(),
        }
    } else {
        String::new()
    };
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe { write_string_out(s, out_ptr, out_len) };
}

/// Reads a column value as an integer from the current row.
///
/// Returns the integer value, or 0 if the column is not an integer or out of range.
///
/// # Safety
///
/// `result` must be a valid handle returned by `kodo_db_query`.
#[no_mangle]
pub unsafe extern "C" fn kodo_db_row_get_int(result: i64, col: i64) -> i64 {
    if result == 0 {
        return 0;
    }
    // SAFETY: caller guarantees result is a valid handle from kodo_db_query.
    let qr = unsafe { &*(result as *const QueryResult) };
    if qr.cursor >= qr.rows.len() {
        return 0;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let col_idx = col as usize;
    let row = &qr.rows[qr.cursor];
    if col_idx < row.len() {
        match &row[col_idx] {
            rusqlite::types::Value::Integer(i) => *i,
            rusqlite::types::Value::Real(f) => {
                #[allow(clippy::cast_possible_truncation)]
                let v = *f as i64;
                v
            }
            _ => 0,
        }
    } else {
        0
    }
}

/// Advances the result cursor past the current row.
///
/// Call this after reading all desired columns from the current row
/// to move to the next one. Returns 1 if there are more rows, 0 if done.
///
/// # Safety
///
/// `result` must be a valid handle returned by `kodo_db_query`.
#[no_mangle]
pub unsafe extern "C" fn kodo_db_row_advance(result: i64) -> i64 {
    if result == 0 {
        return 0;
    }
    // SAFETY: caller guarantees result is a valid handle from kodo_db_query.
    let qr = unsafe { &mut *(result as *mut QueryResult) };
    if qr.cursor < qr.rows.len() {
        qr.cursor += 1;
    }
    i64::from(qr.cursor < qr.rows.len())
}

/// Frees a query result handle previously returned by `kodo_db_query`.
///
/// Does nothing if `result` is 0 (null handle).
///
/// # Safety
///
/// `result` must be a valid handle returned by `kodo_db_query`, or 0.
/// After calling this function, the handle must not be used again.
#[no_mangle]
pub unsafe extern "C" fn kodo_db_result_free(result: i64) {
    if result == 0 {
        return;
    }
    // SAFETY: caller guarantees result was returned by kodo_db_query
    // (i.e. Box::into_raw on a Box<QueryResult>).
    let _ = unsafe { Box::from_raw(result as *mut QueryResult) };
}

/// Closes a database connection previously opened by `kodo_db_open`.
///
/// Does nothing if `db` is 0 (null handle).
///
/// # Safety
///
/// `db` must be a valid handle returned by `kodo_db_open`, or 0.
/// After calling this function, the handle must not be used again.
#[no_mangle]
pub unsafe extern "C" fn kodo_db_close(db: i64) {
    if db == 0 {
        return;
    }
    // SAFETY: caller guarantees db was returned by kodo_db_open
    // (i.e. Box::into_raw on a Box<DbHandle>).
    let _ = unsafe { Box::from_raw(db as *mut DbHandle) };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to open an in-memory database for testing.
    fn open_memory_db() -> i64 {
        let path = ":memory:";
        unsafe { kodo_db_open(path.as_ptr(), path.len()) }
    }

    /// Helper to execute SQL on a database handle.
    fn exec(db: i64, sql: &str) -> i64 {
        unsafe { kodo_db_execute(db, sql.as_ptr(), sql.len()) }
    }

    #[test]
    fn db_open_creates_memory_db() {
        let db = open_memory_db();
        assert_ne!(db, 0);
        unsafe { kodo_db_close(db) };
    }

    #[test]
    fn db_execute_create_table() {
        let db = open_memory_db();
        let result = exec(db, "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)");
        assert_eq!(result, 0);
        unsafe { kodo_db_close(db) };
    }

    #[test]
    fn db_execute_invalid_sql() {
        let db = open_memory_db();
        let result = exec(db, "NOT VALID SQL AT ALL !!!");
        assert_eq!(result, 1);
        unsafe { kodo_db_close(db) };
    }

    #[test]
    fn db_insert_and_query() {
        let db = open_memory_db();
        exec(db, "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)");
        exec(db, "INSERT INTO users (name) VALUES ('Alice')");
        exec(db, "INSERT INTO users (name) VALUES ('Bob')");

        let sql = "SELECT id, name FROM users ORDER BY id";
        let result = unsafe { kodo_db_query(db, sql.as_ptr(), sql.len()) };
        assert_ne!(result, 0);

        // First row
        let has_row = unsafe { kodo_db_row_next(result) };
        assert_eq!(has_row, 1);
        let id = unsafe { kodo_db_row_get_int(result, 0) };
        assert_eq!(id, 1);
        unsafe { kodo_db_row_advance(result) };

        // Second row
        let has_row = unsafe { kodo_db_row_next(result) };
        assert_eq!(has_row, 1);
        let id = unsafe { kodo_db_row_get_int(result, 0) };
        assert_eq!(id, 2);
        unsafe { kodo_db_row_advance(result) };

        // No more rows
        let has_row = unsafe { kodo_db_row_next(result) };
        assert_eq!(has_row, 0);

        unsafe { kodo_db_result_free(result) };
        unsafe { kodo_db_close(db) };
    }

    #[test]
    fn db_row_get_string() {
        let db = open_memory_db();
        exec(db, "CREATE TABLE t (name TEXT)");
        exec(db, "INSERT INTO t (name) VALUES ('hello')");

        let sql = "SELECT name FROM t";
        let result = unsafe { kodo_db_query(db, sql.as_ptr(), sql.len()) };
        assert_ne!(result, 0);

        let has_row = unsafe { kodo_db_row_next(result) };
        assert_eq!(has_row, 1);

        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_db_row_get_string(result, 0, &mut out_ptr, &mut out_len) };
        let s = unsafe { String::from_raw_parts(out_ptr as *mut u8, out_len, out_len) };
        assert_eq!(s, "hello");

        unsafe { kodo_db_result_free(result) };
        unsafe { kodo_db_close(db) };
    }

    #[test]
    fn db_row_get_int() {
        let db = open_memory_db();
        exec(db, "CREATE TABLE t (val INTEGER)");
        exec(db, "INSERT INTO t (val) VALUES (42)");

        let sql = "SELECT val FROM t";
        let result = unsafe { kodo_db_query(db, sql.as_ptr(), sql.len()) };
        assert_ne!(result, 0);

        let has_row = unsafe { kodo_db_row_next(result) };
        assert_eq!(has_row, 1);
        let val = unsafe { kodo_db_row_get_int(result, 0) };
        assert_eq!(val, 42);

        unsafe { kodo_db_result_free(result) };
        unsafe { kodo_db_close(db) };
    }

    #[test]
    fn db_row_next_exhaustion() {
        let db = open_memory_db();
        exec(db, "CREATE TABLE t (x INTEGER)");
        exec(db, "INSERT INTO t VALUES (1)");

        let sql = "SELECT x FROM t";
        let result = unsafe { kodo_db_query(db, sql.as_ptr(), sql.len()) };

        // Advance past the only row
        assert_eq!(unsafe { kodo_db_row_next(result) }, 1);
        unsafe { kodo_db_row_advance(result) };
        assert_eq!(unsafe { kodo_db_row_next(result) }, 0);

        unsafe { kodo_db_result_free(result) };
        unsafe { kodo_db_close(db) };
    }

    #[test]
    fn db_query_empty_result() {
        let db = open_memory_db();
        exec(db, "CREATE TABLE t (x INTEGER)");

        let sql = "SELECT x FROM t";
        let result = unsafe { kodo_db_query(db, sql.as_ptr(), sql.len()) };
        assert_ne!(result, 0);
        assert_eq!(unsafe { kodo_db_row_next(result) }, 0);

        unsafe { kodo_db_result_free(result) };
        unsafe { kodo_db_close(db) };
    }

    #[test]
    fn db_close_and_free_no_crash() {
        let db = open_memory_db();
        exec(db, "CREATE TABLE t (x INTEGER)");
        let sql = "SELECT x FROM t";
        let result = unsafe { kodo_db_query(db, sql.as_ptr(), sql.len()) };
        unsafe { kodo_db_result_free(result) };
        unsafe { kodo_db_close(db) };
        // Also test null handles.
        unsafe { kodo_db_close(0) };
        unsafe { kodo_db_result_free(0) };
    }

    #[test]
    fn db_open_invalid_path() {
        let path = "/nonexistent/deeply/nested/impossible/path/db.sqlite";
        let db = unsafe { kodo_db_open(path.as_ptr(), path.len()) };
        assert_eq!(db, 0);
    }
}
