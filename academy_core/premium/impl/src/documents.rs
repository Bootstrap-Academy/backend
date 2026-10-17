//! Immutable, explicitly approved purchase and renewal document bundle.
use academy_assets::email::{AGB_2026_09_R4_PDF, WIDERRUFSBELEHRUNG_2026_09_R1_PDF};
use academy_models::{
    learning_policy::{LearningDocumentBundleConfig, LearningMode, LearningPolicyConfig},
    purchase::PurchaseStage2Config,
};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct PurchaseDocuments {
    pub terms_pdf: Vec<u8>,
    pub terms_version: String,
    pub withdrawal_pdf: Vec<u8>,
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

    /// The documents of stage 2 of the order dialog (terms 2026-10) and the time from which
    /// new offers bind them; `None` while stage 2 is not configured.
    pub fn load_stage2(
        config: Option<&PurchaseStage2Config>,
    ) -> anyhow::Result<Option<(DateTime<Utc>, Self)>> {
        let Some(config) = config else {
            return Ok(None);
        };
        anyhow::ensure!(
            config.documents.terms_version.to_string() != "2026-09-r4",
            "purchase stage 2 needs a terms version after 2026-09-r4"
        );
        let documents = Self::load_bundle(&config.documents)?;
        anyhow::ensure!(
            documents.terms_pdf != AGB_2026_09_R4_PDF
                && documents.withdrawal_pdf != WIDERRUFSBELEHRUNG_2026_09_R1_PDF,
            "purchase stage 2 must not bind the documents of stage 1"
        );
        Ok(Some((config.effective_from, documents)))
    }

    /// A bundle of approved PDFs, read from absolute paths and checked against their SHA-256.
    fn load_bundle(bundle: &LearningDocumentBundleConfig) -> anyhow::Result<Self> {
        let read = |path: &std::path::Path, expected: &str| -> anyhow::Result<Vec<u8>> {
            anyhow::ensure!(
                path.is_absolute(),
                "purchase document paths must be absolute"
            );
            let size = std::fs::metadata(path)?.len();
            anyhow::ensure!(
                (8..=10_485_760).contains(&size),
                "purchase PDF size is invalid"
            );
            let bytes = std::fs::read(path)?;
            anyhow::ensure!(
                bytes.starts_with(b"%PDF-"),
                "purchase document is not a PDF"
            );
            anyhow::ensure!(
                format!("{:x}", Sha256::digest(&bytes)) == expected,
                "purchase document does not match the approved SHA-256"
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

fn hash_documents(terms: &[u8], withdrawal: &[u8]) -> String {
    let mut hash = Sha256::new();
    for bytes in [terms, withdrawal] {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    format!("{:x}", hash.finalize())
}
