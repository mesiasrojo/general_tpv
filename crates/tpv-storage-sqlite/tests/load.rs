use std::sync::Arc;
use std::time::Instant;

use tpv_application::{
    AddLineRequest, ApplyPaymentRequest, CashRepository, CashService, CatalogRepository, Clock,
    ConfirmSaleRequest, OpenCashRequest, PrintQueueRepository, PrintService, SaleRepository,
    SaleService, SequenceRepository, StartSaleRequest, SyncOutboxRepository, SystemClock,
};
use tpv_domain::sale::PaymentMethod;
use tpv_print::{EscPosRenderer, NullPrinter};
use tpv_storage_sqlite::{DatabaseKey, SqliteStore};

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[tokio::test]
#[ignore = "run explicitly with --ignored; configurable with TPV_LOAD_SALES/TPV_LOAD_PRINT_JOBS"]
async fn sale_and_print_throughput() {
    let sales_count = env_u32("TPV_LOAD_SALES", 200);
    let print_count = env_u32("TPV_LOAD_PRINT_JOBS", 200);
    let minimum_sales_per_second = env_f64("TPV_LOAD_MIN_SALES_PER_SEC", 5.0);
    let minimum_prints_per_second = env_f64("TPV_LOAD_MIN_PRINTS_PER_SEC", 20.0);

    let store = Arc::new(
        SqliteStore::in_memory_with_key(&DatabaseKey::generate())
            .await
            .expect("in-memory store"),
    );
    let demo = store.seed_demo().await.expect("demo data");
    let sale_repo: Arc<dyn SaleRepository> = store.clone();
    let catalog_repo: Arc<dyn CatalogRepository> = store.clone();
    let cash_repo: Arc<dyn CashRepository> = store.clone();
    let sequence_repo: Arc<dyn SequenceRepository> = store.clone();
    let sync_repo: Arc<dyn SyncOutboxRepository> = store.clone();
    let print_repo: Arc<dyn PrintQueueRepository> = store.clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let sync = store.sync_context().await.expect("sync context");

    let sales = SaleService::new(
        sale_repo.clone(),
        catalog_repo.clone(),
        clock.clone(),
        sequence_repo,
        "LOAD",
        sync.clone(),
    );
    let cash = CashService::new(cash_repo, sale_repo.clone(), clock.clone(), sync);
    cash.open(OpenCashRequest {
        store_id: demo.store_id,
        register_id: demo.register_id,
        opened_by: demo.user_id,
        opening_float_cents: 10_000,
    })
    .await
    .expect("open cash");
    let pending_before = sync_repo
        .sync_status()
        .await
        .expect("initial sync status")
        .pending;

    let sale_started = Instant::now();
    let mut last_sale_id = None;
    for _ in 0..sales_count {
        let sale = sales
            .start_sale(StartSaleRequest {
                store_id: demo.store_id,
                register_id: demo.register_id,
                seller_id: demo.seller_id,
                customer_id: Some(demo.customer_id),
                price_list_id: None,
            })
            .await
            .expect("start sale");
        let sale = sales
            .add_line(AddLineRequest {
                sale_id: sale.id,
                sku: "1".into(),
                quantity: "2".into(),
                kind: "sale".into(),
                discount_bp: None,
            })
            .await
            .expect("add line");
        let total = sale.totals().expect("totals").gross.cents();
        sales
            .apply_payment(ApplyPaymentRequest {
                sale_id: sale.id,
                method: PaymentMethod::Cash,
                amount_cents: total,
                received_cents: total,
                reference: None,
            })
            .await
            .expect("apply payment");
        sales
            .confirm_sale(ConfirmSaleRequest { sale_id: sale.id })
            .await
            .expect("confirm sale");
        last_sale_id = Some(sale.id);
    }
    let sale_elapsed = sale_started.elapsed();
    let sales_per_second = f64::from(sales_count) / sale_elapsed.as_secs_f64();

    let recent = sale_repo
        .list_recent(demo.store_id, sales_count + 10)
        .await
        .expect("list recent sales");
    assert_eq!(recent.len(), sales_count as usize);
    let sync_status = sync_repo.sync_status().await.expect("sync status");
    assert_eq!(sync_status.pending, pending_before + sales_count);

    let print_service = PrintService::new(
        sale_repo,
        catalog_repo,
        print_repo,
        Arc::new(EscPosRenderer::default()),
        clock,
    );
    let sale_id = last_sale_id.expect("at least one sale");
    let enqueue_started = Instant::now();
    for _ in 0..print_count {
        print_service
            .enqueue_ticket(sale_id, false)
            .await
            .expect("enqueue ticket");
    }
    let enqueue_elapsed = enqueue_started.elapsed();

    let print_started = Instant::now();
    let results = print_service
        .process_pending(&NullPrinter, print_count)
        .await
        .expect("process print queue");
    let print_elapsed = print_started.elapsed();
    let prints_per_second = f64::from(print_count) / print_elapsed.as_secs_f64();
    assert_eq!(results.len(), print_count as usize);
    assert!(results.iter().all(|result| result.success));

    eprintln!(
        "load: {sales_count} sales in {sale_elapsed:?} ({sales_per_second:.1} sales/s); {print_count} print jobs enqueued in {enqueue_elapsed:?}, printed in {print_elapsed:?} ({prints_per_second:.1} prints/s)"
    );
    assert!(
        sales_per_second >= minimum_sales_per_second,
        "sale throughput {sales_per_second:.1}/s is below {minimum_sales_per_second:.1}/s"
    );
    assert!(
        prints_per_second >= minimum_prints_per_second,
        "print throughput {prints_per_second:.1}/s is below {minimum_prints_per_second:.1}/s"
    );
}
