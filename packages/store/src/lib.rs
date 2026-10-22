//! Storage abstraction for the application.
//!
//! Everything that touches the database goes through [`Store`], so the web
//! server never sees a transaction, a table definition, or a `redb` type. The
//! store is embedded (a single file on disk), which keeps the application
//! self-hosted with no external services to run.
//!
//! `redb` is synchronous, so every query here runs on a blocking task and the
//! public API is async. Callers should not wrap these in `spawn_blocking`.
//!
//! Values are stored as `postcard`-encoded bytes under string keys. That keeps
//! the store independent of any particular application model: a caller decides
//! what a key and its record mean.

mod error;

pub use self::error::{QueryError, StoreInitError};

use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::task;

/// The one table every record lives in, keyed by name.
pub const VALUES: TableDefinition<&str, &[u8]> = TableDefinition::new("values");

/// The directory created under the platform's local data directory.
pub const APP_DIRECTORY: &str = "git-repos-dashboard";

/// The database filename used when no path is configured.
pub const DATABASE_FILE: &str = "store.redb";

/// The default database path, or `None` when the platform has no data dir.
///
/// This uses the *local* data directory rather than the roaming one. A `redb`
/// database is machine-local state, and roaming it between Windows machines
/// would invite file-level conflicts. On Linux and macOS the two directories
/// are the same; the distinction only matters on Windows.
pub fn default_database_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|directory| directory.join(APP_DIRECTORY).join(DATABASE_FILE))
}

pub struct Store {
    db: Arc<Database>,
    path: PathBuf,
}

impl Store {
    pub fn builder() -> StoreBuilder {
        StoreBuilder::new()
    }

    /// Where this store's database file lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of records currently stored.
    pub async fn len(&self) -> Result<u64, QueryError> {
        let db = Arc::clone(&self.db);

        task::spawn_blocking(move || {
            let txn = db.begin_read().map_err(QueryError::Transaction)?;
            let table = txn.open_table(VALUES).map_err(QueryError::Table)?;
            table.len().map_err(QueryError::Storage)
        })
        .await
        .map_err(QueryError::Task)?
    }

    /// Whether the store holds no records.
    pub async fn is_empty(&self) -> Result<bool, QueryError> {
        Ok(self.len().await? == 0)
    }

    /// One record, decoded, or `None` when the key has never been written.
    pub async fn get<T>(&self, key: &str) -> Result<Option<T>, QueryError>
    where
        T: DeserializeOwned + Send + 'static,
    {
        let db = Arc::clone(&self.db);
        let key = key.to_owned();

        task::spawn_blocking(move || {
            let txn = db.begin_read().map_err(QueryError::Transaction)?;
            let table = txn.open_table(VALUES).map_err(QueryError::Table)?;

            match table.get(key.as_str()).map_err(QueryError::Storage)? {
                Some(value) => postcard::from_bytes(value.value())
                    .map(Some)
                    .map_err(QueryError::Decode),
                None => Ok(None),
            }
        })
        .await
        .map_err(QueryError::Task)?
    }

    /// Every record whose key starts with `prefix`, decoded.
    pub async fn list<T>(&self, prefix: &str) -> Result<Vec<T>, QueryError>
    where
        T: DeserializeOwned + Send + 'static,
    {
        let db = Arc::clone(&self.db);
        let prefix = prefix.to_owned();

        task::spawn_blocking(move || {
            let txn = db.begin_read().map_err(QueryError::Transaction)?;
            let table = txn.open_table(VALUES).map_err(QueryError::Table)?;

            let mut values = Vec::new();

            for entry in table.iter().map_err(QueryError::Storage)? {
                let (key, value) = entry.map_err(QueryError::Storage)?;

                if !key.value().starts_with(&prefix) {
                    continue;
                }

                values.push(postcard::from_bytes(value.value()).map_err(QueryError::Decode)?);
            }

            Ok(values)
        })
        .await
        .map_err(QueryError::Task)?
    }

    /// Writes one record, replacing any value already stored under `key`.
    pub async fn set<T>(&self, key: impl AsRef<str>, value: &T) -> Result<(), QueryError>
    where
        T: Serialize + ?Sized,
    {
        let db = Arc::clone(&self.db);
        let key = key.as_ref().to_owned();
        let encoded = postcard::to_allocvec(value).map_err(QueryError::Encode)?;

        task::spawn_blocking(move || {
            let txn = db.begin_write().map_err(QueryError::Transaction)?;

            {
                let mut table = txn.open_table(VALUES).map_err(QueryError::Table)?;
                table
                    .insert(key.as_str(), encoded.as_slice())
                    .map_err(QueryError::Storage)?;
            }

            txn.commit().map_err(QueryError::Commit)?;

            Ok(())
        })
        .await
        .map_err(QueryError::Task)?
    }

    /// Removes the record under `key`, if there is one.
    pub async fn remove(&self, key: &str) -> Result<(), QueryError> {
        let db = Arc::clone(&self.db);
        let key = key.to_owned();

        task::spawn_blocking(move || {
            let txn = db.begin_write().map_err(QueryError::Transaction)?;

            {
                let mut table = txn.open_table(VALUES).map_err(QueryError::Table)?;
                table.remove(key.as_str()).map_err(QueryError::Storage)?;
            }

            txn.commit().map_err(QueryError::Commit)?;

            Ok(())
        })
        .await
        .map_err(QueryError::Task)?
    }
}

pub struct StoreBuilder {
    database_path: Option<PathBuf>,
}

impl StoreBuilder {
    pub fn new() -> Self {
        Self {
            database_path: None,
        }
    }

    pub fn database(mut self, path: impl AsRef<Path>) -> Self {
        self.database_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Opens the database, creating it and its tables if they do not exist yet.
    pub async fn finish(self) -> Result<Store, StoreInitError> {
        use StoreInitError as E;

        let database_path = self
            .database_path
            .or_else(default_database_path)
            .ok_or(E::NoDataDirectory)?;

        let (db, path) = task::spawn_blocking(move || {
            if let Some(parent) = database_path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent).map_err(E::CreateDirectory)?;
            }

            let db = Database::create(&database_path).map_err(E::OpenDatabase)?;

            // Opening a table in a write transaction creates it, so that later
            // read transactions never fail on a fresh database.
            let txn = db.begin_write().map_err(E::Transaction)?;
            txn.open_table(VALUES).map_err(E::Table)?;
            txn.commit().map_err(E::Commit)?;

            Ok::<_, E>((db, database_path))
        })
        .await
        .map_err(E::Task)??;

        Ok(Store {
            db: Arc::new(db),
            path,
        })
    }
}

impl Default for StoreBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::path::PathBuf;

    async fn open_store(name: &str) -> (Store, PathBuf) {
        let path =
            std::env::temp_dir().join(format!("store-test-{}-{name}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let store = Store::builder()
            .database(&path)
            .finish()
            .await
            .expect("failed to open database");

        (store, path)
    }

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Record {
        name: String,
        count: u64,
    }

    #[tokio::test]
    async fn opens_a_fresh_database_and_reads_from_it() {
        let (store, path) = open_store("fresh").await;

        assert_eq!(store.len().await.unwrap(), 0);
        assert_eq!(store.get::<Record>("missing").await.unwrap(), None);
        assert_eq!(store.path(), path.as_path());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_default_path_uses_the_app_directory() {
        let Some(path) = default_database_path() else {
            return;
        };

        assert_eq!(path.file_name().unwrap(), DATABASE_FILE);
        assert_eq!(path.parent().unwrap().file_name().unwrap(), APP_DIRECTORY);
    }

    #[tokio::test]
    async fn a_written_record_reads_back() {
        let (store, path) = open_store("roundtrip").await;

        let record = Record {
            name: "First".to_owned(),
            count: 7,
        };

        store.set("first", &record).await.unwrap();

        assert_eq!(store.len().await.unwrap(), 1);
        assert_eq!(store.get::<Record>("first").await.unwrap(), Some(record));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn listing_returns_only_the_requested_prefix() {
        let (store, path) = open_store("list").await;

        store
            .set(
                "repo:first",
                &Record {
                    name: "First".to_owned(),
                    count: 1,
                },
            )
            .await
            .unwrap();

        store
            .set(
                "repo:second",
                &Record {
                    name: "Second".to_owned(),
                    count: 2,
                },
            )
            .await
            .unwrap();

        store
            .set(
                "other:third",
                &Record {
                    name: "Third".to_owned(),
                    count: 3,
                },
            )
            .await
            .unwrap();

        let records: Vec<Record> = store.list("repo:").await.unwrap();

        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|record| record.name == "First"));
        assert!(records.iter().any(|record| record.name == "Second"));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn setting_the_same_key_replaces_it() {
        let (store, path) = open_store("replace").await;

        store
            .set(
                "only",
                &Record {
                    name: "Before".to_owned(),
                    count: 1,
                },
            )
            .await
            .unwrap();

        store
            .set(
                "only",
                &Record {
                    name: "After".to_owned(),
                    count: 2,
                },
            )
            .await
            .unwrap();

        assert_eq!(store.len().await.unwrap(), 1);
        assert_eq!(
            store.get::<Record>("only").await.unwrap().unwrap().name,
            "After"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn removing_a_record_forgets_it() {
        let (store, path) = open_store("remove").await;

        store
            .set(
                "gone",
                &Record {
                    name: "Gone".to_owned(),
                    count: 0,
                },
            )
            .await
            .unwrap();

        store.remove("gone").await.unwrap();

        assert_eq!(store.len().await.unwrap(), 0);
        assert_eq!(store.get::<Record>("gone").await.unwrap(), None);

        let _ = std::fs::remove_file(&path);
    }
}
