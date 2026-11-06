use super::*;

impl Service {
    pub fn management(&mut self, request: ManagementRequest) -> ManagementReply {
        match request {
            ManagementRequest::Settings { version } => {
                ManagementReply::Refused(if version != VERSION {
                    Refusal::Version
                } else {
                    Refusal::Unsupported
                })
            }
            ManagementRequest::ClaimShutdown { version, operation } => {
                if version != VERSION {
                    return ManagementReply::Refused(Refusal::Version);
                }
                if !request.valid() {
                    return ManagementReply::Refused(Refusal::Identity);
                }
                if self.lease.is_some() {
                    return ManagementReply::Refused(Refusal::Busy);
                }
                if let Some(authority) = self
                    .last_authority
                    .filter(|authority| authority.client == operation.client)
                {
                    if self.pending.contains_key(&operation)
                        || self.results.iter().any(|result| result.id == operation)
                    {
                        return self.management(ManagementRequest::Result {
                            authority,
                            operation,
                        });
                    }
                }
                if self.closed
                    || self.shutdown.is_some()
                    || (!self.is_ready() && !self.engine_closed)
                {
                    return ManagementReply::Refused(Refusal::NotReady);
                }
                if self.actors.contains_key(&operation.client) {
                    return ManagementReply::Refused(Refusal::Duplicate);
                }
                let Some(generation) = self.generation.checked_add(1) else {
                    return ManagementReply::Refused(Refusal::Overloaded);
                };
                self.generation = generation;
                let authority = Authority {
                    lease: LeaseId(operation.client.0),
                    client: operation.client,
                    generation: ConnectionGeneration(generation),
                };
                self.actors.clear();
                self.actors.insert(operation.client, 0);
                self.last_authority = Some(authority);
                self.management(ManagementRequest::Shutdown {
                    authority,
                    operation,
                })
            }
            ManagementRequest::Status { version } => {
                if version != VERSION {
                    return ManagementReply::Refused(Refusal::Version);
                }
                ManagementReply::Status {
                    version: VERSION,
                    ready: self.is_ready(),
                    busy: self.lease.is_some(),
                    closed: self.closed,
                }
            }
            ManagementRequest::Shutdown {
                authority,
                operation,
            }
            | ManagementRequest::Result {
                authority,
                operation,
            } => {
                if self.last_authority != Some(authority) {
                    return ManagementReply::Refused(Refusal::StaleAuthority);
                }
                if !authority.is_valid()
                    || operation.client != authority.client
                    || operation.sequence == 0
                {
                    return ManagementReply::Refused(Refusal::Identity);
                }
                let submit = matches!(request, ManagementRequest::Shutdown { .. });
                if submit {
                    if let Err(reason) = self.admit(authority, operation, None, None, None) {
                        return ManagementReply::Refused(reason);
                    }
                }
                if let Some(result) = self.results.iter().find(|result| result.id == operation) {
                    ManagementReply::Finished {
                        authority,
                        result: result.clone(),
                    }
                } else if let Some(pending) = self.pending.get(&operation) {
                    ManagementReply::Accepted {
                        authority,
                        operation: PendingOperation {
                            id: operation,
                            accepted_at: pending.accepted_at,
                        },
                    }
                } else {
                    ManagementReply::Refused(Refusal::Invalid)
                }
            }
        }
    }

    pub fn begin_host_cleanup(&mut self) -> Result<(), Refusal> {
        if !self.shutdown_ready {
            return Err(Refusal::NotReady);
        }
        self.diagnostics.detach();
        if !self.engine_closed {
            self.backend.close_connection();
            self.engine_closed = true;
        }
        Ok(())
    }
}
