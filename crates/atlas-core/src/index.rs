//! Persistent SQLite index: file snapshots per root (instant revisit),
//! tags, destination assignments, and the action journal.
//!
//! All writes happen on a dedicated DB thread so the UI never blocks on I/O.

use crate::types::FileEntry;
use crossbeam_channel::{unbounded, Receiver, Sender};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct TagState {
    /// rel path -> tags
    pub tags: HashMap<String, Vec<String>>,
    /// rel path -> (dest folder relative to export root, optional new name)
    pub assigns: HashMap<String, (String, Option<String>)>,
}

pub enum DbCmd {
    SaveSnapshot {
        root: PathBuf,
        entries: Vec<(String, u64, i64, i64, String)>, // rel, size, mtime, ctime, owner
    },
    UpsertFile {
        root: PathBuf,
        rel: String,
        size: u64,
        mtime: i64,
        ctime: i64,
        owner: String,
    },
    RemoveFile {
        root: PathBuf,
        rel: String,
    },
    SetTags {
        root: PathBuf,
        rel: String,
        tags: Vec<String>,
    },
    SetAssign {
        root: PathBuf,
        rel: String,
        assign: Option<(String, Option<String>)>,
    },
    SaveJournal {
        root: PathBuf,
        json: String,
    },
    LoadRoot {
        root: PathBuf,
        reply: Sender<LoadedRoot>,
    },
}

pub struct LoadedRoot {
    pub snapshot: Option<Vec<FileEntry>>,
    pub last_scan: i64,
    pub tag_state: TagState,
    pub journal_json: Option<String>,
}

#[derive(Clone)]
pub struct Db {
    tx: Sender<DbCmd>,
}

impl Db {
    pub fn open() -> Db {
        Db::open_at(data_dir().join("atlas.db"))
    }

    pub fn open_at(path: PathBuf) -> Db {
        let (tx, rx) = unbounded::<DbCmd>();
        std::thread::spawn(move || db_thread(path, rx));
        Db { tx }
    }

    pub fn send(&self, cmd: DbCmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn load_root(&self, root: PathBuf) -> Receiver<LoadedRoot> {
        let (reply, rx) = unbounded();
        self.send(DbCmd::LoadRoot { root, reply });
        rx
    }
}

pub fn data_dir() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("NativeFileAtlas");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn db_thread(path: PathBuf, rx: Receiver<DbCmd>) {
    let conn = match Connection::open(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to open index db: {e}");
            return;
        }
    };
    let _ = conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         CREATE TABLE IF NOT EXISTS roots(
            id INTEGER PRIMARY KEY,
            path TEXT UNIQUE NOT NULL,
            last_scan INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS files(
            root_id INTEGER NOT NULL,
            rel TEXT NOT NULL,
            size INTEGER NOT NULL,
            mtime INTEGER NOT NULL,
            PRIMARY KEY(root_id, rel)
         ) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS tags(
            root_id INTEGER NOT NULL,
            rel TEXT NOT NULL,
            tag TEXT NOT NULL,
            PRIMARY KEY(root_id, rel, tag)
         ) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS assigns(
            root_id INTEGER NOT NULL,
            rel TEXT NOT NULL,
            dest_rel TEXT NOT NULL,
            new_name TEXT,
            PRIMARY KEY(root_id, rel)
         ) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS journal(
            root_id INTEGER PRIMARY KEY,
            json TEXT NOT NULL
         );",
    );
    let _ = conn.execute(
        "ALTER TABLE files ADD COLUMN ctime INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE files ADD COLUMN owner TEXT NOT NULL DEFAULT ''",
        [],
    );

    while let Ok(cmd) = rx.recv() {
        if let Err(e) = handle(&conn, cmd) {
            eprintln!("index db error: {e}");
        }
    }
}

fn root_id(conn: &Connection, root: &Path) -> rusqlite::Result<i64> {
    let p = root.to_string_lossy();
    conn.execute(
        "INSERT INTO roots(path) VALUES(?1) ON CONFLICT(path) DO NOTHING",
        [&p],
    )?;
    conn.query_row("SELECT id FROM roots WHERE path=?1", [&p], |r| r.get(0))
}

fn handle(conn: &Connection, cmd: DbCmd) -> rusqlite::Result<()> {
    match cmd {
        DbCmd::SaveSnapshot { root, entries } => {
            let rid = root_id(conn, &root)?;
            conn.execute_batch("BEGIN")?;
            let result = (|| -> rusqlite::Result<()> {
                conn.execute("DELETE FROM files WHERE root_id=?1", [rid])?;
                let mut stmt = conn.prepare_cached(
                    "INSERT OR REPLACE INTO files(root_id, rel, size, mtime, ctime, owner) VALUES(?1,?2,?3,?4,?5,?6)",
                )?;
                for (rel, size, mtime, ctime, owner) in &entries {
                    stmt.execute(rusqlite::params![
                        rid,
                        rel,
                        *size as i64,
                        mtime,
                        ctime,
                        owner
                    ])?;
                }
                conn.execute(
                    "UPDATE roots SET last_scan=?2 WHERE id=?1",
                    rusqlite::params![rid, crate::scanner::now_unix()],
                )?;
                Ok(())
            })();
            if result.is_ok() {
                conn.execute_batch("COMMIT")?;
            } else {
                let _ = conn.execute_batch("ROLLBACK");
                result?;
            }
        }
        DbCmd::UpsertFile {
            root,
            rel,
            size,
            mtime,
            ctime,
            owner,
        } => {
            let rid = root_id(conn, &root)?;
            conn.execute(
                "INSERT INTO files(root_id, rel, size, mtime, ctime, owner) VALUES(?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(root_id, rel) DO UPDATE SET size=?3, mtime=?4, ctime=?5, owner=?6",
                rusqlite::params![rid, rel, size as i64, mtime, ctime, owner],
            )?;
        }
        DbCmd::RemoveFile { root, rel } => {
            let rid = root_id(conn, &root)?;
            conn.execute(
                "DELETE FROM files WHERE root_id=?1 AND rel=?2",
                rusqlite::params![rid, rel],
            )?;
        }
        DbCmd::SetTags { root, rel, tags } => {
            let rid = root_id(conn, &root)?;
            conn.execute(
                "DELETE FROM tags WHERE root_id=?1 AND rel=?2",
                rusqlite::params![rid, rel],
            )?;
            let mut stmt =
                conn.prepare_cached("INSERT INTO tags(root_id, rel, tag) VALUES(?1,?2,?3)")?;
            for t in tags {
                stmt.execute(rusqlite::params![rid, rel, t])?;
            }
        }
        DbCmd::SetAssign { root, rel, assign } => {
            let rid = root_id(conn, &root)?;
            match assign {
                Some((dest, new_name)) => {
                    conn.execute(
                        "INSERT INTO assigns(root_id, rel, dest_rel, new_name) VALUES(?1,?2,?3,?4)
                         ON CONFLICT(root_id, rel) DO UPDATE SET dest_rel=?3, new_name=?4",
                        rusqlite::params![rid, rel, dest, new_name],
                    )?;
                }
                None => {
                    conn.execute(
                        "DELETE FROM assigns WHERE root_id=?1 AND rel=?2",
                        rusqlite::params![rid, rel],
                    )?;
                }
            }
        }
        DbCmd::SaveJournal { root, json } => {
            let rid = root_id(conn, &root)?;
            conn.execute(
                "INSERT INTO journal(root_id, json) VALUES(?1,?2)
                 ON CONFLICT(root_id) DO UPDATE SET json=?2",
                rusqlite::params![rid, json],
            )?;
        }
        DbCmd::LoadRoot { root, reply } => {
            let p = root.to_string_lossy().into_owned();
            let rid: Option<i64> = conn
                .query_row("SELECT id FROM roots WHERE path=?1", [&p], |r| r.get(0))
                .ok();

            let mut loaded = LoadedRoot {
                snapshot: None,
                last_scan: 0,
                tag_state: TagState {
                    tags: HashMap::new(),
                    assigns: HashMap::new(),
                },
                journal_json: None,
            };

            if let Some(rid) = rid {
                loaded.last_scan = conn
                    .query_row("SELECT last_scan FROM roots WHERE id=?1", [rid], |r| {
                        r.get(0)
                    })
                    .unwrap_or(0);

                let mut stmt = conn.prepare_cached(
                    "SELECT rel, size, mtime, ctime, owner FROM files WHERE root_id=?1",
                )?;
                let rows: Vec<FileEntry> = stmt
                    .query_map([rid], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, String>(4)?,
                        ))
                    })?
                    .flatten()
                    .map(|(rel, size, mtime, ctime, owner)| {
                        FileEntry::from_rel(&root, rel, size as u64, mtime, ctime, owner)
                    })
                    .collect();
                if !rows.is_empty() {
                    loaded.snapshot = Some(rows);
                }

                let mut stmt = conn.prepare_cached("SELECT rel, tag FROM tags WHERE root_id=?1")?;
                for row in stmt.query_map([rid], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })? {
                    if let Ok((rel, tag)) = row {
                        loaded.tag_state.tags.entry(rel).or_default().push(tag);
                    }
                }

                let mut stmt = conn.prepare_cached(
                    "SELECT rel, dest_rel, new_name FROM assigns WHERE root_id=?1",
                )?;
                for row in stmt.query_map([rid], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                })? {
                    if let Ok((rel, dest, nn)) = row {
                        loaded.tag_state.assigns.insert(rel, (dest, nn));
                    }
                }

                loaded.journal_json = conn
                    .query_row("SELECT json FROM journal WHERE root_id=?1", [rid], |r| {
                        r.get(0)
                    })
                    .ok();
            }

            let _ = reply.send(loaded);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_tags_assigns_journal_round_trip() {
        let base = std::env::temp_dir().join(format!("nfa_db_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let db = Db::open_at(base.join("test.db"));
        let root = PathBuf::from(r"C:\fake\root");

        db.send(DbCmd::SaveSnapshot {
            root: root.clone(),
            entries: vec![
                (
                    "a.jpg".into(),
                    100,
                    1_700_000_000,
                    1_700_000_000,
                    "jmoser".into(),
                ),
                (
                    r"sub\b.mp4".into(),
                    200,
                    1_700_000_100,
                    1_700_000_050,
                    "jmoser".into(),
                ),
            ],
        });
        db.send(DbCmd::SetTags {
            root: root.clone(),
            rel: "a.jpg".into(),
            tags: vec!["hero".into(), "red".into()],
        });
        db.send(DbCmd::SetAssign {
            root: root.clone(),
            rel: r"sub\b.mp4".into(),
            assign: Some(("Videos".into(), Some("clip.mp4".into()))),
        });
        db.send(DbCmd::SaveJournal {
            root: root.clone(),
            json: r#"{"entries":[],"cursor":0}"#.into(),
        });

        let loaded = db
            .load_root(root.clone())
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let snap = loaded.snapshot.expect("snapshot missing");
        assert_eq!(snap.len(), 2);
        let b = snap.iter().find(|e| e.rel == r"sub\b.mp4").unwrap();
        assert_eq!(b.size, 200);
        assert_eq!(b.name, "b.mp4");
        assert_eq!(
            loaded.tag_state.tags.get("a.jpg").unwrap(),
            &vec!["hero".to_string(), "red".to_string()]
        );
        assert_eq!(
            loaded.tag_state.assigns.get(r"sub\b.mp4").unwrap(),
            &("Videos".to_string(), Some("clip.mp4".to_string()))
        );
        assert!(loaded.journal_json.is_some());

        // Incremental update + removal.
        db.send(DbCmd::UpsertFile {
            root: root.clone(),
            rel: "c.png".into(),
            size: 5,
            mtime: 1,
            ctime: 1,
            owner: String::new(),
        });
        db.send(DbCmd::RemoveFile {
            root: root.clone(),
            rel: "a.jpg".into(),
        });
        let loaded = db
            .load_root(root)
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let rels: Vec<String> = loaded
            .snapshot
            .unwrap()
            .into_iter()
            .map(|e| e.rel)
            .collect();
        assert!(rels.contains(&"c.png".to_string()));
        assert!(!rels.contains(&"a.jpg".to_string()));

        let _ = std::fs::remove_dir_all(&base);
    }
}
