//! Large blob support for components >64KB
//!
//! This module handles large component data using iroh-blobs. When a component
//! exceeds the inline threshold (64KB), it's stored as a blob and referenced
//! by its hash in the ComponentOp.
//!
//! **NOTE:** This is a simplified implementation for Phase 6. Full iroh-blobs
//! integration will be completed when we integrate with actual gossip
//! networking.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

use bevy::prelude::*;

use crate::networking::{
    error::{
        NetworkingError,
        Result,
    },
    messages::ComponentData,
};

/// Threshold for storing data as a blob (64KB)
pub const BLOB_THRESHOLD: usize = 64 * 1024;

/// Hash type for blob references
pub type BlobHash = Vec<u8>;

/// Bevy resource for managing blobs
///
/// This resource provides blob storage and retrieval. In Phase 6, we use
/// an in-memory cache. Later phases will integrate with iroh-blobs for
/// persistent storage and P2P transfer.
#[derive(Resource, Clone)]
pub struct BlobStore {
    /// In-memory cache of blobs (hash -> data)
    cache: Arc<Mutex<HashMap<BlobHash, Vec<u8>>>>,
}

impl BlobStore {
    /// Create a new blob store
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Store a blob and return its hash
    ///
    /// # Example
    ///
    /// ```
    /// use lib::networking::BlobStore;
    ///
    /// let store = BlobStore::new();
    /// let data = vec![1, 2, 3, 4, 5];
    /// let hash = store.store_blob(data.clone()).unwrap();
    ///
    /// let retrieved = store.get_blob(&hash).unwrap();
    /// assert_eq!(retrieved, Some(data));
    /// ```
    pub fn store_blob(&self, data: Vec<u8>) -> Result<BlobHash> {
        // Use SHA-256 for content-addressable storage
        let hash = Self::hash_data(&data);

        self.cache
            .lock()
            .map_err(|e| NetworkingError::Blob(format!("Failed to lock cache: {}", e)))?
            .insert(hash.clone(), data);

        Ok(hash)
    }

    /// Retrieve a blob by its hash
    ///
    /// Returns `None` if the blob is not in the cache.
    pub fn get_blob(&self, hash: &BlobHash) -> Result<Option<Vec<u8>>> {
        Ok(self
            .cache
            .lock()
            .map_err(|e| NetworkingError::Blob(format!("Failed to lock cache: {}", e)))?
            .get(hash)
            .cloned())
    }

    /// Check if a blob exists in the cache
    ///
    /// Returns an error if the cache lock is poisoned.
    pub fn has_blob(&self, hash: &BlobHash) -> Result<bool> {
        Ok(self
            .cache
            .lock()
            .map_err(|e| NetworkingError::Blob(format!("Failed to lock cache: {}", e)))?
            .contains_key(hash))
    }

    /// Get a blob if it exists (atomic check-and-get)
    ///
    /// This is safer than calling `has_blob()` followed by `get_blob()` because
    /// it's atomic - the blob can't be removed between the check and get.
    pub fn get_blob_if_exists(&self, hash: &BlobHash) -> Result<Option<Vec<u8>>> {
        Ok(self
            .cache
            .lock()
            .map_err(|e| NetworkingError::Blob(format!("Failed to lock cache: {}", e)))?
            .get(hash)
            .cloned())
    }

    /// Get cache size (number of blobs)
    ///
    /// Returns an error if the cache lock is poisoned.
    pub fn cache_size(&self) -> Result<usize> {
        Ok(self
            .cache
            .lock()
            .map_err(|e| NetworkingError::Blob(format!("Failed to lock cache: {}", e)))?
            .len())
    }

    /// Clear the cache
    pub fn clear_cache(&self) -> Result<()> {
        self.cache
            .lock()
            .map_err(|e| NetworkingError::Blob(format!("Failed to lock cache: {}", e)))?
            .clear();
        Ok(())
    }

    /// Hash data using SHA-256
    fn hash_data(data: &[u8]) -> BlobHash {
        use sha2::{
            Digest,
            Sha256,
        };

        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().to_vec()
    }
}

impl Default for BlobStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine whether data should be stored as a blob
///
/// # Example
///
/// ```
/// use lib::networking::should_use_blob;
///
/// let small_data = vec![1, 2, 3];
/// assert!(!should_use_blob(&small_data));
///
/// let large_data = vec![0u8; 100_000];
/// assert!(should_use_blob(&large_data));
/// ```
pub fn should_use_blob(data: &[u8]) -> bool {
    data.len() > BLOB_THRESHOLD
}

/// Create ComponentData, automatically choosing inline vs blob
///
/// This helper function inspects the data size and creates the appropriate
/// ComponentData variant.
///
/// # Example
///
/// ```
/// use lib::networking::{
///     BlobStore,
///     create_component_data,
/// };
///
/// let store = BlobStore::new();
///
/// // Small data goes inline
/// let small_data = vec![1, 2, 3];
/// let component_data = create_component_data(small_data, &store).unwrap();
///
/// // Large data becomes a blob reference
/// let large_data = vec![0u8; 100_000];
/// let component_data = create_component_data(large_data, &store).unwrap();
/// ```
pub fn create_component_data(data: Vec<u8>, blob_store: &BlobStore) -> Result<ComponentData> {
    if should_use_blob(&data) {
        let size = data.len() as u64;
        let hash = blob_store.store_blob(data)?;
        Ok(ComponentData::BlobRef { hash, size })
    } else {
        Ok(ComponentData::Inline(data))
    }
}

/// Retrieve the actual data from ComponentData
///
/// This resolves blob references by fetching from the blob store.
///
/// # Example
///
/// ```
/// use lib::networking::{
///     BlobStore,
///     ComponentData,
///     get_component_data,
/// };
///
/// let store = BlobStore::new();
///
/// // Inline data
/// let inline = ComponentData::Inline(vec![1, 2, 3]);
/// let data = get_component_data(&inline, &store).unwrap();
/// assert_eq!(data, vec![1, 2, 3]);
/// ```
pub fn get_component_data(data: &ComponentData, blob_store: &BlobStore) -> Result<Vec<u8>> {
    match data {
        | ComponentData::Inline(bytes) => Ok(bytes.clone()),
        | ComponentData::BlobRef { hash, size: _ } => blob_store
            .get_blob(hash)?
            .ok_or_else(|| NetworkingError::Blob(format!("Blob not found: {:x?}", hash))),
    }
}

/// Request a blob from the network
///
/// **NOTE:** This is a stub for Phase 6. Will be implemented in later phases
/// when we have full gossip integration.
pub fn request_blob_from_network(_hash: &BlobHash, _blob_store: &BlobStore) -> Result<()> {
    // TODO: Implement in later phases with iroh-gossip
    debug!("request_blob_from_network not yet implemented");
    Ok(())
}

/// Bevy system to handle blob requests
///
/// This system processes incoming blob requests and serves blobs to peers.
///
/// **NOTE:** Stub implementation for Phase 6.
pub fn blob_transfer_system(_blob_store: Option<Res<BlobStore>>) {
    // TODO: Implement when we have gossip networking
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_store_creation() {
        let store = BlobStore::new();
        assert_eq!(store.cache_size().unwrap(), 0);
    }

    #[test]
    fn test_store_and_retrieve_blob() {
        let store = BlobStore::new();
        let data = vec![1, 2, 3, 4, 5];

        let hash = store.store_blob(data.clone()).unwrap();
        let retrieved = store.get_blob(&hash).unwrap();

        assert_eq!(retrieved, Some(data));
    }

    #[test]
    fn test_blob_hash_is_deterministic() {
        let store = BlobStore::new();
        let data = vec![1, 2, 3, 4, 5];

        let hash1 = store.store_blob(data.clone()).unwrap();
        let hash2 = store.store_blob(data.clone()).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_has_blob() {
        let store = BlobStore::new();
        let data = vec![1, 2, 3, 4, 5];

        let hash = store.store_blob(data).unwrap();
        assert!(store.has_blob(&hash).unwrap());

        let fake_hash = vec![0; 32];
        assert!(!store.has_blob(&fake_hash).unwrap());
    }

    #[test]
    fn test_clear_cache() {
        let store = BlobStore::new();
        let data = vec![1, 2, 3, 4, 5];

        store.store_blob(data).unwrap();
        assert_eq!(store.cache_size().unwrap(), 1);

        store.clear_cache().unwrap();
        assert_eq!(store.cache_size().unwrap(), 0);
    }

    #[test]
    fn test_should_use_blob() {
        let small_data = vec![0u8; 1000];
        assert!(!should_use_blob(&small_data));

        let large_data = vec![0u8; 100_000];
        assert!(should_use_blob(&large_data));

        let threshold_data = vec![0u8; BLOB_THRESHOLD];
        assert!(!should_use_blob(&threshold_data));

        let over_threshold = vec![0u8; BLOB_THRESHOLD + 1];
        assert!(should_use_blob(&over_threshold));
    }

    #[test]
    fn test_create_component_data_inline() {
        let store = BlobStore::new();
        let small_data = vec![1, 2, 3];

        let component_data = create_component_data(small_data.clone(), &store).unwrap();

        match component_data {
            | ComponentData::Inline(data) => assert_eq!(data, small_data),
            | ComponentData::BlobRef { .. } => panic!("Expected inline data"),
        }
    }

    #[test]
    fn test_create_component_data_blob() {
        let store = BlobStore::new();
        let large_data = vec![0u8; 100_000];

        let component_data = create_component_data(large_data.clone(), &store).unwrap();

        match component_data {
            | ComponentData::BlobRef { hash, size } => {
                assert_eq!(size, 100_000);
                assert!(store.has_blob(&hash).unwrap());
            },
            | ComponentData::Inline(_) => panic!("Expected blob reference"),
        }
    }

    #[test]
    fn test_get_component_data_inline() {
        let store = BlobStore::new();
        let inline = ComponentData::Inline(vec![1, 2, 3]);

        let data = get_component_data(&inline, &store).unwrap();
        assert_eq!(data, vec![1, 2, 3]);
    }

    #[test]
    fn test_get_component_data_blob() {
        let store = BlobStore::new();
        let large_data = vec![0u8; 100_000];
        let hash = store.store_blob(large_data.clone()).unwrap();

        let blob_ref = ComponentData::BlobRef {
            hash,
            size: 100_000,
        };

        let data = get_component_data(&blob_ref, &store).unwrap();
        assert_eq!(data, large_data);
    }

    #[test]
    fn test_get_component_data_missing_blob() {
        let store = BlobStore::new();
        let fake_hash = vec![0; 32];

        let blob_ref = ComponentData::BlobRef {
            hash: fake_hash,
            size: 1000,
        };

        let result = get_component_data(&blob_ref, &store);
        assert!(result.is_err());
    }
}
