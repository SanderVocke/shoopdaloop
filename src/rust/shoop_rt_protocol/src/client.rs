//! A client connection over any carrier, enforcing the engine's credits.

use crate::carrier::{Carrier, CarrierError};
use crate::message::{ClientMessage, EngineMessage};
use crate::schema::{BulkRead, Capabilities, DriverStatus, Limits, Request};
use crate::PROTOCOL_VERSION;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientState {
    Idle,
    Connecting,
    Connected,
    Rejected,
    Disconnected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientError {
    NotConnected,
    /// All credits of the lane are in use; wait for answers.
    Busy,
    Carrier(CarrierError),
}

pub struct ProtocolClient<C> {
    carrier: C,
    state: ClientState,
    capabilities: Capabilities,
    limits: Option<Limits>,
    driver: Option<DriverStatus>,
    next_correlation: u64,
    reliable_in_flight: u32,
    bulk_in_flight: u32,
}

impl<C: Carrier> ProtocolClient<C> {
    pub fn new(carrier: C) -> Self {
        Self {
            carrier,
            state: ClientState::Idle,
            capabilities: Capabilities::default(),
            limits: None,
            driver: None,
            next_correlation: 1,
            reliable_in_flight: 0,
            bulk_in_flight: 0,
        }
    }

    pub fn carrier_mut(&mut self) -> &mut C {
        &mut self.carrier
    }
    pub fn state(&self) -> ClientState {
        self.state
    }
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
    pub fn limits(&self) -> Option<Limits> {
        self.limits
    }
    pub fn driver(&self) -> Option<DriverStatus> {
        self.driver
    }
    pub fn reliable_in_flight(&self) -> u32 {
        self.reliable_in_flight
    }

    pub fn connect(&mut self) -> Result<(), ClientError> {
        self.send(ClientMessage::Hello {
            version: PROTOCOL_VERSION,
        })?;
        self.state = ClientState::Connecting;
        Ok(())
    }

    /// Send a request and return its correlation.
    pub fn request(&mut self, request: Request) -> Result<u64, ClientError> {
        let limits = self.connected()?;
        if self.reliable_in_flight >= u32::from(limits.reliable_credits) {
            return Err(ClientError::Busy);
        }
        let correlation = self.next_correlation;
        self.send(ClientMessage::Request {
            correlation,
            request,
        })?;
        self.next_correlation += 1;
        self.reliable_in_flight += 1;
        Ok(correlation)
    }

    pub fn bulk(&mut self, read: BulkRead) -> Result<(), ClientError> {
        let limits = self.connected()?;
        if self.bulk_in_flight >= u32::from(limits.bulk_credits) {
            return Err(ClientError::Busy);
        }
        self.send(ClientMessage::Bulk(read))?;
        self.bulk_in_flight += 1;
        Ok(())
    }

    pub fn resync(&mut self) -> Result<(), ClientError> {
        self.connected()?;
        self.send(ClientMessage::Resync)
    }

    pub fn disconnect(&mut self) {
        let _ = self.carrier.send(ClientMessage::Goodbye);
        self.state = ClientState::Disconnected;
    }

    /// Receive one message, updating connection state and credits.
    pub fn poll(&mut self) -> Option<EngineMessage> {
        let message = self.carrier.receive()?;
        match message {
            EngineMessage::Welcome {
                version,
                capabilities,
                limits,
                driver,
            } if version == PROTOCOL_VERSION && self.state == ClientState::Connecting => {
                self.state = ClientState::Connected;
                self.capabilities = capabilities;
                self.limits = Some(limits);
                self.driver = Some(driver);
            }
            EngineMessage::Welcome { .. } | EngineMessage::Rejected { .. } => {
                self.state = ClientState::Rejected;
            }
            EngineMessage::Result { .. } => {
                self.reliable_in_flight = self.reliable_in_flight.saturating_sub(1);
            }
            EngineMessage::Bulk(_) => {
                self.bulk_in_flight = self.bulk_in_flight.saturating_sub(1);
            }
            EngineMessage::LimitsChanged(limits) => self.limits = Some(limits),
            EngineMessage::Telemetry(crate::schema::Telemetry::Engine { driver, .. }) => {
                self.driver = Some(driver);
            }
            EngineMessage::Disconnected(_) => {
                self.state = ClientState::Disconnected;
                self.reliable_in_flight = 0;
                self.bulk_in_flight = 0;
            }
            EngineMessage::Telemetry(_) | EngineMessage::State(_) => {}
        }
        Some(message)
    }

    fn connected(&self) -> Result<Limits, ClientError> {
        match (self.state, self.limits) {
            (ClientState::Connected, Some(limits)) => Ok(limits),
            _ => Err(ClientError::NotConnected),
        }
    }

    fn send(&mut self, message: ClientMessage) -> Result<(), ClientError> {
        self.carrier.send(message).map_err(|error| {
            if error == CarrierError::Disconnected {
                self.state = ClientState::Disconnected;
            }
            ClientError::Carrier(error)
        })
    }
}
