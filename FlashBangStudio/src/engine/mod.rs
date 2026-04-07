use crate::session::HelloInfo;

#[derive(Clone, Debug, Default)]
pub struct OperationStateView {
    pub is_busy: bool,
    pub busy_action: Option<String>,
}

#[derive(Clone, Debug)]
pub enum OperationEvent {
    Queued { label: String },
    Switched { label: String },
    Completed,
}

pub fn reduce_operation_event(
    _current: &OperationStateView,
    event: OperationEvent,
) -> OperationStateView {
    match event {
        OperationEvent::Queued { label } => OperationStateView {
            is_busy: true,
            busy_action: Some(label),
        },
        OperationEvent::Switched { label } => OperationStateView {
            is_busy: true,
            busy_action: Some(label),
        },
        OperationEvent::Completed => OperationStateView {
            is_busy: false,
            busy_action: None,
        },
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionKey {
    FetchImage,
    FetchRange,
    FetchSector,
    EraseImage,
    EraseSector,
    CopyImage,
    CopyRange,
    CopySector,
    FlashImage,
    FlashRange,
    FlashSector,
    LoadImage,
    LoadSector,
    SaveImage,
    SaveSector,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectFlowState {
    #[default]
    Inactive,
    Active,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectFlowStep {
    QueryFirmware,
    QueryId,
    UploadDriver,
    FetchImage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectFlowEvent {
    Start,
    FirmwareOk,
    IdOk,
    UploadDriverOk { auto_fetch: bool },
    FetchDone,
    Abort,
}

pub fn reduce_connect_flow(
    state: ConnectFlowState,
    event: ConnectFlowEvent,
) -> (ConnectFlowState, Option<ConnectFlowStep>) {
    match event {
        ConnectFlowEvent::Start => (ConnectFlowState::Active, Some(ConnectFlowStep::QueryFirmware)),
        ConnectFlowEvent::FirmwareOk if state == ConnectFlowState::Active => {
            (ConnectFlowState::Active, Some(ConnectFlowStep::QueryId))
        }
        ConnectFlowEvent::IdOk if state == ConnectFlowState::Active => {
            (ConnectFlowState::Active, Some(ConnectFlowStep::UploadDriver))
        }
        ConnectFlowEvent::UploadDriverOk { auto_fetch } if state == ConnectFlowState::Active => {
            if auto_fetch {
                (ConnectFlowState::Active, Some(ConnectFlowStep::FetchImage))
            } else {
                (ConnectFlowState::Inactive, None)
            }
        }
        ConnectFlowEvent::FetchDone if state == ConnectFlowState::Active => {
            (ConnectFlowState::Inactive, None)
        }
        ConnectFlowEvent::Abort => (ConnectFlowState::Inactive, None),
        _ => (state, None),
    }
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeState {
    pub operation: OperationStateView,
    pub connect_flow: ConnectFlowState,
}

impl RuntimeState {
    pub fn is_busy(&self) -> bool {
        self.operation.is_busy
    }

    pub fn busy_label(&self) -> Option<&str> {
        self.operation.busy_action.as_deref()
    }

    pub fn connect_active(&self) -> bool {
        self.connect_flow == ConnectFlowState::Active
    }
}

#[derive(Clone, Debug)]
pub enum RuntimeEvent {
    Operation(OperationEvent),
    ConnectFlow(ConnectFlowEvent),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeIntent {
    QueueConnectStep(ConnectFlowStep),
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeUpdate {
    pub state: RuntimeState,
    pub intents: Vec<RuntimeIntent>,
}

pub fn reduce_runtime_event(current: &RuntimeState, event: RuntimeEvent) -> RuntimeUpdate {
    match event {
        RuntimeEvent::Operation(op_event) => RuntimeUpdate {
            state: RuntimeState {
                operation: reduce_operation_event(&current.operation, op_event),
                connect_flow: current.connect_flow,
            },
            intents: Vec::new(),
        },
        RuntimeEvent::ConnectFlow(flow_event) => {
            let (connect_flow, next_connect_step) = reduce_connect_flow(current.connect_flow, flow_event);
            RuntimeUpdate {
                state: RuntimeState {
                    operation: current.operation.clone(),
                    connect_flow,
                },
                intents: next_connect_step
                    .map(RuntimeIntent::QueueConnectStep)
                    .into_iter()
                    .collect(),
            }
        }
    }
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

    pub fn availability_for(&self, key: ActionKey) -> &ActionAvailability {
        match key {
            ActionKey::FetchImage => &self.availability.fetch_image,
            ActionKey::FetchRange => &self.availability.fetch_range,
            ActionKey::FetchSector => &self.availability.fetch_sector,
            ActionKey::EraseImage => &self.availability.erase_image,
            ActionKey::EraseSector => &self.availability.erase_sector,
            ActionKey::CopyImage => &self.availability.copy_image,
            ActionKey::CopyRange => &self.availability.copy_range,
            ActionKey::CopySector => &self.availability.copy_sector,
            ActionKey::FlashImage => &self.availability.flash_image,
            ActionKey::FlashRange => &self.availability.flash_range,
            ActionKey::FlashSector => &self.availability.flash_sector,
            ActionKey::LoadImage => &self.availability.load_image,
            ActionKey::LoadSector => &self.availability.load_sector,
            ActionKey::SaveImage => &self.availability.save_image,
            ActionKey::SaveSector => &self.availability.save_sector,
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
