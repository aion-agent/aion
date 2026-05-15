use rusqlite::{Connection, params};
use std::path::Path;

const MIGRATION_001: &str = include_str!("migrations/001_init.sql");
const QUERY_WRITE: &str = include_str!("queries/write.sql");
const QUERY_READ: &str = include_str!("queries/read.sql");
const QUERY_SEARCH: &str = include_str!("queries/search.sql");
const QUERY_REGISTER_CHILD: &str = include_str!("queries/register_child.sql");
const QUERY_LIST_CHILDREN: &str = include_str!("queries/list_children.sql");

pub struct Memory {
    conn: Connection,
    path: std::path::PathBuf,
}

pub struct ChildDb {
    pub name: String,
    pub path: String,
    pub reason: Option<String>,
    pub created_at: String,
}

impl Memory {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(MIGRATION_001)?;
        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    pub fn write(&self, key: &str, value: &str, scope: &str) -> anyhow::Result<()> {
        self.conn.execute(QUERY_WRITE, params![key, value, scope])?;
        Ok(())
    }

    pub fn read(&self, key: &str) -> anyhow::Result<Option<String>> {
        let mut stmt = self.conn.prepare(QUERY_READ)?;
        let mut rows = stmt.query(params![key])?;
        Ok(rows.next()?.map(|r| r.get(0)).transpose()?)
    }

    pub fn search(&self, query: &str) -> anyhow::Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(QUERY_SEARCH)?;
        let rows = stmt.query_map(params![query], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn create_child(&self, name: &str, reason: &str) -> anyhow::Result<Memory> {
        let child_path = self.path
            .parent()
            .unwrap()
            .join(format!("{}.db", name));

        self.conn.execute(
            QUERY_REGISTER_CHILD,
            params![name, child_path.to_str().unwrap(), reason],
        )?;

        Memory::open(&child_path)
    }

    pub fn list_children(&self) -> anyhow::Result<Vec<ChildDb>> {
        let mut stmt = self.conn.prepare(QUERY_LIST_CHILDREN)?;
        let rows = stmt.query_map([], |r| {
            Ok(ChildDb {
                name: r.get(0)?,
                path: r.get(1)?,
                reason: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }
}
