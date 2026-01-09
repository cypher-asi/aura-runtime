//! `RocksDB` implementation of the Store trait.

use crate::cf;
use crate::error::StoreError;
use crate::keys::{AgentMetaKey, InboxKey, KeyCodec, RecordKey};
use crate::store::{AgentStatus, Store};
use aura_core::{AgentId, RecordEntry, Transaction};
use rocksdb::{
    BoundColumnFamily, ColumnFamilyDescriptor, DBWithThreadMode, IteratorMode, MultiThreaded,
    Options, WriteBatch, WriteOptions,
};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, instrument};

/// `RocksDB`-based store implementation.
pub struct RocksStore {
    db: Arc<DBWithThreadMode<MultiThreaded>>,
    sync_writes: bool,
}

impl RocksStore {
    /// Open or create a `RocksDB` store at the given path.
    ///
    /// # Errors
    /// Returns error if the database cannot be opened.
    pub fn open(path: impl AsRef<Path>, sync_writes: bool) -> Result<Self, StoreError> {
        let path = path.as_ref();
        debug!(?path, "Opening RocksDB store");

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Define column families
        let cf_names = [cf::RECORD, cf::AGENT_META, cf::INBOX];
        let cf_descriptors: Vec<_> = cf_names
            .iter()
            .map(|name| {
                let cf_opts = Options::default();
                ColumnFamilyDescriptor::new(*name, cf_opts)
            })
            .collect();

        let db =
            DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(&opts, path, cf_descriptors)?;

        Ok(Self {
            db: Arc::new(db),
            sync_writes,
        })
    }

    /// Get a column family handle.
    fn cf(&self, name: &str) -> Result<Arc<BoundColumnFamily<'_>>, StoreError> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| StoreError::ColumnFamilyNotFound(name.to_string()))
    }

    /// Create write options based on `sync_writes` setting.
    fn write_opts(&self) -> WriteOptions {
        let mut opts = WriteOptions::default();
        opts.set_sync(self.sync_writes);
        opts
    }

    /// Read a u64 value from agent metadata.
    fn read_meta_u64(&self, key: &AgentMetaKey) -> Result<u64, StoreError> {
        let cf = self.cf(cf::AGENT_META)?;
        let encoded_key = key.encode();

        match self.db.get_cf(&cf, &encoded_key)? {
            Some(bytes) => {
                let arr: [u8; 8] = bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| StoreError::Deserialization("invalid u64 bytes".to_string()))?;
                Ok(u64::from_be_bytes(arr))
            }
            None => Ok(0), // Default to 0 if not set
        }
    }
}

impl Store for RocksStore {
    #[instrument(skip(self, tx), fields(agent_id = %tx.agent_id, tx_id = %tx.tx_id))]
    fn enqueue_tx(&self, tx: &Transaction) -> Result<(), StoreError> {
        let cf_inbox = self.cf(cf::INBOX)?;
        let cf_meta = self.cf(cf::AGENT_META)?;

        // Get current inbox tail
        let tail_key = AgentMetaKey::inbox_tail(tx.agent_id);
        let tail = self.read_meta_u64(&tail_key)?;

        // Create inbox key
        let inbox_key = InboxKey::new(tx.agent_id, tail);

        // Serialize transaction
        let tx_bytes = serde_json::to_vec(tx)?;

        // Write batch: inbox entry + update tail
        let mut batch = WriteBatch::default();
        batch.put_cf(&cf_inbox, inbox_key.encode(), tx_bytes);
        batch.put_cf(&cf_meta, tail_key.encode(), (tail + 1).to_be_bytes());

        self.db.write_opt(batch, &self.write_opts())?;

        debug!(inbox_seq = tail, "Transaction enqueued");
        Ok(())
    }

    #[instrument(skip(self), fields(agent_id = %agent_id))]
    fn dequeue_tx(&self, agent_id: AgentId) -> Result<Option<(u64, Transaction)>, StoreError> {
        let cf_inbox = self.cf(cf::INBOX)?;

        // Get current inbox head and tail
        let head_key = AgentMetaKey::inbox_head(agent_id);
        let tail_key = AgentMetaKey::inbox_tail(agent_id);
        let head = self.read_meta_u64(&head_key)?;
        let tail = self.read_meta_u64(&tail_key)?;

        // Check if inbox is empty
        if head >= tail {
            debug!("Inbox empty");
            return Ok(None);
        }

        // Read the transaction at head
        let inbox_key = InboxKey::new(agent_id, head);
        let encoded_key = inbox_key.encode();

        if let Some(bytes) = self.db.get_cf(&cf_inbox, &encoded_key)? {
            let tx: Transaction = serde_json::from_slice(&bytes)
                .map_err(|e| StoreError::Deserialization(e.to_string()))?;
            debug!(inbox_seq = head, "Transaction dequeued");
            Ok(Some((head, tx)))
        } else {
            // This shouldn't happen if head < tail, but handle gracefully
            debug!("Inbox entry missing at head");
            Ok(None)
        }
    }

    #[instrument(skip(self), fields(agent_id = %agent_id))]
    fn get_head_seq(&self, agent_id: AgentId) -> Result<u64, StoreError> {
        let key = AgentMetaKey::head_seq(agent_id);
        self.read_meta_u64(&key)
    }

    #[instrument(skip(self, entry), fields(agent_id = %agent_id, seq = next_seq))]
    fn append_entry_atomic(
        &self,
        agent_id: AgentId,
        next_seq: u64,
        entry: &RecordEntry,
        dequeued_inbox_seq: u64,
    ) -> Result<(), StoreError> {
        let cf_record = self.cf(cf::RECORD)?;
        let cf_meta = self.cf(cf::AGENT_META)?;
        let cf_inbox = self.cf(cf::INBOX)?;

        // Verify sequence
        let current_head = self.get_head_seq(agent_id)?;
        if next_seq != current_head + 1 {
            return Err(StoreError::SequenceMismatch {
                expected: current_head + 1,
                actual: next_seq,
            });
        }

        // Serialize entry
        let entry_bytes = serde_json::to_vec(entry)?;

        // Create keys
        let record_key = RecordKey::new(agent_id, next_seq);
        let head_seq_key = AgentMetaKey::head_seq(agent_id);
        let inbox_key = InboxKey::new(agent_id, dequeued_inbox_seq);
        let inbox_head_key = AgentMetaKey::inbox_head(agent_id);

        // Atomic batch write
        let mut batch = WriteBatch::default();

        // 1. Put record entry
        batch.put_cf(&cf_record, record_key.encode(), entry_bytes);

        // 2. Update head_seq
        batch.put_cf(&cf_meta, head_seq_key.encode(), next_seq.to_be_bytes());

        // 3. Delete inbox entry
        batch.delete_cf(&cf_inbox, inbox_key.encode());

        // 4. Update inbox_head cursor
        batch.put_cf(
            &cf_meta,
            inbox_head_key.encode(),
            (dequeued_inbox_seq + 1).to_be_bytes(),
        );

        self.db.write_opt(batch, &self.write_opts())?;

        debug!("Record entry committed atomically");
        Ok(())
    }

    #[instrument(skip(self), fields(agent_id = %agent_id, from_seq, limit))]
    fn scan_record(
        &self,
        agent_id: AgentId,
        from_seq: u64,
        limit: usize,
    ) -> Result<Vec<RecordEntry>, StoreError> {
        let cf = self.cf(cf::RECORD)?;

        let start_key = RecordKey::scan_from(agent_id, from_seq);
        let end_key = RecordKey::scan_end(agent_id);

        let iter = self.db.iterator_cf(
            &cf,
            IteratorMode::From(&start_key, rocksdb::Direction::Forward),
        );

        let mut entries = Vec::with_capacity(limit);

        for item in iter {
            let (key, value) = item?;

            // Check if we're still within the agent's range
            if key.as_ref() >= end_key.as_slice() {
                break;
            }

            // Decode and verify key
            let record_key =
                RecordKey::decode(&key).map_err(|e| StoreError::InvalidKey(e.to_string()))?;

            if record_key.agent_id != agent_id {
                break;
            }

            // Deserialize entry
            let entry: RecordEntry = serde_json::from_slice(&value)
                .map_err(|e| StoreError::Deserialization(e.to_string()))?;

            entries.push(entry);

            if entries.len() >= limit {
                break;
            }
        }

        debug!(count = entries.len(), "Record scan complete");
        Ok(entries)
    }

    #[instrument(skip(self), fields(agent_id = %agent_id, seq))]
    fn get_record_entry(&self, agent_id: AgentId, seq: u64) -> Result<RecordEntry, StoreError> {
        let cf = self.cf(cf::RECORD)?;
        let key = RecordKey::new(agent_id, seq);

        match self.db.get_cf(&cf, key.encode())? {
            Some(bytes) => {
                let entry: RecordEntry = serde_json::from_slice(&bytes)
                    .map_err(|e| StoreError::Deserialization(e.to_string()))?;
                Ok(entry)
            }
            None => Err(StoreError::RecordEntryNotFound(agent_id, seq)),
        }
    }

    #[instrument(skip(self), fields(agent_id = %agent_id))]
    fn get_agent_status(&self, agent_id: AgentId) -> Result<AgentStatus, StoreError> {
        let cf = self.cf(cf::AGENT_META)?;
        let key = AgentMetaKey::status(agent_id);

        match self.db.get_cf(&cf, key.encode())? {
            Some(bytes) => {
                if bytes.is_empty() {
                    return Ok(AgentStatus::default());
                }
                AgentStatus::from_byte(bytes[0])
                    .ok_or_else(|| StoreError::Deserialization("invalid agent status".to_string()))
            }
            None => Ok(AgentStatus::default()),
        }
    }

    #[instrument(skip(self), fields(agent_id = %agent_id, ?status))]
    fn set_agent_status(&self, agent_id: AgentId, status: AgentStatus) -> Result<(), StoreError> {
        let cf = self.cf(cf::AGENT_META)?;
        let key = AgentMetaKey::status(agent_id);

        self.db
            .put_cf_opt(&cf, key.encode(), [status.as_byte()], &self.write_opts())?;
        Ok(())
    }

    fn has_pending_tx(&self, agent_id: AgentId) -> Result<bool, StoreError> {
        let head = self.read_meta_u64(&AgentMetaKey::inbox_head(agent_id))?;
        let tail = self.read_meta_u64(&AgentMetaKey::inbox_tail(agent_id))?;
        Ok(tail > head)
    }

    fn get_inbox_depth(&self, agent_id: AgentId) -> Result<u64, StoreError> {
        let head = self.read_meta_u64(&AgentMetaKey::inbox_head(agent_id))?;
        let tail = self.read_meta_u64(&AgentMetaKey::inbox_tail(agent_id))?;
        Ok(tail.saturating_sub(head))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::{Decision, ProposalSet, TransactionKind};
    use bytes::Bytes;
    use tempfile::TempDir;

    fn create_test_store() -> (RocksStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = RocksStore::open(dir.path(), false).unwrap();
        (store, dir)
    }

    fn create_test_tx(agent_id: AgentId) -> Transaction {
        Transaction::new(
            aura_core::TxId::from_content(b"test"),
            agent_id,
            1000,
            TransactionKind::UserPrompt,
            Bytes::from_static(b"test payload"),
        )
    }

    #[test]
    fn test_enqueue_dequeue() {
        let (store, _dir) = create_test_store();
        let agent_id = AgentId::generate();
        let tx = create_test_tx(agent_id);

        // Enqueue
        store.enqueue_tx(&tx).unwrap();

        // Dequeue
        let result = store.dequeue_tx(agent_id).unwrap();
        assert!(result.is_some());

        let (inbox_seq, dequeued_tx) = result.unwrap();
        assert_eq!(inbox_seq, 0);
        assert_eq!(dequeued_tx.tx_id, tx.tx_id);
    }

    #[test]
    fn test_inbox_empty() {
        let (store, _dir) = create_test_store();
        let agent_id = AgentId::generate();

        let result = store.dequeue_tx(agent_id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_atomic_commit() {
        let (store, _dir) = create_test_store();
        let agent_id = AgentId::generate();
        let tx = create_test_tx(agent_id);

        // Enqueue transaction
        store.enqueue_tx(&tx).unwrap();

        // Get head_seq (should be 0)
        let head_seq = store.get_head_seq(agent_id).unwrap();
        assert_eq!(head_seq, 0);

        // Dequeue
        let (inbox_seq, _) = store.dequeue_tx(agent_id).unwrap().unwrap();

        // Create record entry
        let entry = RecordEntry::builder(1, tx)
            .context_hash([0u8; 32])
            .proposals(ProposalSet::new())
            .decision(Decision::new())
            .build();

        // Atomic commit
        store
            .append_entry_atomic(agent_id, 1, &entry, inbox_seq)
            .unwrap();

        // Verify head_seq updated
        let new_head = store.get_head_seq(agent_id).unwrap();
        assert_eq!(new_head, 1);

        // Verify inbox is empty
        assert!(!store.has_pending_tx(agent_id).unwrap());

        // Verify record entry exists
        let retrieved = store.get_record_entry(agent_id, 1).unwrap();
        assert_eq!(retrieved.seq, 1);
    }

    #[test]
    fn test_scan_record() {
        let (store, _dir) = create_test_store();
        let agent_id = AgentId::generate();

        // Create and commit 5 entries
        for i in 1..=5 {
            let tx = create_test_tx(agent_id);
            store.enqueue_tx(&tx).unwrap();
            let (inbox_seq, tx) = store.dequeue_tx(agent_id).unwrap().unwrap();

            #[allow(clippy::cast_possible_truncation)] // i is always 1-5 in test
            let entry = RecordEntry::builder(i, tx)
                .context_hash([i as u8; 32])
                .build();

            store
                .append_entry_atomic(agent_id, i, &entry, inbox_seq)
                .unwrap();
        }

        // Scan all
        let entries = store.scan_record(agent_id, 1, 10).unwrap();
        assert_eq!(entries.len(), 5);

        // Scan from seq 3
        let entries = store.scan_record(agent_id, 3, 10).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].seq, 3);

        // Scan with limit
        let entries = store.scan_record(agent_id, 1, 2).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_agent_status() {
        let (store, _dir) = create_test_store();
        let agent_id = AgentId::generate();

        // Default status
        let status = store.get_agent_status(agent_id).unwrap();
        assert_eq!(status, AgentStatus::Active);

        // Set paused
        store
            .set_agent_status(agent_id, AgentStatus::Paused)
            .unwrap();
        let status = store.get_agent_status(agent_id).unwrap();
        assert_eq!(status, AgentStatus::Paused);
    }

    #[test]
    fn test_sequence_mismatch() {
        let (store, _dir) = create_test_store();
        let agent_id = AgentId::generate();
        let tx = create_test_tx(agent_id);

        store.enqueue_tx(&tx).unwrap();
        let (inbox_seq, tx) = store.dequeue_tx(agent_id).unwrap().unwrap();

        let entry = RecordEntry::builder(5, tx) // Wrong seq - should be 1
            .build();

        let result = store.append_entry_atomic(agent_id, 5, &entry, inbox_seq);
        assert!(matches!(result, Err(StoreError::SequenceMismatch { .. })));
    }
}
