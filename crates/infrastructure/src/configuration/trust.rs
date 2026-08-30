use std::path::{Path, PathBuf};

use heikas_application::configuration::RepositoryTrustRecord;
use heikas_application::error::ApplicationResult;
use heikas_domain::clock::Timestamp;
use heikas_domain::identity::ContentDigest;
use serde::{Deserialize, Serialize};

use crate::atomic::{read_json, write_atomic_json};
use crate::layout::StoreLayout;

const TRUST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrustDocument {
    schema_version: u32,
    #[serde(default)]
    repositories: Vec<RepositoryTrustRecord>,
}

impl Default for TrustDocument {
    fn default() -> Self {
        Self {
            schema_version: TRUST_SCHEMA_VERSION,
            repositories: Vec::new(),
        }
    }
}

pub struct FileRepositoryTrustStore {
    path: PathBuf,
}

impl FileRepositoryTrustStore {
    pub fn new(layout: &StoreLayout) -> Self {
        Self {
            path: layout.config_directory().join("trusted-repositories.json"),
        }
    }

    pub fn identity_of(repository: &Path) -> String {
        let resolved =
            std::fs::canonicalize(repository).unwrap_or_else(|_| repository.to_path_buf());
        resolved.display().to_string().replace('\\', "/")
    }

    fn load(&self) -> ApplicationResult<TrustDocument> {
        Ok(read_json::<TrustDocument>(&self.path)?.unwrap_or_default())
    }

    fn store(&self, document: &TrustDocument) -> ApplicationResult<()> {
        write_atomic_json(&self.path, document)
    }

    pub fn record_for(
        &self,
        repository: &Path,
    ) -> ApplicationResult<Option<RepositoryTrustRecord>> {
        let identity = Self::identity_of(repository);
        Ok(self
            .load()?
            .repositories
            .into_iter()
            .find(|record| record.repository_path == identity))
    }

    pub fn grant(
        &self,
        repository: &Path,
        configuration_digest: ContentDigest,
        granted_at: Timestamp,
    ) -> ApplicationResult<RepositoryTrustRecord> {
        let identity = Self::identity_of(repository);
        let mut document = self.load()?;
        document.schema_version = TRUST_SCHEMA_VERSION;
        document
            .repositories
            .retain(|record| record.repository_path != identity);
        let record = RepositoryTrustRecord {
            repository_path: identity,
            configuration_digest,
            granted_at,
        };
        document.repositories.push(record.clone());
        document
            .repositories
            .sort_by(|left, right| left.repository_path.cmp(&right.repository_path));
        self.store(&document)?;
        Ok(record)
    }

    pub fn revoke(&self, repository: &Path) -> ApplicationResult<bool> {
        let identity = Self::identity_of(repository);
        let mut document = self.load()?;
        let before = document.repositories.len();
        document
            .repositories
            .retain(|record| record.repository_path != identity);
        if document.repositories.len() == before {
            return Ok(false);
        }
        self.store(&document)?;
        Ok(true)
    }

    pub fn records(&self) -> ApplicationResult<Vec<RepositoryTrustRecord>> {
        Ok(self.load()?.repositories)
    }
}
