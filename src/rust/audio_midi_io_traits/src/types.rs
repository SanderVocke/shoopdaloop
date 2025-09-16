pub struct ExternalPortDescriptor {
    pub name: String,
    pub direction: PortDirection,
    pub data_type: PortDataType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortDataType {
    Audio,
    Midi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortDirection {
    Input,
    Output,
}

pub struct ExternalConnectionStatus {
    pub external_port: String,
    pub connected: bool,
}
