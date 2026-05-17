use rusqlite::{Connection, Result};

pub(crate) struct Database {
    connection: Connection,
}

impl Database {
    pub(crate) fn sqlite_version(&self) -> Result<String> {
        self.connection
            .query_row("select sqlite_version()", [], |row| row.get(0))
    }
}

pub(crate) fn open_memory_database() -> Result<Database> {
    let connection = Connection::open_in_memory()?;
    connection.execute(
        "create table if not exists health_check (id integer primary key)",
        [],
    )?;

    Ok(Database { connection })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_sqlite_database() {
        let database = open_memory_database().unwrap();

        assert!(!database.sqlite_version().unwrap().is_empty());
    }
}
