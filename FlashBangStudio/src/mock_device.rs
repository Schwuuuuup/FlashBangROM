/// In-memory mock of an SST39SF040 (512 KiB, MFR=0xBF, DEV=0xB7).
///
/// Responds to the FlashBang ASCII protocol commands and returns
/// deterministic, pre-populated data so the host TUI can be tested
/// without any physical hardware.
pub struct MockDevice {
    pub memory: Vec<u8>,
}

impl MockDevice {
    pub fn new() -> Self {
        let size = 512 * 1024;
        let mut memory = vec![0xFF_u8; size];

        // Fake header at offset 0x0000
        let header = b"FLASHBANG-DEMO\x00\x01\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF";
        memory[..header.len()].copy_from_slice(header);

        // Incrementing pattern at 0x1000–0x101F
        for (i, b) in memory[0x1000..0x1020].iter_mut().enumerate() {
            *b = i as u8;
        }

        // Pseudo-random data at 0x2000–0x20FF
        for (i, b) in memory[0x2000..0x2100].iter_mut().enumerate() {
            *b = ((i.wrapping_mul(3).wrapping_add(0x42)) & 0xFF) as u8;
        }

        MockDevice { memory }
    }

    /// Process a command string and return the response lines.
    pub fn handle(&self, cmd: &str) -> Vec<String> {
        let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
        match parts.first().copied().unwrap_or("") {
            "HELLO" => vec!["HELLO|flashbang-mock-0.3.0|0.1|sst39-core,data-hex".to_string()],
            "ID" => vec!["OK|ID|manufacturer=0xBF,device=0xB7".to_string()],
            "READ" => self.handle_read(&parts),
            "ERASE_SECTOR" => vec!["OK|ERASE_SECTOR|done".to_string()],
            "ERASE_CHIP" => vec!["OK|ERASE_CHIP|done".to_string()],
            "PROGRAM" => vec!["OK|PROGRAM|done".to_string()],
            _ => vec!["ERR|E_PARSE|unknown command".to_string()],
        }
    }

    fn handle_read(&self, parts: &[&str]) -> Vec<String> {
        if parts.len() < 3 {
            return vec!["ERR|E_PARSE|READ requires addr and len".to_string()];
        }

        let addr_str = parts[1].trim_start_matches("0x").trim_start_matches("0X");
        let addr = usize::from_str_radix(addr_str, 16).unwrap_or(0);
        let len: usize = parts[2].parse().unwrap_or(0);

        if addr + len > self.memory.len() {
            return vec!["ERR|E_RANGE|address range out of bounds".to_string()];
        }

        const CHUNK: usize = 32;
        let mut frames = Vec::new();
        let mut offset = 0;

        while offset < len {
            let n = (len - offset).min(CHUNK);
            let slice = &self.memory[addr + offset..addr + offset + n];
            let hex: String = slice.iter().map(|b| format!("{b:02X}")).collect();
            frames.push(format!("DATA|{:05X}|{n}|{hex}", addr + offset));
            offset += n;
        }

        frames.push("OK|READ|done".to_string());
        frames
    }
}
