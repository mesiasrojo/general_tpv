use async_trait::async_trait;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use tpv_application::{
    PrintJob, PrintJobStatus, PrintJobType, PrintQueueRepository, Result as AppResult,
    TicketDocument,
};
use tpv_domain::ids::{PrintJobId, SaleId};

use crate::error::StorageResult;
use crate::helpers::{
    enum_from_text, enum_to_text, format_time, parse_optional_time, parse_time, parse_uuid,
};
use crate::store::SqliteStore;

fn row_to_print_job(row: &SqliteRow) -> StorageResult<PrintJob> {
    let document: TicketDocument =
        serde_json::from_str(&row.try_get::<String, _>("document_json")?)?;
    Ok(PrintJob {
        id: PrintJobId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        sale_id: SaleId::from_uuid(parse_uuid(&row.try_get::<String, _>("sale_id")?)?),
        job_type: enum_from_text::<PrintJobType>(&row.try_get::<String, _>("job_type")?)?,
        document,
        bytes: row.try_get::<Vec<u8>, _>("bytes")?,
        status: enum_from_text::<PrintJobStatus>(&row.try_get::<String, _>("status")?)?,
        attempts: u32::try_from(row.try_get::<i64, _>("attempts")?)
            .map_err(|_| crate::error::StorageError::InvalidData("invalid attempts".into()))?,
        created_at: parse_time(&row.try_get::<String, _>("created_at")?)?,
        printed_at: parse_optional_time(row.try_get("printed_at")?)?,
        last_error: row.try_get("last_error")?,
    })
}

impl SqliteStore {
    async fn enqueue_print_job_inner(&self, job: &PrintJob) -> StorageResult<()> {
        let document_json = serde_json::to_string(&job.document)?;
        sqlx::query(
            "INSERT INTO print_jobs
                (id, sale_id, job_type, document_json, bytes, status, attempts, created_at, printed_at, last_error)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(job.id.to_string())
        .bind(job.sale_id.to_string())
        .bind(enum_to_text(&job.job_type)?)
        .bind(document_json)
        .bind(job.bytes.as_slice())
        .bind(enum_to_text(&job.status)?)
        .bind(i64::from(job.attempts))
        .bind(format_time(job.created_at)?)
        .bind(job.printed_at.map(format_time).transpose()?)
        .bind(job.last_error.as_deref())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    async fn pending_print_jobs_inner(&self, limit: u32) -> StorageResult<Vec<PrintJob>> {
        let rows = sqlx::query(
            "SELECT * FROM print_jobs WHERE status = 'pending' ORDER BY created_at LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(row_to_print_job).collect()
    }

    async fn mark_print_job_printed_inner(&self, id: PrintJobId) -> StorageResult<()> {
        sqlx::query(
            "UPDATE print_jobs
             SET status = 'printed', printed_at = ?, last_error = NULL, attempts = attempts + 1
             WHERE id = ?",
        )
        .bind(format_time(time::OffsetDateTime::now_utc())?)
        .bind(id.to_string())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    async fn mark_print_job_failed_inner(&self, id: PrintJobId, error: &str) -> StorageResult<()> {
        sqlx::query(
            "UPDATE print_jobs
             SET status = 'failed', last_error = ?, attempts = attempts + 1
             WHERE id = ?",
        )
        .bind(error)
        .bind(id.to_string())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    async fn list_print_jobs_inner(&self, limit: u32) -> StorageResult<Vec<PrintJob>> {
        let rows = sqlx::query("SELECT * FROM print_jobs ORDER BY created_at DESC LIMIT ?")
            .bind(i64::from(limit))
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(row_to_print_job).collect()
    }
}

#[async_trait]
impl PrintQueueRepository for SqliteStore {
    async fn enqueue(&self, job: &PrintJob) -> AppResult<()> {
        Ok(self.enqueue_print_job_inner(job).await?)
    }

    async fn pending(&self, limit: u32) -> AppResult<Vec<PrintJob>> {
        Ok(self.pending_print_jobs_inner(limit).await?)
    }

    async fn mark_printed(&self, id: PrintJobId) -> AppResult<()> {
        Ok(self.mark_print_job_printed_inner(id).await?)
    }

    async fn mark_failed(&self, id: PrintJobId, error: &str) -> AppResult<()> {
        Ok(self.mark_print_job_failed_inner(id, error).await?)
    }

    async fn list_recent(&self, limit: u32) -> AppResult<Vec<PrintJob>> {
        Ok(self.list_print_jobs_inner(limit).await?)
    }
}
