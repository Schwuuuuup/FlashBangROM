use crate::session::HelloInfo;

#[derive(Clone, Debug, Default)]
pub struct OperationStateView {
    pub is_busy: bool,
    pub busy_action: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ActionAvailability {
    pub enabled: bool,
    pub disabled_reason: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ActionFacts {
    pub connected: bool,
    pub chip_known: bool,
    pub chip_size_known: bool,
    pub valid_range: bool,
    pub valid_sector: bool,
    pub workspace_available: bool,
    pub workspace_dirty: bool,
    pub inspector_image_known: bool,
    pub inspector_range_known: bool,
    pub inspector_sector_known: bool,
    pub flash_image_ready: bool,
    pub flash_range_ready: bool,
    pub flash_sector_ready: bool,
    pub flash_image_reason: Option<String>,
    pub flash_range_reason: Option<String>,
    pub flash_sector_reason: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ActionAvailabilitySet {
    pub fetch_image: ActionAvailability,
    pub fetch_range: ActionAvailability,
    pub fetch_sector: ActionAvailability,
    pub erase_image: ActionAvailability,
    pub erase_sector: ActionAvailability,
    pub copy_image: ActionAvailability,
    pub copy_range: ActionAvailability,
    pub copy_sector: ActionAvailability,
    pub flash_image: ActionAvailability,
    pub flash_range: ActionAvailability,
    pub flash_sector: ActionAvailability,
    pub load_image: ActionAvailability,
    pub load_sector: ActionAvailability,
    pub save_image: ActionAvailability,
    pub save_sector: ActionAvailability,
}

#[derive(Clone, Debug, Default)]
pub struct CapabilitySnapshot {
    pub protocol_commands: Vec<String>,
    pub driver_sequences: Vec<String>,
    pub custom_driver_commands: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SessionSnapshot {
    pub operation: OperationStateView,
    pub capabilities: CapabilitySnapshot,
    pub facts: ActionFacts,
    pub availability: ActionAvailabilitySet,
}

#[derive(Clone, Debug, Default)]
pub struct SessionSnapshotInput {
    pub operation: OperationStateView,
    pub hello: Option<HelloInfo>,
    pub upload_lines: Option<Vec<String>>,
    pub facts: ActionFacts,
}

impl SessionSnapshot {
    pub fn from_input(input: SessionSnapshotInput) -> Self {
        let capabilities = CapabilitySnapshot::from_sources(
            input.hello.as_ref(),
            input.upload_lines.as_deref(),
        );
        let availability = ActionAvailabilitySet::from_facts(&input.facts);
        Self {
            operation: input.operation,
            capabilities,
            facts: input.facts,
            availability,
        }
    }
}

impl CapabilitySnapshot {
    pub fn from_sources(hello: Option<&HelloInfo>, upload_lines: Option<&[String]>) -> Self {
        let mut protocol_commands = hello
            .map(|h| {
                h.capabilities
                    .iter()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        protocol_commands.sort();
        protocol_commands.dedup();

        let mut driver_sequences = upload_lines
            .map(|lines| {
                lines
                    .iter()
                    .filter_map(|line| line.strip_prefix("SEQUENCE|"))
                    .filter_map(|rest| rest.split('|').next())
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        driver_sequences.sort();
        driver_sequences.dedup();

        let standard = [
            "ID_ENTRY",
            "ID_READ",
            "ID_EXIT",
            "PROGRAM_BYTE",
            "PROGRAM_RANGE",
            "SECTOR_ERASE",
            "CHIP_ERASE",
        ];

        let custom_driver_commands = driver_sequences
            .iter()
            .filter(|name| !standard.contains(&name.as_str()))
            .cloned()
            .collect::<Vec<_>>();

        Self {
            protocol_commands,
            driver_sequences,
            custom_driver_commands,
        }
    }
}

impl ActionAvailabilitySet {
    pub fn from_facts(facts: &ActionFacts) -> Self {
        let fetch_image = gate(
            facts.connected && facts.chip_size_known,
            collect_reasons(&[
                (!facts.connected, "Not Connected"),
                (!facts.chip_size_known, "Kein erkannter Chip"),
            ]),
        );

        let fetch_range = gate(
            facts.connected && facts.chip_size_known && facts.valid_range,
            collect_reasons(&[
                (!facts.connected, "Not Connected"),
                (!facts.chip_size_known, "Kein erkannter Chip"),
                (!facts.valid_range, "Ungueltige Range-Eingabe"),
            ]),
        );

        let fetch_sector = gate(
            facts.connected && facts.chip_known && facts.valid_sector,
            collect_reasons(&[
                (!facts.connected, "Not Connected"),
                (!facts.chip_known, "Kein erkannter Chip"),
                (!facts.valid_sector, "Ungueltige Sektor-Eingabe"),
            ]),
        );

        let erase_image = gate(
            facts.connected && facts.chip_size_known,
            collect_reasons(&[
                (!facts.connected, "Not Connected"),
                (!facts.chip_size_known, "Kein erkannter Chip"),
            ]),
        );

        let erase_sector = gate(
            facts.connected && facts.chip_known && facts.valid_sector,
            collect_reasons(&[
                (!facts.connected, "Not Connected"),
                (!facts.chip_known, "Kein erkannter Chip"),
                (!facts.valid_sector, "Ungueltige Sektor-Eingabe"),
            ]),
        );

        let copy_image = gate(
            facts.inspector_image_known,
            collect_reasons(&[
                (!facts.chip_size_known, "Kein erkannter Chip"),
                (
                    !facts.inspector_image_known,
                    "Inspector-Daten nicht vollstaendig (erst Fetch ausfuehren)",
                ),
            ]),
        );

        let copy_range = gate(
            facts.valid_range && facts.inspector_range_known,
            collect_reasons(&[
                (!facts.valid_range, "Ungueltige Range-Eingabe"),
                (
                    !facts.inspector_range_known,
                    "Inspector-Range nicht gelesen (erst Fetch Range)",
                ),
            ]),
        );

        let copy_sector = gate(
            facts.valid_sector && facts.inspector_sector_known,
            collect_reasons(&[
                (!facts.valid_sector, "Ungueltige Sektor-Eingabe"),
                (
                    !facts.inspector_sector_known,
                    "Inspector-Sektor nicht gelesen (erst Fetch Sector)",
                ),
            ]),
        );

        let flash_image = gate(
            facts.flash_image_ready,
            facts
                .flash_image_reason
                .clone()
                .or_else(|| (!facts.chip_size_known).then(|| "Kein erkannter Chip".to_string())),
        );

        let flash_range = gate(
            facts.flash_range_ready,
            facts
                .flash_range_reason
                .clone()
                .or_else(|| (!facts.valid_range).then(|| "Ungueltige Range-Eingabe".to_string())),
        );

        let flash_sector = gate(
            facts.flash_sector_ready,
            facts
                .flash_sector_reason
                .clone()
                .or_else(|| (!facts.valid_sector).then(|| "Ungueltige Sektor-Eingabe".to_string())),
        );

        let load_image = gate(
            facts.workspace_available,
            (!facts.workspace_available).then(|| "Workspace nicht verfuegbar".to_string()),
        );
        let load_sector = load_image.clone();

        let save_image = gate(
            facts.workspace_available && facts.workspace_dirty,
            if !facts.workspace_available {
                Some("Workspace nicht verfuegbar".to_string())
            } else if !facts.workspace_dirty {
                Some("Keine ungespeicherten Workbench-Aenderungen".to_string())
            } else {
                None
            },
        );

        let save_sector = gate(
            facts.valid_sector && facts.workspace_dirty,
            if !facts.workspace_dirty {
                Some("Keine ungespeicherten Workbench-Aenderungen".to_string())
            } else if !facts.valid_sector {
                Some("Ungueltige Sektor-Eingabe".to_string())
            } else {
                None
            },
        );

        Self {
            fetch_image,
            fetch_range,
            fetch_sector,
            erase_image,
            erase_sector,
            copy_image,
            copy_range,
            copy_sector,
            flash_image,
            flash_range,
            flash_sector,
            load_image,
            load_sector,
            save_image,
            save_sector,
        }
    }
}

pub fn with_operation_gate(
    availability: &ActionAvailability,
    operation: &OperationStateView,
) -> ActionAvailability {
    if operation.is_busy {
        let reason = operation
            .busy_action
            .as_deref()
            .map(|a| format!("GUI beschaeftigt: {a}"))
            .unwrap_or_else(|| "GUI beschaeftigt".to_string());
        return ActionAvailability {
            enabled: false,
            disabled_reason: Some(reason),
        };
    }

    availability.clone()
}

fn gate(enabled: bool, disabled_reason: Option<String>) -> ActionAvailability {
    ActionAvailability {
        enabled,
        disabled_reason: if enabled { None } else { disabled_reason },
    }
}

fn collect_reasons(parts: &[(bool, &str)]) -> Option<String> {
    let joined = parts
        .iter()
        .filter_map(|(cond, text)| cond.then_some(*text))
        .collect::<Vec<_>>()
        .join(" | ");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}
