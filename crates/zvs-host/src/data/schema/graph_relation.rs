use rusqlite::Row;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRelationRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub directed: bool,
}

impl GraphRelationRow {
    pub const COLUMNS: &str = "id, name, description, directed";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            description: row.get("description")?,
            directed: row.get("directed")?,
        })
    }
}
