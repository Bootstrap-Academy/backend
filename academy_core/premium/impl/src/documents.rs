//! Immutable, explicitly approved purchase and renewal document bundle.
use academy_assets::email::{AGB_2026_09_R4_PDF, WIDERRUFSBELEHRUNG_2026_09_R1_PDF};
use std::path::Path;

use academy_models::{
    learning_policy::{LearningDocumentBundleConfig, LearningMode, LearningPolicyConfig},
    legal_time::starts_a_berlin_day,
    purchase::{PurchaseDocumentManifest, PurchaseStage2Config},
};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

/// The terms release stage 2 binds. Its declarations and texts are written for it.
pub const STAGE2_RELEASE: &str = "2026-10";

#[derive(Debug, Clone)]
pub struct PurchaseDocuments {
    pub terms_pdf: Vec<u8>,
    pub terms_version: String,
    pub withdrawal_pdf: Vec<u8>,
}

/// From `effective_from` on, the backend issues its own offers of stage 2 (terms 2026-10):
/// they bind `documents`, are accepted with the order button, and carry a declaration only
/// for a Premium period that begins within the withdrawal period. The monthly Premium
/// renewal ordered from then on binds the same documents. Offers issued before keep their own
/// documents and rules.
#[derive(Debug, Clone)]
pub struct PurchaseStage2 {
    pub effective_from: DateTime<Utc>,
    pub documents: PurchaseDocuments,
}

impl PurchaseStage2 {
    /// Whether something issued at `at` belongs to stage 2.
    pub fn applies_at(&self, at: DateTime<Utc>) -> bool {
        at >= self.effective_from
    }
}

impl PurchaseDocuments {
    pub fn legacy() -> Self {
        Self {
            terms_version: "2026-09-r4".into(),
            terms_pdf: AGB_2026_09_R4_PDF.to_vec(),
            withdrawal_pdf: WIDERRUFSBELEHRUNG_2026_09_R1_PDF.to_vec(),
        }
    }

    pub fn load_daily(config: &LearningPolicyConfig) -> anyhow::Result<Option<Self>> {
        let Some(bundle) = &config.daily_documents else {
            anyhow::ensure!(
                config.mode != LearningMode::Daily,
                "daily learning cannot activate without an approved purchase document bundle"
            );
            return Ok(None);
        };
        anyhow::ensure!(
            config.terms_version.as_ref() == Some(&bundle.terms_version),
            "daily purchase document version must match the learning policy terms"
        );
        let documents = Self::load_bundle(bundle)?;
        anyhow::ensure!(
            documents.terms_pdf != AGB_2026_09_R4_PDF,
            "daily learning must not bind the legacy r4 terms"
        );
        Ok(Some(documents))
    }

    /// Stage 2 of the order dialog (terms 2026-10) from the configured manifest; `None` while
    /// stage 2 is not configured. The manifest has to match its pinned SHA-256, name release
    /// 2026-10, come from a clean frontend checkout and start at 00:00 in Europe/Berlin; the
    /// two PDFs next to it have to match the SHA-256 it records.
    pub fn load_stage2(
        config: Option<&PurchaseStage2Config>,
    ) -> anyhow::Result<Option<PurchaseStage2>> {
        let Some(config) = config else {
            return Ok(None);
        };
        let bytes = read_pinned(&config.manifest_path, &config.manifest_sha256, 1_048_576)
            .map_err(|err| err.context("purchase stage 2 manifest"))?;
        let manifest = PurchaseDocumentManifest::parse(&bytes)
            .map_err(|err| err.context("purchase stage 2 manifest is not valid"))?;
        anyhow::ensure!(
            manifest.release == STAGE2_RELEASE && manifest.bundle.terms_version == STAGE2_RELEASE,
            "purchase stage 2 binds terms {STAGE2_RELEASE}, the manifest names release {} and terms {}",
            manifest.release,
            manifest.bundle.terms_version
        );
        anyhow::ensure!(
            !manifest.frontend_commit.ends_with("-dirty"),
            "the purchase stage 2 documents were rendered from a frontend checkout with changes"
        );
        let effective_from = manifest.effective_from.with_timezone(&Utc);
        anyhow::ensure!(
            starts_a_berlin_day(effective_from),
            "the start of terms {STAGE2_RELEASE} in the manifest is not 00:00 in Europe/Berlin"
        );
        let directory = config
            .manifest_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("purchase stage 2 manifest has no directory"))?;
        let beside = |name: &str| -> anyhow::Result<std::path::PathBuf> {
            anyhow::ensure!(
                !name.is_empty() && Path::new(name).file_name() == Some(name.as_ref()),
                "the manifest names its PDFs by file name only"
            );
            Ok(directory.join(name))
        };
        let documents = Self::load_bundle(&LearningDocumentBundleConfig {
            terms_version: manifest.bundle.terms_version.clone().try_into()?,
            terms_pdf_path: beside(&manifest.bundle.terms_pdf)?,
            terms_sha256: manifest.bundle.terms_sha256.clone(),
            withdrawal_pdf_path: beside(&manifest.bundle.withdrawal_pdf)?,
            withdrawal_sha256: manifest.bundle.withdrawal_sha256.clone(),
        })?;
        anyhow::ensure!(
            documents.terms_pdf != AGB_2026_09_R4_PDF
                && documents.withdrawal_pdf != WIDERRUFSBELEHRUNG_2026_09_R1_PDF,
            "purchase stage 2 must not bind the documents of stage 1"
        );
        Ok(Some(PurchaseStage2 {
            effective_from,
            documents,
        }))
    }

    /// A bundle of approved PDFs, read from absolute paths and checked against their SHA-256.
    fn load_bundle(bundle: &LearningDocumentBundleConfig) -> anyhow::Result<Self> {
        let read = |path: &Path, expected: &str| -> anyhow::Result<Vec<u8>> {
            let bytes = read_pinned(path, expected, 10_485_760)?;
            anyhow::ensure!(
                bytes.starts_with(b"%PDF-"),
                "purchase document is not a PDF"
            );
            Ok(bytes)
        };
        Ok(Self {
            terms_pdf: read(&bundle.terms_pdf_path, &bundle.terms_sha256)?,
            terms_version: bundle.terms_version.to_string(),
            withdrawal_pdf: read(&bundle.withdrawal_pdf_path, &bundle.withdrawal_sha256)?,
        })
    }

    pub fn hash(&self) -> String {
        hash_documents(&self.terms_pdf, &self.withdrawal_pdf)
    }
}

/// A file from an absolute path, at most `max` bytes, that matches its pinned SHA-256.
fn read_pinned(path: &Path, expected: &str, max: u64) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        path.is_absolute(),
        "purchase document paths must be absolute"
    );
    let size = std::fs::metadata(path)?.len();
    anyhow::ensure!((8..=max).contains(&size), "purchase file size is invalid");
    let bytes = std::fs::read(path)?;
    anyhow::ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == expected,
        "purchase file {} does not match the approved SHA-256",
        path.display()
    );
    Ok(bytes)
}

fn hash_documents(terms: &[u8], withdrawal: &[u8]) -> String {
    let mut hash = Sha256::new();
    for bytes in [terms, withdrawal] {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    format!("{:x}", hash.finalize())
}
