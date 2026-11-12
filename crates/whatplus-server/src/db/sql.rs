//! `sqlite` crate 的薄包裝。
//!
//! 不用 rusqlite：ccusage（Antigravity adapter）已經用 `sqlite` crate 連結內建的 SQLite，
//! 同一個 workspace 只能有一個連結 `sqlite3` 的套件。

use anyhow::{Context, Result};
use sqlite::{State, Statement, Value};

pub struct Conn(sqlite::Connection);

/// 查詢結果的一列。
pub struct Row<'a, 'b>(&'a Statement<'b>);

impl Row<'_, '_> {
    pub fn get<T: sqlite::ReadableWithIndex>(&self, index: usize) -> Result<T> {
        Ok(self.0.read::<T, usize>(index)?)
    }

    pub fn get_bool(&self, index: usize) -> Result<bool> {
        Ok(self.get::<i64>(index)? != 0)
    }
}

/// 轉成 SQL 參數。
pub trait IntoValue {
    fn into_value(self) -> Value;
}

macro_rules! into_value {
    ($($t:ty => |$v:ident| $e:expr),* $(,)?) => {
        $(impl IntoValue for $t {
            fn into_value(self) -> Value {
                let $v = self;
                $e
            }
        })*
    };
}

into_value! {
    i64 => |v| Value::Integer(v),
    f64 => |v| Value::Float(v),
    bool => |v| Value::Integer(v as i64),
    &str => |v| Value::String(v.to_string()),
    String => |v| Value::String(v),
    &String => |v| Value::String(v.clone()),
    Option<&str> => |v| v.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
}

#[macro_export]
macro_rules! params {
    ($($p:expr),* $(,)?) => {
        &[$($crate::db::sql::IntoValue::into_value($p)),*][..]
    };
}

impl Conn {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        Ok(Self(sqlite::Connection::open(path)?))
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Ok(Self(sqlite::Connection::open(":memory:")?))
    }

    pub fn set_busy_timeout(&mut self, ms: usize) -> Result<()> {
        Ok(self.0.set_busy_timeout(ms)?)
    }

    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        Ok(self.0.execute(sql)?)
    }

    fn prepare(&self, sql: &str, params: &[Value]) -> Result<Statement<'_>> {
        let mut stmt = self
            .0
            .prepare(sql)
            .with_context(|| format!("prepare: {}", first_line(sql)))?;
        for (i, p) in params.iter().enumerate() {
            stmt.bind((i + 1, p))?;
        }
        Ok(stmt)
    }

    /// 執行一個寫入語句，回傳影響的列數。
    pub fn execute(&self, sql: &str, params: &[Value]) -> Result<usize> {
        let mut stmt = self.prepare(sql, params)?;
        while stmt.next()? == State::Row {}
        Ok(self.0.change_count())
    }

    pub fn query_map<T>(
        &self,
        sql: &str,
        params: &[Value],
        mut f: impl FnMut(&Row<'_, '_>) -> Result<T>,
    ) -> Result<Vec<T>> {
        let mut stmt = self.prepare(sql, params)?;
        let mut out = Vec::new();
        while stmt.next()? == State::Row {
            out.push(f(&Row(&stmt))?);
        }
        Ok(out)
    }

    /// 只取第一列；沒有資料時回傳 `None`。
    pub fn query_row<T>(
        &self,
        sql: &str,
        params: &[Value],
        f: impl FnOnce(&Row<'_, '_>) -> Result<T>,
    ) -> Result<Option<T>> {
        let mut stmt = self.prepare(sql, params)?;
        if stmt.next()? == State::Row {
            Ok(Some(f(&Row(&stmt))?))
        } else {
            Ok(None)
        }
    }

    pub fn last_insert_rowid(&self) -> Result<i64> {
        Ok(self
            .query_row("SELECT last_insert_rowid()", &[], |r| r.get(0))?
            .unwrap_or(0))
    }

    /// 在 transaction 內執行；`f` 失敗時 rollback。
    pub fn transaction<T>(&self, immediate: bool, f: impl FnOnce(&Conn) -> Result<T>) -> Result<T> {
        self.execute_batch(if immediate {
            "BEGIN IMMEDIATE"
        } else {
            "BEGIN"
        })?;
        match f(self) {
            Ok(v) => {
                self.execute_batch("COMMIT")?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
}

fn first_line(sql: &str) -> &str {
    sql.trim().lines().next().unwrap_or("")
}
