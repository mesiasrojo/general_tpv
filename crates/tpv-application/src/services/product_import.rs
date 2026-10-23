use std::sync::Arc;

use tpv_domain::ids::StoreId;

use crate::dto::SyncContext;
use crate::error::Result;
use crate::import::{
    ProductImportPlan, ProductImportPreview, ProductImportReport, parse_product_import_csv,
};
use crate::ports::ProductImportRepository;

/// Coordinates CSV validation with the atomic storage import.
#[derive(Clone)]
pub struct ProductImportService {
    repository: Arc<dyn ProductImportRepository>,
}

impl ProductImportService {
    pub fn new(repository: Arc<dyn ProductImportRepository>) -> Self {
        Self { repository }
    }

    /// Parses and validates the file. No database access happens here, so the
    /// same plan can be previewed and then committed without re-reading disk.
    pub fn prepare(
        &self,
        source_system: &str,
        batch_id: Option<&str>,
        csv: &str,
    ) -> Result<ProductImportPlan> {
        parse_product_import_csv(source_system, batch_id, csv)
    }

    /// Builds the report for a plan without touching storage. Useful when a
    /// caller needs the same error shape for a plan that is not committable.
    #[must_use]
    pub fn report(&self, plan: &ProductImportPlan, dry_run: bool) -> ProductImportReport {
        base_report(plan, dry_run)
    }

    /// Classifies the batch without writing anything.
    pub async fn preview(&self, plan: &ProductImportPlan) -> Result<ProductImportReport> {
        let mut report = base_report(plan, true);
        if plan.rows.is_empty() {
            return Ok(report);
        }
        let preview = self
            .repository
            .preview_import(&plan.source_system, &plan.batch_id, &plan.rows)
            .await?;
        apply_preview(&mut report, preview);
        Ok(report)
    }

    /// Commits the batch atomically. Invalid plans are returned untouched so
    /// callers can display the row-level report.
    pub async fn commit(
        &self,
        context: &SyncContext,
        store_id: StoreId,
        plan: &ProductImportPlan,
    ) -> Result<ProductImportReport> {
        let mut report = base_report(plan, false);
        if !plan.is_valid() {
            return Ok(report);
        }
        let preview = self
            .repository
            .commit_import(
                context,
                store_id,
                &plan.source_system,
                &plan.batch_id,
                &plan.rows,
            )
            .await?;
        apply_preview(&mut report, preview);
        report.committed = true;
        Ok(report)
    }
}

fn base_report(plan: &ProductImportPlan, dry_run: bool) -> ProductImportReport {
    ProductImportReport {
        source_system: plan.source_system.clone(),
        batch_id: plan.batch_id.clone(),
        dry_run,
        committed: false,
        already_imported: false,
        total_rows: plan.total_rows,
        valid_rows: plan.rows.len(),
        invalid_rows: plan.errors.len(),
        inserted: 0,
        updated: 0,
        unchanged: 0,
        errors: plan.errors.clone(),
    }
}

fn apply_preview(report: &mut ProductImportReport, preview: ProductImportPreview) {
    report.already_imported = preview.already_imported;
    report.inserted = preview.inserted;
    report.updated = preview.updated;
    report.unchanged = preview.unchanged;
}
