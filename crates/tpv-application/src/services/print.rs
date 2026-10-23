use std::sync::Arc;

use tpv_domain::ids::{PrintJobId, SaleId};

use crate::dto::{PrintJob, PrintResult, TicketDocument};
use crate::error::{ApplicationError, Result};
use crate::ports::{
    CatalogRepository, Clock, PrintQueueRepository, PrinterPort, SaleRepository, TicketRenderer,
};

#[derive(Clone)]
pub struct PrintService {
    sale_repo: Arc<dyn SaleRepository>,
    catalog_repo: Arc<dyn CatalogRepository>,
    print_repo: Arc<dyn PrintQueueRepository>,
    renderer: Arc<dyn TicketRenderer>,
    clock: Arc<dyn Clock>,
}

impl PrintService {
    pub fn new(
        sale_repo: Arc<dyn SaleRepository>,
        catalog_repo: Arc<dyn CatalogRepository>,
        print_repo: Arc<dyn PrintQueueRepository>,
        renderer: Arc<dyn TicketRenderer>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            sale_repo,
            catalog_repo,
            print_repo,
            renderer,
            clock,
        }
    }

    pub async fn enqueue_ticket(&self, sale_id: SaleId, reprint: bool) -> Result<PrintJob> {
        let sale = self
            .sale_repo
            .find(sale_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("sale {sale_id}")))?;
        let store = self
            .catalog_repo
            .find_store(sale.store_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("store {}", sale.store_id)))?;
        let seller = self
            .catalog_repo
            .find_seller(sale.seller_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("seller {}", sale.seller_id)))?;
        let customer = if let Some(customer_id) = sale.customer_id {
            self.catalog_repo.find_customer(customer_id).await?
        } else {
            None
        };
        let document =
            TicketDocument::from_sale(&sale, &store, &seller, customer.as_ref(), reprint)?;
        let bytes = self.renderer.render_ticket(&document)?;
        let job = PrintJob::ticket(sale.id, document, bytes, self.clock.now());
        self.print_repo.enqueue(&job).await?;
        Ok(job)
    }

    pub async fn process_pending(
        &self,
        printer: &dyn PrinterPort,
        limit: u32,
    ) -> Result<Vec<PrintResult>> {
        let jobs = self.print_repo.pending(limit).await?;
        let mut results = Vec::with_capacity(jobs.len());
        for job in jobs {
            match printer.print(&job).await {
                Ok(()) => {
                    self.print_repo.mark_printed(job.id).await?;
                    results.push(PrintResult {
                        job_id: job.id,
                        success: true,
                        error: None,
                    });
                }
                Err(error) => {
                    let message = error.to_string();
                    self.print_repo.mark_failed(job.id, &message).await?;
                    results.push(PrintResult {
                        job_id: job.id,
                        success: false,
                        error: Some(message),
                    });
                }
            }
        }
        Ok(results)
    }

    pub async fn list_recent(&self, limit: u32) -> Result<Vec<PrintJob>> {
        self.print_repo.list_recent(limit).await
    }

    pub async fn get_job(&self, _id: PrintJobId) -> Result<Option<PrintJob>> {
        // The first slice reads through the pending/recent list. A dedicated
        // lookup can be added to the port when the print console needs it.
        Ok(None)
    }
}
