use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::platform::paths::StoredPaths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRewrite {
    pub from: PathBuf,
    pub to: PathBuf,
}

impl PathRewrite {
    pub fn new(from: PathBuf, to: PathBuf) -> Self {
        Self { from, to }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Native,
    Escaped,
    Slashed,
}

impl Form {
    const ALL: [Self; 3] = [Self::Native, Self::Escaped, Self::Slashed];

    fn render(self, path: &Path) -> String {
        let native = path.to_string_lossy().replace('/', "\\");
        match self {
            Self::Native => native,
            Self::Escaped => native.replace('\\', "\\\\"),
            Self::Slashed => native.replace('\\', "/"),
        }
    }

    const fn separator(self) -> &'static str {
        match self {
            Self::Native => "\\",
            Self::Escaped => "\\\\",
            Self::Slashed => "/",
        }
    }
}

fn ordered(rewrites: &[PathRewrite]) -> Vec<&PathRewrite> {
    let mut ordered: Vec<&PathRewrite> = rewrites.iter().collect();
    ordered.sort_by_key(|rewrite| std::cmp::Reverse(rewrite.from.as_os_str().len()));
    ordered
}

pub fn rewrite_text(text: &str, rewrites: &[PathRewrite]) -> String {
    for rewrite in ordered(rewrites) {
        for form in Form::ALL {
            let from = form.render(&rewrite.from);
            if text == from {
                return form.render(&rewrite.to);
            }
        }
    }
    let mut text = text.to_owned();
    for rewrite in ordered(rewrites) {
        for form in Form::ALL {
            let from = format!("{}{}", form.render(&rewrite.from), form.separator());
            if text.contains(&from) {
                let to = format!("{}{}", form.render(&rewrite.to), form.separator());
                text = text.replace(&from, &to);
            }
        }
    }
    text
}

fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn tables(connection: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT name FROM pragma_table_list WHERE schema = 'main' AND type = 'table' \
         AND name NOT LIKE 'sqlite_%'",
    )?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(names)
}

fn columns(connection: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare("SELECT name FROM pragma_table_info(?1)")?;
    let names = statement
        .query_map([table], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(names)
}

fn mentions(
    connection: &Connection,
    table: &str,
    column: &str,
    roots: &[String],
) -> rusqlite::Result<bool> {
    let column = quoted(column);
    for root in roots {
        let found: Option<i64> = connection
            .query_row(
                &format!(
                    "SELECT 1 FROM {} WHERE typeof({column}) = 'text' AND instr({column}, ?1) > 0 LIMIT 1",
                    quoted(table)
                ),
                [root],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        if found.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn encode_column(
    connection: &Connection,
    table: &str,
    column: &str,
    rewrites: &[PathRewrite],
    stored: &StoredPaths,
) -> rusqlite::Result<usize> {
    let has_table = tables(connection)?.iter().any(|name| name == table);
    if !has_table {
        return Ok(0);
    }
    let rows: Vec<(i64, String)> = {
        let mut statement = connection.prepare(&format!(
            "SELECT rowid, {} FROM {} WHERE typeof({}) = 'text'",
            quoted(column),
            quoted(table),
            quoted(column)
        ))?;
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut changed = 0;
    for (rowid, value) in rows {
        let moved = rewrite_text(&value, rewrites);
        let encoded = stored.encode(Path::new(&moved));
        if encoded != value {
            connection.execute(
                &format!(
                    "UPDATE {} SET {} = ?1 WHERE rowid = ?2",
                    quoted(table),
                    quoted(column)
                ),
                params![encoded, rowid],
            )?;
            changed += 1;
        }
    }
    Ok(changed)
}

pub const STORED_COLUMNS: [(&str, &str); 2] = [("download", "target_path"), ("skill", "path")];

pub fn rewrite_database(
    connection: &Connection,
    legacy_root: &Path,
    rewrites: &[PathRewrite],
    stored: &StoredPaths,
) -> rusqlite::Result<usize> {
    let mut changed = 0;
    for (table, column) in STORED_COLUMNS {
        changed += encode_column(connection, table, column, rewrites, stored)?;
    }
    let roots: Vec<String> = Form::ALL
        .iter()
        .map(|form| form.render(legacy_root))
        .collect();
    for table in tables(connection)? {
        for column in columns(connection, &table)? {
            if !mentions(connection, &table, &column, &roots)? {
                continue;
            }
            let target = quoted(&column);
            for rewrite in ordered(rewrites) {
                for form in Form::ALL {
                    let from = form.render(&rewrite.from);
                    let to = form.render(&rewrite.to);
                    changed += connection.execute(
                        &format!(
                            "UPDATE {} SET {target} = ?2 WHERE typeof({target}) = 'text' AND {target} = ?1",
                            quoted(&table)
                        ),
                        params![from, to],
                    )?;
                    let separator = form.separator();
                    changed += connection.execute(
                        &format!(
                            "UPDATE {} SET {target} = replace({target}, ?1, ?2) \
                             WHERE typeof({target}) = 'text' AND instr({target}, ?1) > 0",
                            quoted(&table)
                        ),
                        params![format!("{from}{separator}"), format!("{to}{separator}")],
                    )?;
                }
            }
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::paths::StoredRoot;

    fn legacy() -> PathBuf {
        PathBuf::from(r"C:\Users\u\AppData\Roaming\com.zvs.aistudio")
    }

    fn root() -> PathBuf {
        PathBuf::from(r"C:\Users\u\AppData\Local\ZVS\AIStudio")
    }

    fn rewrites() -> Vec<PathRewrite> {
        vec![
            PathRewrite::new(
                legacy().join("browser").join("extensions"),
                root().join("assets").join("extensions").join("enabled"),
            ),
            PathRewrite::new(
                legacy().join("browser").join("extensions-off"),
                root().join("assets").join("extensions").join("disabled"),
            ),
            PathRewrite::new(
                legacy().join("downloads").join("models"),
                root().join("assets").join("models"),
            ),
            PathRewrite::new(
                legacy().join("conversations"),
                root().join("data").join("conversations"),
            ),
        ]
    }

    #[cfg(windows)]
    #[test]
    fn text_is_rewritten_in_every_form_and_only_at_a_path_boundary() {
        let rewrites = rewrites();
        assert_eq!(
            rewrite_text(
                r"C:\Users\u\AppData\Roaming\com.zvs.aistudio\conversations\c1\work\a.docx",
                &rewrites
            ),
            r"C:\Users\u\AppData\Local\ZVS\AIStudio\data\conversations\c1\work\a.docx"
        );
        assert_eq!(
            rewrite_text(
                r#"{"path":"C:\\Users\\u\\AppData\\Roaming\\com.zvs.aistudio\\downloads\\models\\m.gguf"}"#,
                &rewrites
            ),
            r#"{"path":"C:\\Users\\u\\AppData\\Local\\ZVS\\AIStudio\\assets\\models\\m.gguf"}"#
        );
        assert_eq!(
            rewrite_text(
                "C:/Users/u/AppData/Roaming/com.zvs.aistudio/browser/extensions-off/x",
                &rewrites
            ),
            "C:/Users/u/AppData/Local/ZVS/AIStudio/assets/extensions/disabled/x"
        );
        assert_eq!(
            rewrite_text(
                r"C:\Users\u\AppData\Roaming\com.zvs.aistudio\conversations",
                &rewrites
            ),
            r"C:\Users\u\AppData\Local\ZVS\AIStudio\data\conversations"
        );
        let unrelated = r"C:\Users\u\AppData\Roaming\com.zvs.aistudio\conversations-old\x";
        assert_eq!(rewrite_text(unrelated, &rewrites), unrelated);
    }

    #[cfg(windows)]
    #[test]
    fn the_database_gets_root_relative_columns_and_rewritten_text() {
        let connection = Connection::open_in_memory().expect("opens");
        connection
            .execute_batch(
                "CREATE TABLE download (id TEXT PRIMARY KEY, target_path TEXT NOT NULL);
                 CREATE TABLE skill (id TEXT PRIMARY KEY, path TEXT NOT NULL);
                 CREATE TABLE message (id TEXT PRIMARY KEY, body TEXT, size INTEGER);
                 CREATE TABLE setting (key TEXT PRIMARY KEY, value TEXT);",
            )
            .expect("creates");
        let legacy_model = legacy().join("downloads").join("models").join("m.gguf");
        connection
            .execute(
                "INSERT INTO download VALUES ('d1', ?1), ('d2', 'D:\\elsewhere\\x.bin')",
                [legacy_model.to_string_lossy()],
            )
            .expect("inserts");
        connection
            .execute(
                "INSERT INTO skill VALUES ('s1', 'C:\\bundle\\skills\\pdf')",
                [],
            )
            .expect("inserts");
        connection
            .execute(
                "INSERT INTO message VALUES ('m1', ?1, 3), ('m2', 'nothing here', 4)",
                [format!(
                    "Saved to {}",
                    legacy()
                        .join("conversations")
                        .join("c1")
                        .join("work")
                        .join("a.docx")
                        .display()
                )],
            )
            .expect("inserts");
        let stored = StoredPaths::new(vec![(StoredRoot::Assets, root().join("assets"))]);

        let changed =
            rewrite_database(&connection, &legacy(), &rewrites(), &stored).expect("rewrites");

        assert!(changed >= 2);
        let target: String = connection
            .query_row(
                "SELECT target_path FROM download WHERE id = 'd1'",
                [],
                |row| row.get(0),
            )
            .expect("reads");
        assert_eq!(target, "assets:models/m.gguf");
        let foreign: String = connection
            .query_row(
                "SELECT target_path FROM download WHERE id = 'd2'",
                [],
                |row| row.get(0),
            )
            .expect("reads");
        assert_eq!(foreign, r"D:\elsewhere\x.bin");
        let body: String = connection
            .query_row("SELECT body FROM message WHERE id = 'm1'", [], |row| {
                row.get(0)
            })
            .expect("reads");
        assert_eq!(
            body,
            format!(
                "Saved to {}",
                root()
                    .join("data")
                    .join("conversations")
                    .join("c1")
                    .join("work")
                    .join("a.docx")
                    .display()
            )
        );
        assert_eq!(
            rewrite_database(&connection, &legacy(), &rewrites(), &stored).expect("rewrites"),
            0
        );
    }
}
