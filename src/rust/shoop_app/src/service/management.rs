use super::*;

impl Service {
    pub fn management(&mut self, request: ManagementRequest) -> ManagementReply {
        match request {
            ManagementRequest::Status {
                version,
                host,
                session,
            } => {
                if version != VERSION {
                    return ManagementReply::Refused(Refusal::Version);
                }
                if host != self.options.host || session != self.options.session {
                    return ManagementReply::Refused(Refusal::Identity);
                }
                ManagementReply::Status {
                    version: VERSION,
                    host,
                    session,
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
                    if let Err(reason) = self.admit(authority, operation, None, None) {
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
        if !self.engine_closed {
            self.backend.close_connection();
            self.engine_closed = true;
        }
        Ok(())
    }
}
