use std::sync::Arc;

use async_trait::async_trait;
use nusb::descriptors::TransferType;
use nusb::transfer::{Bulk, Direction, Out};
use tokio::io::AsyncWriteExt;
use tokio_serial::SerialPortBuilderExt;
use tpv_application::{ApplicationError, PrintJob, PrinterPort, Result as AppResult};

use crate::{DirectoryPrinter, FilePrinter, NetworkPrinter, NullPrinter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialDataBits {
    Five,
    Six,
    Seven,
    Eight,
}

impl SerialDataBits {
    fn to_serial(self) -> tokio_serial::DataBits {
        match self {
            Self::Five => tokio_serial::DataBits::Five,
            Self::Six => tokio_serial::DataBits::Six,
            Self::Seven => tokio_serial::DataBits::Seven,
            Self::Eight => tokio_serial::DataBits::Eight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialParity {
    None,
    Odd,
    Even,
}

impl SerialParity {
    fn to_serial(self) -> tokio_serial::Parity {
        match self {
            Self::None => tokio_serial::Parity::None,
            Self::Odd => tokio_serial::Parity::Odd,
            Self::Even => tokio_serial::Parity::Even,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialStopBits {
    One,
    Two,
}

impl SerialStopBits {
    fn to_serial(self) -> tokio_serial::StopBits {
        match self {
            Self::One => tokio_serial::StopBits::One,
            Self::Two => tokio_serial::StopBits::Two,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialFlowControl {
    None,
    Software,
    Hardware,
}

impl SerialFlowControl {
    fn to_serial(self) -> tokio_serial::FlowControl {
        match self {
            Self::None => tokio_serial::FlowControl::None,
            Self::Software => tokio_serial::FlowControl::Software,
            Self::Hardware => tokio_serial::FlowControl::Hardware,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SerialPrinter {
    port: String,
    baud_rate: u32,
    data_bits: SerialDataBits,
    parity: SerialParity,
    stop_bits: SerialStopBits,
    flow_control: SerialFlowControl,
}

impl SerialPrinter {
    #[must_use]
    pub fn new(port: impl Into<String>) -> Self {
        Self {
            port: port.into(),
            baud_rate: 9_600,
            data_bits: SerialDataBits::Eight,
            parity: SerialParity::None,
            stop_bits: SerialStopBits::One,
            flow_control: SerialFlowControl::None,
        }
    }

    #[must_use]
    pub fn baud_rate(mut self, baud_rate: u32) -> Self {
        self.baud_rate = baud_rate;
        self
    }

    #[must_use]
    pub fn data_bits(mut self, data_bits: SerialDataBits) -> Self {
        self.data_bits = data_bits;
        self
    }

    #[must_use]
    pub fn parity(mut self, parity: SerialParity) -> Self {
        self.parity = parity;
        self
    }

    #[must_use]
    pub fn stop_bits(mut self, stop_bits: SerialStopBits) -> Self {
        self.stop_bits = stop_bits;
        self
    }

    #[must_use]
    pub fn flow_control(mut self, flow_control: SerialFlowControl) -> Self {
        self.flow_control = flow_control;
        self
    }
}

#[async_trait]
impl PrinterPort for SerialPrinter {
    async fn print(&self, job: &PrintJob) -> AppResult<()> {
        let builder = tokio_serial::new(&self.port, self.baud_rate)
            .data_bits(self.data_bits.to_serial())
            .parity(self.parity.to_serial())
            .stop_bits(self.stop_bits.to_serial())
            .flow_control(self.flow_control.to_serial());
        let mut port = builder
            .open_native_async()
            .map_err(ApplicationError::repository)?;
        port.write_all(&job.bytes)
            .await
            .map_err(ApplicationError::repository)?;
        port.flush().await.map_err(ApplicationError::repository)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct UsbPrinter {
    vendor_id: u16,
    product_id: u16,
    interface_number: Option<u8>,
    endpoint_address: Option<u8>,
    serial_number: Option<String>,
    packet_size: Option<usize>,
}

impl UsbPrinter {
    #[must_use]
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        Self {
            vendor_id,
            product_id,
            interface_number: None,
            endpoint_address: None,
            serial_number: None,
            packet_size: None,
        }
    }

    #[must_use]
    pub fn interface_number(mut self, interface_number: u8) -> Self {
        self.interface_number = Some(interface_number);
        self
    }

    #[must_use]
    pub fn endpoint_address(mut self, endpoint_address: u8) -> Self {
        self.endpoint_address = Some(endpoint_address);
        self
    }

    #[must_use]
    pub fn serial_number(mut self, serial_number: impl Into<String>) -> Self {
        self.serial_number = Some(serial_number.into());
        self
    }

    #[must_use]
    pub fn packet_size(mut self, packet_size: usize) -> Self {
        self.packet_size = Some(packet_size.clamp(8, 4096));
        self
    }

    async fn find_device(&self) -> AppResult<nusb::DeviceInfo> {
        let candidates = nusb::list_devices()
            .await
            .map_err(ApplicationError::repository)?
            .filter(|info| {
                info.vendor_id() == self.vendor_id && info.product_id() == self.product_id
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err(ApplicationError::NotFound(format!(
                "USB printer {:04x}:{:04x}",
                self.vendor_id, self.product_id
            )));
        }
        let Some(expected_serial) = &self.serial_number else {
            return Ok(candidates.into_iter().next().expect("non-empty"));
        };
        for info in candidates {
            let serial = match info.serial_number() {
                Some(serial) => Some(serial.to_string()),
                None => info
                    .get_serial_number_string()
                    .await
                    .map_err(ApplicationError::repository)?,
            };
            if serial.as_deref() == Some(expected_serial.as_str()) {
                return Ok(info);
            }
        }
        Err(ApplicationError::NotFound(format!(
            "USB printer {:04x}:{:04x} with serial {expected_serial}",
            self.vendor_id, self.product_id
        )))
    }

    fn select_endpoint(
        &self,
        configuration: &nusb::descriptors::ConfigurationDescriptor<'_>,
    ) -> AppResult<(u8, u8, usize)> {
        for interface in configuration.interfaces() {
            let interface_number = interface.interface_number();
            if self
                .interface_number
                .is_some_and(|expected| expected != interface_number)
            {
                continue;
            }
            for setting in interface.alt_settings() {
                for endpoint in setting.endpoints() {
                    if endpoint.transfer_type() != TransferType::Bulk
                        || endpoint.direction() != Direction::Out
                    {
                        continue;
                    }
                    if self
                        .endpoint_address
                        .is_some_and(|expected| expected != endpoint.address())
                    {
                        continue;
                    }
                    return Ok((
                        interface_number,
                        endpoint.address(),
                        endpoint.max_packet_size(),
                    ));
                }
            }
        }
        Err(ApplicationError::NotFound(
            "USB bulk OUT endpoint for the printer".into(),
        ))
    }
}

#[async_trait]
impl PrinterPort for UsbPrinter {
    async fn print(&self, job: &PrintJob) -> AppResult<()> {
        let info = self.find_device().await?;
        let device = info.open().await.map_err(ApplicationError::repository)?;
        let configuration = device
            .active_configuration()
            .map_err(ApplicationError::repository)?;
        let (interface_number, endpoint_address, max_packet_size) =
            self.select_endpoint(&configuration)?;
        let interface = match device.claim_interface(interface_number).await {
            Ok(interface) => interface,
            Err(claim_error) => device
                .detach_and_claim_interface(interface_number)
                .await
                .map_err(|detach_error| {
                    ApplicationError::repository(format!(
                        "{claim_error}; cannot detach kernel driver: {detach_error}"
                    ))
                })?,
        };
        let packet_size = self.packet_size.unwrap_or(max_packet_size).clamp(8, 4096);
        let mut writer = interface
            .endpoint::<Bulk, Out>(endpoint_address)
            .map_err(ApplicationError::repository)?
            .writer(packet_size)
            .with_num_transfers(4);
        writer
            .write_all(&job.bytes)
            .await
            .map_err(ApplicationError::repository)?;
        writer.flush().await.map_err(ApplicationError::repository)?;
        Ok(())
    }
}

/// Known printer profiles. Width is in characters for the selected font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrinterProfile {
    pub name: &'static str,
    pub width: usize,
    pub baud_rate: u32,
    pub cut: bool,
    pub drawer: bool,
}

#[must_use]
pub fn printer_profile(name: &str) -> Option<PrinterProfile> {
    match name.trim().to_ascii_lowercase().as_str() {
        "generic-58" => Some(PrinterProfile {
            name: "generic-58",
            width: 32,
            baud_rate: 9_600,
            cut: true,
            drawer: true,
        }),
        "generic-80" => Some(PrinterProfile {
            name: "generic-80",
            width: 48,
            baud_rate: 9_600,
            cut: true,
            drawer: true,
        }),
        "epson-tm-t20" => Some(PrinterProfile {
            name: "epson-tm-t20",
            width: 42,
            baud_rate: 9_600,
            cut: true,
            drawer: true,
        }),
        "bixolon-srp-350" => Some(PrinterProfile {
            name: "bixolon-srp-350",
            width: 42,
            baud_rate: 9_600,
            cut: true,
            drawer: true,
        }),
        _ => None,
    }
}

/// Adds the profile's default baud rate to a serial spec that does not set it
/// explicitly. Other printer kinds are returned unchanged.
#[must_use]
pub fn apply_profile_defaults(spec: &str, profile: PrinterProfile) -> String {
    let spec = spec.trim();
    let lower = spec.to_ascii_lowercase();
    if lower.starts_with("serial:") && !lower.contains("baud=") {
        format!("{spec};baud={}", profile.baud_rate)
    } else {
        spec.to_string()
    }
}

/// Builds a printer adapter from a compact specification:
///
/// - `file:/path/ticket.escpos`
/// - `dir:/path/tickets`
/// - `network:192.168.1.50:9100`
/// - `serial:/dev/ttyUSB0;baud=9600;data=8;parity=none;stop=1;flow=none`
/// - `usb:0x04b8:0x0e15;interface=0;endpoint=0x02;packet=512`
/// - `null`
pub fn build_printer(spec: &str) -> AppResult<Arc<dyn PrinterPort>> {
    let spec = spec.trim();
    if spec.eq_ignore_ascii_case("null") {
        return Ok(Arc::new(NullPrinter));
    }
    let (kind, rest) = spec.split_once(':').ok_or_else(|| {
        ApplicationError::Validation(
            "printer spec must be file:..., dir:..., network:..., serial:... or usb:...".into(),
        )
    })?;
    if rest.trim().is_empty() {
        return Err(ApplicationError::Validation(format!(
            "printer spec {kind}: requires a value"
        )));
    }
    match kind.trim().to_ascii_lowercase().as_str() {
        "file" => Ok(Arc::new(FilePrinter::new(rest))),
        "dir" => Ok(Arc::new(DirectoryPrinter::new(rest))),
        "network" => Ok(Arc::new(NetworkPrinter::new(rest))),
        "serial" => Ok(Arc::new(parse_serial(rest)?)),
        "usb" => Ok(Arc::new(parse_usb(rest)?)),
        other => Err(ApplicationError::Validation(format!(
            "unknown printer type: {other}"
        ))),
    }
}

fn parse_serial(input: &str) -> AppResult<SerialPrinter> {
    let (port, options) = split_options(input);
    if port.is_empty() {
        return Err(ApplicationError::Validation(
            "serial printer requires a port".into(),
        ));
    }
    let mut printer = SerialPrinter::new(port);
    for (key, value) in options {
        printer = match key {
            "baud" => printer.baud_rate(parse_u32(value, "baud")?),
            "data" => printer.data_bits(match value {
                "5" => SerialDataBits::Five,
                "6" => SerialDataBits::Six,
                "7" => SerialDataBits::Seven,
                "8" => SerialDataBits::Eight,
                other => {
                    return Err(ApplicationError::Validation(format!(
                        "invalid serial data bits: {other}"
                    )));
                }
            }),
            "parity" => printer.parity(match value.to_ascii_lowercase().as_str() {
                "none" => SerialParity::None,
                "odd" => SerialParity::Odd,
                "even" => SerialParity::Even,
                other => {
                    return Err(ApplicationError::Validation(format!(
                        "invalid serial parity: {other}"
                    )));
                }
            }),
            "stop" => printer.stop_bits(match value {
                "1" => SerialStopBits::One,
                "2" => SerialStopBits::Two,
                other => {
                    return Err(ApplicationError::Validation(format!(
                        "invalid serial stop bits: {other}"
                    )));
                }
            }),
            "flow" => printer.flow_control(match value.to_ascii_lowercase().as_str() {
                "none" => SerialFlowControl::None,
                "software" => SerialFlowControl::Software,
                "hardware" => SerialFlowControl::Hardware,
                other => {
                    return Err(ApplicationError::Validation(format!(
                        "invalid serial flow control: {other}"
                    )));
                }
            }),
            other => {
                return Err(ApplicationError::Validation(format!(
                    "unknown serial option: {other}"
                )));
            }
        };
    }
    Ok(printer)
}

fn parse_usb(input: &str) -> AppResult<UsbPrinter> {
    let (ids, options) = split_options(input);
    let (vendor, product) = ids.split_once(':').ok_or_else(|| {
        ApplicationError::Validation("usb printer requires vendor:product ids".into())
    })?;
    let mut printer = UsbPrinter::new(
        parse_hex_u16(vendor, "vendor id")?,
        parse_hex_u16(product, "product id")?,
    );
    for (key, value) in options {
        printer = match key {
            "interface" => printer.interface_number(parse_u8(value, "interface")?),
            "endpoint" => printer.endpoint_address(parse_hex_u8(value)?),
            "serial" => printer.serial_number(value),
            "packet" => printer.packet_size(parse_usize(value, "packet size")?),
            other => {
                return Err(ApplicationError::Validation(format!(
                    "unknown usb option: {other}"
                )));
            }
        };
    }
    Ok(printer)
}

fn split_options(input: &str) -> (&str, Vec<(&str, &str)>) {
    let (head, tail) = input.split_once(';').unwrap_or((input, ""));
    let mut options = Vec::new();
    for item in tail.split(';').filter(|item| !item.trim().is_empty()) {
        if let Some((key, value)) = item.split_once('=') {
            options.push((key.trim(), value.trim()));
        }
    }
    (head.trim(), options)
}

fn parse_u32(value: &str, field: &str) -> AppResult<u32> {
    value
        .parse()
        .map_err(|_| ApplicationError::Validation(format!("invalid {field}: {value}")))
}

fn parse_usize(value: &str, field: &str) -> AppResult<usize> {
    value
        .parse()
        .map_err(|_| ApplicationError::Validation(format!("invalid {field}: {value}")))
}

fn parse_u8(value: &str, field: &str) -> AppResult<u8> {
    value
        .parse()
        .map_err(|_| ApplicationError::Validation(format!("invalid {field}: {value}")))
}

fn parse_hex_u16(value: &str, field: &str) -> AppResult<u16> {
    let value = value.trim();
    let number = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u16::from_str_radix(hex, 16)
    } else {
        value.parse()
    };
    number.map_err(|_| ApplicationError::Validation(format!("invalid {field}: {value}")))
}

fn parse_hex_u8(value: &str) -> AppResult<u8> {
    let value = value.trim();
    let number = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u8::from_str_radix(hex, 16)
    } else {
        value.parse()
    };
    number.map_err(|_| ApplicationError::Validation(format!("invalid endpoint: {value}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_serial_specification() {
        let printer =
            parse_serial("/dev/ttyUSB0;baud=19200;data=7;parity=even;stop=2;flow=software")
                .unwrap();
        let debug = format!("{printer:?}");
        assert!(debug.contains("19200"));
        assert!(debug.contains("Seven"));
        assert!(debug.contains("Even"));
        assert!(debug.contains("Two"));
        assert!(debug.contains("Software"));
    }

    #[test]
    fn parses_usb_specification() {
        let printer =
            parse_usb("0x04b8:0x0e15;interface=1;endpoint=0x02;serial=ABC;packet=512").unwrap();
        let debug = format!("{printer:?}");
        assert!(debug.contains("1208"));
        assert!(debug.contains("3605"));
        assert!(debug.contains("ABC"));
        assert!(debug.contains("512"));
    }

    #[test]
    fn rejects_invalid_printer_specs() {
        assert!(build_printer("serial:").is_err());
        assert!(build_printer("serial:/dev/ttyUSB0;baud=fast").is_err());
        assert!(build_printer("serial:/dev/ttyUSB0;wat=1").is_err());
        assert!(build_printer("usb:zzzz:0x01").is_err());
        assert!(build_printer("usb:0x01:0x02;endpoint=0x100").is_err());
        assert!(build_printer("unknown:value").is_err());
        assert!(build_printer("no-colon").is_err());
    }

    #[test]
    fn exposes_known_printer_profiles() {
        let profile = printer_profile("generic-80").unwrap();
        assert_eq!(profile.width, 48);
        assert_eq!(profile.baud_rate, 9_600);
        assert!(profile.cut);
        assert!(printer_profile("epson-tm-t20").is_some());
        assert!(printer_profile("unknown").is_none());
    }

    #[test]
    fn profile_adds_the_default_baud_rate_only_when_needed() {
        let profile = printer_profile("generic-80").unwrap();
        assert_eq!(
            apply_profile_defaults("serial:/dev/ttyUSB0", profile),
            "serial:/dev/ttyUSB0;baud=9600"
        );
        assert_eq!(
            apply_profile_defaults("serial:/dev/ttyUSB0;baud=19200", profile),
            "serial:/dev/ttyUSB0;baud=19200"
        );
        assert_eq!(
            apply_profile_defaults("network:127.0.0.1:9100", profile),
            "network:127.0.0.1:9100"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn serial_printer_writes_bytes_to_a_pty() {
        use std::io::Read;
        use std::time::Duration;

        use serialport::SerialPort;

        let (mut master, slave) = serialport::TTYPort::pair().unwrap();
        master.set_timeout(Duration::from_secs(2)).unwrap();
        let name = slave.name().unwrap();
        let _slave = slave;

        let bytes = vec![0x1B, 0x40, b'T', b'P', b'V', b'\n'];
        let document = tpv_application::TicketDocument {
            store_name: String::new(),
            store_address: String::new(),
            store_tax_id: String::new(),
            store_phone: None,
            ticket_label: String::new(),
            issued_at: time::OffsetDateTime::now_utc(),
            seller_code: String::new(),
            seller_name: String::new(),
            customer_name: None,
            customer_tax_id: None,
            lines: Vec::new(),
            totals: tpv_domain::sale::SaleTotals {
                gross: tpv_domain::Money::ZERO,
                net: tpv_domain::Money::ZERO,
                tax: tpv_domain::Money::ZERO,
                tax_breakdown: Vec::new(),
                paid: tpv_domain::Money::ZERO,
                received: tpv_domain::Money::ZERO,
                due: tpv_domain::Money::ZERO,
                change: tpv_domain::Money::ZERO,
            },
            reprint: false,
            footer: Vec::new(),
        };
        let job = PrintJob::ticket(
            tpv_domain::ids::SaleId::new(),
            document,
            bytes.clone(),
            time::OffsetDateTime::now_utc(),
        );
        let expected_len = bytes.len();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let reader = tokio::task::spawn_blocking(move || {
            let mut received = vec![0_u8; expected_len];
            master.read_exact(&mut received).unwrap();
            let _ = sender.send(received);
        });
        SerialPrinter::new(name).print(&job).await.unwrap();
        let received = receiver.await.unwrap();
        reader.await.unwrap();
        assert_eq!(received, bytes);
    }
}
