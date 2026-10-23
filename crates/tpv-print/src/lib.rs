//! ESC/POS ticket rendering and printer adapters.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use async_trait::async_trait;
use encoding_rs::WINDOWS_1252;
use tpv_application::{
    ApplicationError, PrintJob, PrinterPort, Result as AppResult, TicketDocument, TicketRenderer,
};

const ESC_INIT: [u8; 2] = [0x1B, 0x40];
const ESC_ALIGN_LEFT: [u8; 3] = [0x1B, 0x61, 0x00];
const ESC_ALIGN_CENTER: [u8; 3] = [0x1B, 0x61, 0x01];
const ESC_BOLD_ON: [u8; 3] = [0x1B, 0x45, 0x01];
const ESC_BOLD_OFF: [u8; 3] = [0x1B, 0x45, 0x00];
const ESC_CODE_PAGE_WPC1252: [u8; 3] = [0x1B, 0x74, 0x10];
const GS_DOUBLE_ON: [u8; 3] = [0x1D, 0x21, 0x11];
const GS_DOUBLE_OFF: [u8; 3] = [0x1D, 0x21, 0x00];
const ESC_FEED_4: [u8; 3] = [0x1B, 0x64, 0x04];
const GS_CUT: [u8; 4] = [0x1D, 0x56, 0x42, 0x00];
const ESC_DRAWER_PULSE: [u8; 5] = [0x1B, 0x70, 0x00, 0x19, 0xFA];

#[derive(Debug, Clone)]
pub struct EscPosRenderer {
    width: usize,
    open_drawer: bool,
    cut: bool,
}

impl Default for EscPosRenderer {
    fn default() -> Self {
        Self {
            width: 42,
            open_drawer: true,
            cut: true,
        }
    }
}

impl EscPosRenderer {
    #[must_use]
    pub fn new(width: usize, open_drawer: bool, cut: bool) -> Self {
        Self {
            width: width.clamp(24, 64),
            open_drawer,
            cut,
        }
    }

    fn text(&self, out: &mut Vec<u8>, value: &str) -> AppResult<()> {
        let (bytes, _, had_errors) = WINDOWS_1252.encode(value);
        if had_errors {
            return Err(ApplicationError::Validation(format!(
                "text cannot be encoded as Windows-1252: {value}"
            )));
        }
        out.extend_from_slice(&bytes);
        Ok(())
    }

    fn line(&self, out: &mut Vec<u8>, value: &str) -> AppResult<()> {
        self.text(out, value)?;
        out.push(b'\n');
        Ok(())
    }

    fn center(&self, out: &mut Vec<u8>, value: &str) -> AppResult<()> {
        let length = value.chars().count();
        let padding = self.width.saturating_sub(length) / 2;
        self.line(out, &format!("{}{}", " ".repeat(padding), value))
    }

    fn two_columns(&self, out: &mut Vec<u8>, left: &str, right: &str) -> AppResult<()> {
        let right_length = right.chars().count();
        if right_length >= self.width {
            self.line(out, right)?;
            return Ok(());
        }
        let left_limit = self.width - right_length - 1;
        let left = if left.chars().count() > left_limit {
            left.chars().take(left_limit).collect::<String>()
        } else {
            left.to_string()
        };
        let padding = self.width - left.chars().count() - right_length;
        self.line(out, &format!("{left}{}{right}", " ".repeat(padding)))
    }

    fn divider(&self, out: &mut Vec<u8>) -> AppResult<()> {
        self.line(out, &"-".repeat(self.width))
    }
}

impl TicketRenderer for EscPosRenderer {
    fn render_ticket(&self, document: &TicketDocument) -> AppResult<Vec<u8>> {
        let mut out = Vec::with_capacity(2048);
        out.extend_from_slice(&ESC_INIT);
        out.extend_from_slice(&ESC_CODE_PAGE_WPC1252);

        out.extend_from_slice(&ESC_ALIGN_CENTER);
        out.extend_from_slice(&ESC_BOLD_ON);
        self.center(&mut out, &document.store_name)?;
        out.extend_from_slice(&ESC_BOLD_OFF);
        self.center(&mut out, &document.store_address)?;
        self.center(
            &mut out,
            &format!(
                "NIF: {}  Tel: {}",
                document.store_tax_id,
                document.store_phone.as_deref().unwrap_or("-")
            ),
        )?;
        self.divider(&mut out)?;

        out.extend_from_slice(&ESC_ALIGN_LEFT);
        if document.reprint {
            out.extend_from_slice(&ESC_BOLD_ON);
            self.center(&mut out, "COPIA")?;
            out.extend_from_slice(&ESC_BOLD_OFF);
        }
        self.two_columns(&mut out, &format!("Ticket: {}", document.ticket_label), "")?;
        self.two_columns(
            &mut out,
            "Fecha:",
            &document
                .issued_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        )?;
        self.two_columns(
            &mut out,
            &format!(
                "Vendedor: {} - {}",
                document.seller_code, document.seller_name
            ),
            "",
        )?;
        if let Some(customer) = &document.customer_name {
            self.two_columns(&mut out, &format!("Cliente: {customer}"), "")?;
            if let Some(nif) = &document.customer_tax_id {
                self.two_columns(&mut out, &format!("NIF: {nif}"), "")?;
            }
        }
        self.divider(&mut out)?;

        self.line(&mut out, "CANT  DESCRIPCION                 IMPORTE")?;
        for line in &document.lines {
            let description = format!("{} x {}", line.quantity, line.description);
            let total = line.total.format_eur();
            self.two_columns(&mut out, &description, &total)?;
            if line.unit_price != "0" {
                self.two_columns(&mut out, &format!("      {} €/ud", line.unit_price), "")?;
            }
        }
        self.divider(&mut out)?;

        for breakdown in &document.totals.tax_breakdown {
            self.two_columns(
                &mut out,
                &format!(
                    "IVA {:.2}%  base {}",
                    f64::from(breakdown.percent_bp) / 100.0,
                    breakdown.net.format_eur()
                ),
                &breakdown.tax.format_eur(),
            )?;
        }
        out.extend_from_slice(&GS_DOUBLE_ON);
        self.two_columns(&mut out, "TOTAL", &document.totals.gross.format_eur())?;
        out.extend_from_slice(&GS_DOUBLE_OFF);
        self.two_columns(
            &mut out,
            "Entregado",
            &document.totals.received.format_eur(),
        )?;
        self.two_columns(&mut out, "Cambio", &document.totals.change.format_eur())?;
        self.divider(&mut out)?;

        out.extend_from_slice(&ESC_ALIGN_CENTER);
        for footer in &document.footer {
            self.center(&mut out, footer)?;
        }
        out.extend_from_slice(&ESC_FEED_4);
        if self.open_drawer {
            out.extend_from_slice(&ESC_DRAWER_PULSE);
        }
        if self.cut {
            out.extend_from_slice(&GS_CUT);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct FilePrinter {
    path: PathBuf,
}

impl FilePrinter {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl PrinterPort for FilePrinter {
    async fn print(&self, job: &PrintJob) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(ApplicationError::repository)?;
        }
        tokio::fs::write(&self.path, &job.bytes)
            .await
            .map_err(ApplicationError::repository)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct NetworkPrinter {
    address: String,
}

impl NetworkPrinter {
    #[must_use]
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
        }
    }
}

#[async_trait]
impl PrinterPort for NetworkPrinter {
    async fn print(&self, job: &PrintJob) -> AppResult<()> {
        use tokio::io::AsyncWriteExt;

        let mut stream = tokio::net::TcpStream::connect(&self.address)
            .await
            .map_err(ApplicationError::repository)?;
        stream
            .write_all(&job.bytes)
            .await
            .map_err(ApplicationError::repository)?;
        stream.flush().await.map_err(ApplicationError::repository)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DirectoryPrinter {
    directory: PathBuf,
}

impl DirectoryPrinter {
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
}

#[async_trait]
impl PrinterPort for DirectoryPrinter {
    async fn print(&self, job: &PrintJob) -> AppResult<()> {
        tokio::fs::create_dir_all(&self.directory)
            .await
            .map_err(ApplicationError::repository)?;
        let path = self.directory.join(format!("{}.escpos", job.id));
        tokio::fs::write(path, &job.bytes)
            .await
            .map_err(ApplicationError::repository)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct NullPrinter;

#[async_trait]
impl PrinterPort for NullPrinter {
    async fn print(&self, _job: &PrintJob) -> AppResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpv_application::{PrintJob, TicketDocument, TicketLine};
    use tpv_domain::ids::SaleId;
    use tpv_domain::money::Money;
    use tpv_domain::sale::{LineKind, SaleTotals};
    use tpv_domain::tax::TaxBreakdown;

    fn document(reprint: bool) -> TicketDocument {
        let gross = Money::from_cents(140);
        let tax = Money::from_cents(14);
        TicketDocument {
            store_name: "100%SHOPPING".into(),
            store_address: "Paseo Marítimo 1, Benalmádena".into(),
            store_tax_id: "B12345674".into(),
            store_phone: Some("952574066".into()),
            ticket_label: "TPV-000001".into(),
            issued_at: time::macros::datetime!(2026-10-22 19:43:06 UTC),
            seller_code: "1".into(),
            seller_name: "Rafi".into(),
            customer_name: Some("Ana García".into()),
            customer_tax_id: Some("12345678Z".into()),
            lines: vec![TicketLine {
                sku: "1".into(),
                description: "Bolsa plástico".into(),
                quantity: "2".into(),
                unit_price: "0.10".into(),
                total: Money::from_cents(20),
                kind: LineKind::Sale,
            }],
            totals: SaleTotals {
                gross,
                net: Money::from_cents(126),
                tax,
                tax_breakdown: vec![TaxBreakdown {
                    code: "IVA21".into(),
                    percent_bp: 2100,
                    gross,
                    net: Money::from_cents(126),
                    tax,
                }],
                paid: gross,
                received: Money::from_cents(200),
                due: Money::ZERO,
                change: Money::from_cents(60),
            },
            reprint,
            footer: vec!["IVA INCLUIDO".into(), "GRACIAS POR SU COMPRA".into()],
        }
    }

    #[test]
    fn renders_escpos_with_ticket_content_and_commands() {
        let renderer = EscPosRenderer::default();
        let bytes = renderer.render_ticket(&document(false)).unwrap();
        let text = WINDOWS_1252.decode(&bytes).0;
        assert!(text.contains("100%SHOPPING"));
        assert!(text.contains("TPV-000001"));
        assert!(text.contains("Bolsa plástico"));
        assert!(text.contains("TOTAL"));
        assert!(bytes.windows(2).any(|window| window == ESC_INIT.as_slice()));
        assert!(
            bytes
                .windows(5)
                .any(|window| window == ESC_DRAWER_PULSE.as_slice())
        );
        assert!(bytes.windows(4).any(|window| window == GS_CUT.as_slice()));
    }

    #[test]
    fn reprint_is_marked() {
        let renderer = EscPosRenderer::default();
        let bytes = renderer.render_ticket(&document(true)).unwrap();
        let text = WINDOWS_1252.decode(&bytes).0;
        assert!(text.contains("COPIA"));
    }

    #[tokio::test]
    async fn file_printer_writes_bytes() {
        let path = std::env::temp_dir().join(format!("tpv-ticket-{}.escpos", std::process::id()));
        let renderer = EscPosRenderer::default();
        let document = document(false);
        let bytes = renderer.render_ticket(&document).unwrap();
        let job = PrintJob::ticket(
            SaleId::new(),
            document,
            bytes.clone(),
            time::OffsetDateTime::now_utc(),
        );
        FilePrinter::new(&path).print(&job).await.unwrap();
        let written = tokio::fs::read(&path).await.unwrap();
        assert_eq!(written, bytes);
        let _ = tokio::fs::remove_file(path).await;
    }
}
