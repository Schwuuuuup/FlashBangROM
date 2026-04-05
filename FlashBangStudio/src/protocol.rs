#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceFrame {
    Hello {
        fw_version: String,
        protocol_version: String,
        capabilities: String,
    },
    Ok {
        command: String,
        detail: String,
    },
    Err {
        code: String,
        message: String,
    },
    DataHex {
        address: u32,
        len: usize,
        data: Vec<u8>,
    },
    Status {
        operation: String,
        phase: String,
        progress: u32,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    InvalidFormat,
    InvalidNumber,
    InvalidHexPayload,
    LengthMismatch,
}

fn parse_hex_u32(s: &str) -> Result<u32, ParseError> {
    u32::from_str_radix(s, 16).map_err(|_| ParseError::InvalidNumber)
}

fn parse_dec_u32(s: &str) -> Result<u32, ParseError> {
    s.parse::<u32>().map_err(|_| ParseError::InvalidNumber)
}

pub fn decode_hex_payload(hex: &str) -> Result<Vec<u8>, ParseError> {
    if (hex.len() % 2) != 0 {
        return Err(ParseError::InvalidHexPayload);
    }

    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = bytes[i] as char;
        let lo = bytes[i + 1] as char;
        let h = hi.to_digit(16).ok_or(ParseError::InvalidHexPayload)?;
        let l = lo.to_digit(16).ok_or(ParseError::InvalidHexPayload)?;
        out.push(((h << 4) | l) as u8);
        i += 2;
    }
    Ok(out)
}

pub fn parse_device_frame(line: &str) -> Result<DeviceFrame, ParseError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(ParseError::Empty);
    }

    if let Some(rest) = line.strip_prefix("HELLO|") {
        let parts: Vec<&str> = rest.split('|').collect();
        if parts.len() < 3 {
            return Err(ParseError::InvalidFormat);
        }
        return Ok(DeviceFrame::Hello {
            fw_version: parts[0].to_string(),
            protocol_version: parts[1].to_string(),
            capabilities: parts[2..].join("|"),
        });
    }

    if let Some(rest) = line.strip_prefix("OK|") {
        let parts: Vec<&str> = rest.split('|').collect();
        if parts.len() < 2 {
            return Err(ParseError::InvalidFormat);
        }
        return Ok(DeviceFrame::Ok {
            command: parts[0].to_string(),
            detail: parts[1..].join("|"),
        });
    }

    if let Some(rest) = line.strip_prefix("ERR|") {
        let parts: Vec<&str> = rest.split('|').collect();
        if parts.len() < 2 {
            return Err(ParseError::InvalidFormat);
        }
        return Ok(DeviceFrame::Err {
            code: parts[0].to_string(),
            message: parts[1..].join("|"),
        });
    }

    if let Some(rest) = line.strip_prefix("DATA|") {
        let parts: Vec<&str> = rest.split('|').collect();
        if parts.len() != 3 {
            return Err(ParseError::InvalidFormat);
        }
        let address = parse_hex_u32(parts[0])?;
        let len = parse_dec_u32(parts[1])? as usize;
        let data = decode_hex_payload(parts[2])?;
        if data.len() != len {
            return Err(ParseError::LengthMismatch);
        }
        return Ok(DeviceFrame::DataHex { address, len, data });
    }

    if let Some(rest) = line.strip_prefix("STATUS|") {
        let parts: Vec<&str> = rest.split('|').collect();
        if parts.len() < 4 {
            return Err(ParseError::InvalidFormat);
        }
        return Ok(DeviceFrame::Status {
            operation: parts[0].to_string(),
            phase: parts[1].to_string(),
            progress: parse_dec_u32(parts[2])?,
            detail: parts[3..].join("|"),
        });
    }

    Err(ParseError::InvalidFormat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hello_frame() {
        let f = parse_device_frame("HELLO|flashbang-fw-dev-0.5.0|0.5.0|driver-upload")
            .expect("hello should parse");
        match f {
            DeviceFrame::Hello {
                fw_version,
                protocol_version,
                capabilities,
            } => {
                assert_eq!(fw_version, "flashbang-fw-dev-0.5.0");
                assert_eq!(protocol_version, "0.5.0");
                assert_eq!(capabilities, "driver-upload");
            }
            _ => panic!("unexpected frame"),
        }
    }

    #[test]
    fn parses_data_hex_frame() {
        let f = parse_device_frame("DATA|00010|4|DEADBEEF").expect("data frame should parse");
        match f {
            DeviceFrame::DataHex { address, len, data } => {
                assert_eq!(address, 0x0010);
                assert_eq!(len, 4);
                assert_eq!(data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
            }
            _ => panic!("unexpected frame"),
        }
    }

    #[test]
    fn rejects_data_len_mismatch() {
        let err = parse_device_frame("DATA|00010|4|DEAD").expect_err("must fail");
        assert_eq!(err, ParseError::LengthMismatch);
    }

    #[test]
    fn rejects_garbage_line() {
        let err = parse_device_frame("XYZ").expect_err("must fail");
        assert_eq!(err, ParseError::InvalidFormat);
    }

    #[test]
    fn rejects_invalid_hex_payload() {
        let err = parse_device_frame("DATA|00010|2|ZZZZ").expect_err("must fail");
        assert_eq!(err, ParseError::InvalidHexPayload);
    }
}
