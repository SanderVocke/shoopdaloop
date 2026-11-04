//! External port graph values shared with every provider.
pub use shoop_port_graph::{
    Capabilities as HostGraphCapabilities, ConnectionRequest as HostGraphRequest,
    DataType as PortDataType, Direction as PortDirection, EndpointId as HostGraphPortId,
    Link as HostGraphLink, LocalPortKey as HostGraphLocalKey, Port as HostGraphPort,
    Publication as HostGraphPublication, PublicationState as HostGraphPublicationState,
    Snapshot as HostGraphState, Status as HostGraphStatus,
};
