use crate::{
    driver_catalog,
    mock_device::MockDevice,
    protocol::{parse_device_frame, DeviceFrame},
    version,
};

#[derive(Debug, Clone)]
pub struct SerialPortEntry {
    pub name: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HelloInfo {
    pub fw_version: String,
    pub protocol_version: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ChipId {
    pub manufacturer_id: u8,
    pub device_id: u8,
    pub name: String,
    pub size_bytes: u32,
    pub sector_size: u32,
    pub driver_id: String,
}

impl ChipId {
    pub fn from_ids(mfr: u8, dev: u8) -> Option<ChipId> {
        if let Some(chip) = driver_catalog::lookup_chip(mfr, dev) {
            return Some(chip);
        }
        match (mfr, dev) {
            (0xBF, 0xB5) => Some(ChipId {
                manufacturer_id: mfr,
                device_id: dev,
                name: "SST39SF010A".to_string(),
                size_bytes: 128 * 1024,
                sector_size: 4096,
                driver_id: "sst39-core".to_string(),
            }),
            (0xBF, 0xB6) => Some(ChipId {
                manufacturer_id: mfr,
                device_id: dev,
                name: "SST39SF020A".to_string(),
                size_bytes: 256 * 1024,
                sector_size: 4096,
                driver_id: "sst39-core".to_string(),
            }),
            (0xBF, 0xB7) => Some(ChipId {
                manufacturer_id: mfr,
                device_id: dev,
                name: "SST39SF040".to_string(),
                size_bytes: 512 * 1024,
                sector_size: 4096,
                driver_id: "sst39-core".to_string(),
            }),
            _ => None,
        }
    }

    pub fn sector_count(&self) -> u32 {
        self.size_bytes / self.sector_size
    }
}

#[derive(Debug)]
pub enum SessionError {
    Protocol(String),
    ChipUnknown(u8, u8),
    Io(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::Protocol(s) => write!(f, "protocol error: {s}"),
            SessionError::ChipUnknown(m, d) => {
                write!(f, "unknown chip MFR=0x{m:02X} DEV=0x{d:02X}")
            }
            SessionError::Io(s) => write!(f, "io error: {s}"),
        }
    }
}

pub fn list_serial_ports() -> Result<Vec<SerialPortEntry>, SessionError> {
    let ports = serialport::available_ports()
        .map_err(|e| SessionError::Io(format!("scan ports failed: {e}")))?;

    let mut out = Vec::with_capacity(ports.len());
    for p in ports {
        let description = match p.port_type {
            serialport::SerialPortType::UsbPort(usb) => format!(
                "USB VID:PID={:04X}:{:04X} {} {}",
                usb.vid,
                usb.pid,
                usb.manufacturer.unwrap_or_default(),
                usb.product.unwrap_or_default()
            )
            .trim()
            .to_string(),
            serialport::SerialPortType::BluetoothPort => "Bluetooth".to_string(),
            serialport::SerialPortType::PciPort => "PCI".to_string(),
            serialport::SerialPortType::Unknown => "Unknown".to_string(),
        };

        out.push(SerialPortEntry {
            name: p.port_name,
            description,
        });
    }
    Ok(out)
}

pub fn open_serial_port(
    port_name: &str,
    baud_rate: u32,
    timeout_ms: u64,
) -> Result<Box<dyn serialport::SerialPort>, SessionError> {
    serialport::new(port_name, baud_rate)
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .open()
        .map_err(|e| SessionError::Io(format!("open {port_name} failed: {e}")))
}

// ---------------------------------------------------------------------------
// Session trait
// ---------------------------------------------------------------------------

pub trait DeviceSession {
    fn handshake(&mut self) -> Result<HelloInfo, SessionError>;
    fn identify(&mut self) -> Result<ChipId, SessionError>;
    /// Read `len` bytes starting at `addr`. `on_progress(done, total)` is
    /// called after each received chunk.
    fn read_range(
        &mut self,
        addr: u32,
        len: u32,
        on_progress: &mut dyn FnMut(u32, u32),
    ) -> Result<Vec<u8>, SessionError>;
}

pub fn parse_id_detail(detail: &str) -> (Option<u8>, Option<u8>) {
    let mut mfr = None;
    let mut dev = None;
    for kv in detail.split(',') {
        let mut parts = kv.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim().to_lowercase();
        let val = parts
            .next()
            .unwrap_or("")
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        if let Ok(v) = u8::from_str_radix(val, 16) {
            match key.as_str() {
                "mf" | "manufacturer" => mfr = Some(v),
                "dev" | "device" => dev = Some(v),
                _ => {}
            }
        }
    }
    (mfr, dev)
}

// ---------------------------------------------------------------------------
// Mock session (backed by MockDevice)
// ---------------------------------------------------------------------------

pub struct MockSession {
    device: MockDevice,
}

impl MockSession {
    pub fn new() -> Self {
        MockSession {
            device: MockDevice::new(),
        }
    }

    fn send(&self, cmd: &str) -> Vec<String> {
        self.device.handle(cmd)
    }
}

impl DeviceSession for MockSession {
    fn handshake(&mut self) -> Result<HelloInfo, SessionError> {
        let lines = self.send("HELLO");
        for line in &lines {
            if let Ok(DeviceFrame::Hello {
                fw_version,
                protocol_version,
                capabilities,
            }) = parse_device_frame(line)
            {
                if !version::is_protocol_compatible(&protocol_version) {
                    return Err(SessionError::Protocol(format!(
                        "unsupported protocol version: got {}, minimum {}",
                        protocol_version,
                        version::supported_protocol_version()
                    )));
                }
                return Ok(HelloInfo {
                    fw_version,
                    protocol_version,
                    capabilities: capabilities.split(',').map(String::from).collect(),
                });
            }
        }
        Err(SessionError::Protocol("no HELLO frame received".to_string()))
    }

    fn identify(&mut self) -> Result<ChipId, SessionError> {
        let lines = self.send("ID");
        for line in &lines {
            if let Ok(DeviceFrame::Ok { command: _, detail }) = parse_device_frame(line) {
                let (mfr, dev) = parse_id_detail(&detail);
                let mfr = mfr.unwrap_or(0);
                let dev = dev.unwrap_or(0);
                return ChipId::from_ids(mfr, dev).ok_or(SessionError::ChipUnknown(mfr, dev));
            }
        }
        Err(SessionError::Protocol("no ID response".to_string()))
    }

    fn read_range(
        &mut self,
        addr: u32,
        len: u32,
        on_progress: &mut dyn FnMut(u32, u32),
    ) -> Result<Vec<u8>, SessionError> {
        let cmd = format!("READ 0x{addr:05X} {len}");
        let lines = self.send(&cmd);
        let mut data: Vec<u8> = Vec::with_capacity(len as usize);

        for line in &lines {
            match parse_device_frame(line) {
                Ok(DeviceFrame::DataHex {
                    address: _,
                    len: _,
                    data: bytes,
                }) => {
                    data.extend_from_slice(&bytes);
                    on_progress(data.len() as u32, len);
                }
                Ok(DeviceFrame::Ok { .. }) => {}
                Ok(DeviceFrame::Err { code, message }) => {
                    return Err(SessionError::Protocol(format!("{code}: {message}")));
                }
                _ => {}
            }
        }

        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_id_detail;

    #[test]
    fn parse_id_detail_accepts_short_keys() {
        let (mfr, dev) = parse_id_detail("mf=0xBF,dev=0xB7");
        assert_eq!(mfr, Some(0xBF));
        assert_eq!(dev, Some(0xB7));
    }

    #[test]
    fn parse_id_detail_accepts_legacy_keys() {
        let (mfr, dev) = parse_id_detail("manufacturer=0xDA,device=0xC1");
        assert_eq!(mfr, Some(0xDA));
        assert_eq!(dev, Some(0xC1));
    }
}
