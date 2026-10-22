use derive_more::{Display, Error};
use std::io;
use tokio::task;

#[derive(Debug, Display, Error)]
pub enum StoreInitError {
    #[display("could not determine a data directory for the store")]
    NoDataDirectory,
    #[display("failed to create the database directory: {_0}")]
    CreateDirectory(io::Error),
    #[display("database task failed: {_0}")]
    Task(task::JoinError),
    #[display("failed to open database: {_0}")]
    OpenDatabase(redb::DatabaseError),
    #[display("failed to begin transaction: {_0}")]
    Transaction(redb::TransactionError),
    #[display("failed to open table: {_0}")]
    Table(redb::TableError),
    #[display("failed to commit transaction: {_0}")]
    Commit(redb::CommitError),
}

#[derive(Debug, Display, Error)]
pub enum QueryError {
    #[display("database task failed: {_0}")]
    Task(task::JoinError),
    #[display("failed to begin transaction: {_0}")]
    Transaction(redb::TransactionError),
    #[display("failed to open table: {_0}")]
    Table(redb::TableError),
    #[display("failed to commit transaction: {_0}")]
    Commit(redb::CommitError),
    #[display("storage error: {_0}")]
    Storage(redb::StorageError),
    #[display("failed to encode a record: {_0}")]
    Encode(postcard::Error),
    /// A record written by an incompatible build of this crate.
    #[display("failed to decode a record: {_0}")]
    Decode(postcard::Error),
}
