use std::time::{Duration, Instant};

use tpv_application::{PrintJob, TicketDocument, TicketRenderer};
use tpv_domain::Money;
use tpv_domain::ids::SaleId;
use tpv_domain::sale::SaleTotals;
use tpv_print::{EscPosRenderer, build_printer};

fn required_env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => {
            if std::env::var("TPV_HIL_REQUIRED").ok().as_deref() == Some("1") {
                panic!("{name} is required for hardware-in-the-loop tests");
            }
            eprintln!("HIL SKIP: {name} is not set");
            None
        }
    }
}

fn test_job() -> PrintJob {
    let document = TicketDocument {
        store_name: "GENERAL TPV".into(),
        store_address: "Prueba hardware-in-the-loop".into(),
        store_tax_id: "B12345674".into(),
        store_phone: Some("952574066".into()),
        ticket_label: "HIL-000001".into(),
        issued_at: time::OffsetDateTime::now_utc(),
        seller_code: "1".into(),
        seller_name: "HIL".into(),
        customer_name: None,
        customer_tax_id: None,
        lines: vec![tpv_application::TicketLine {
            sku: "HIL".into(),
            description: "Prueba de impresión".into(),
            quantity: "1".into(),
            unit_price: "0.00".into(),
            total: Money::ZERO,
            kind: tpv_domain::sale::LineKind::Sale,
        }],
        totals: SaleTotals {
            gross: Money::ZERO,
            net: Money::ZERO,
            tax: Money::ZERO,
            tax_breakdown: Vec::new(),
            paid: Money::ZERO,
            received: Money::ZERO,
            due: Money::ZERO,
            change: Money::ZERO,
        },
        reprint: false,
        footer: vec!["FIN DE LA PRUEBA".into()],
    };
    let bytes = EscPosRenderer::default()
        .render_ticket(&document)
        .expect("render HIL ticket");
    PrintJob::ticket(
        SaleId::new(),
        document,
        bytes,
        time::OffsetDateTime::now_utc(),
    )
}

async fn print_with_spec(spec: &str) {
    let printer = build_printer(spec).expect("build HIL printer");
    let job = test_job();
    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(30), printer.print(&job))
        .await
        .expect("HIL printer timed out after 30s")
        .expect("HIL print failed");
    eprintln!(
        "HIL OK: {} bytes via {spec} in {:?}",
        job.bytes.len(),
        started.elapsed()
    );
}

#[tokio::test]
#[ignore = "requires a physical network printer in TPV_HIL_NETWORK_ADDR"]
async fn network_printer_hardware_in_the_loop() {
    let Some(address) = required_env("TPV_HIL_NETWORK_ADDR") else {
        return;
    };
    print_with_spec(&format!("network:{address}")).await;
}

#[tokio::test]
#[ignore = "requires a physical serial printer in TPV_HIL_SERIAL_PORT"]
async fn serial_printer_hardware_in_the_loop() {
    let Some(port) = required_env("TPV_HIL_SERIAL_PORT") else {
        return;
    };
    let mut spec = format!("serial:{port}");
    if let Ok(baud) = std::env::var("TPV_HIL_SERIAL_BAUD") {
        if !baud.trim().is_empty() {
            spec.push_str(&format!(";baud={}", baud.trim()));
        }
    }
    print_with_spec(&spec).await;
}

#[tokio::test]
#[ignore = "requires a physical USB printer in TPV_HIL_USB_VID/TPV_HIL_USB_PID"]
async fn usb_printer_hardware_in_the_loop() {
    let Some(vendor) = required_env("TPV_HIL_USB_VID") else {
        return;
    };
    let Some(product) = required_env("TPV_HIL_USB_PID") else {
        return;
    };
    let mut spec = format!("usb:{vendor}:{product}");
    for (variable, option) in [
        ("TPV_HIL_USB_INTERFACE", "interface"),
        ("TPV_HIL_USB_ENDPOINT", "endpoint"),
        ("TPV_HIL_USB_SERIAL", "serial"),
        ("TPV_HIL_USB_PACKET", "packet"),
    ] {
        if let Ok(value) = std::env::var(variable) {
            if !value.trim().is_empty() {
                spec.push_str(&format!(";{option}={}", value.trim()));
            }
        }
    }
    print_with_spec(&spec).await;
}
