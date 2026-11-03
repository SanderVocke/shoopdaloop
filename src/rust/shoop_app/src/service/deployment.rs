use super::*;
use shoop_deployment_protocol as deploy;

fn refusal(reason: Refusal) -> deploy::Refusal {
    match reason {
        Refusal::Busy => deploy::Refusal::Busy,
        Refusal::NotReady => deploy::Refusal::NotReady,
        Refusal::ShutdownFailed | Refusal::Engine => deploy::Refusal::CleanupFailed,
        Refusal::Duplicate | Refusal::RevisionConflict => deploy::Refusal::Conflict,
        Refusal::Overloaded => deploy::Refusal::Overloaded,
        Refusal::Unsupported => deploy::Refusal::Unsupported,
        Refusal::Version => deploy::Refusal::Version,
        _ => deploy::Refusal::Invalid,
    }
}
fn reply(message: ManagementReply, operation: deploy::Operation) -> deploy::Reply {
    match message {
        ManagementReply::Accepted { .. } => deploy::Reply::Accepted(operation),
        ManagementReply::Finished { result, .. } => deploy::Reply::Finished {
            operation,
            result: match result.outcome {
                Outcome::Confirmed { .. } => Ok(()),
                Outcome::Refused { reason, .. } => Err(refusal(reason)),
                _ => Err(deploy::Refusal::Invalid),
            },
        },
        ManagementReply::Refused(reason) => deploy::Reply::Refused(refusal(reason)),
        _ => deploy::Reply::Refused(deploy::Refusal::Invalid),
    }
}

impl Service {
    /// Only the platform composition root enables this capability after health confirmation.
    #[tracing::instrument(skip_all)]
    pub fn set_deployment_info(&mut self, info: deploy::Info) -> Result<()> {
        if !info.valid() {
            return Err(anyhow!("invalid deployment identity"));
        }
        self.deployment = Some(info);
        Ok(())
    }

    /// The runtime may reset only for the operation currently being cleaned up.
    pub fn updater_request(&self) -> Option<deploy::Operation> {
        self.updater_operation
            .filter(|id| self.shutdown == Some(*id))
            .map(|id| deploy::Operation {
                client: id.client.0,
                sequence: id.sequence,
            })
    }

    #[tracing::instrument(skip_all)]
    pub fn deployment(&mut self, request: deploy::Request) -> deploy::Reply {
        if !request.valid() {
            return deploy::Reply::Refused(deploy::Refusal::Invalid);
        }
        let Some(info) = self.deployment.as_ref() else {
            return deploy::Reply::Refused(deploy::Refusal::Unsupported);
        };
        let (nonce, operation, enter) = match request {
            deploy::Request::Status => {
                let mut info = info.clone();
                info.busy = self.lease.is_some() || self.shutdown.is_some();
                info.ready = self.is_ready() && !self.closed;
                return deploy::Reply::Info(info);
            }
            deploy::Request::Enter {
                boot_nonce,
                operation,
            } => (boot_nonce, operation, true),
            deploy::Request::Result {
                boot_nonce,
                operation,
            } => (boot_nonce, operation, false),
        };
        if nonce != info.boot_nonce {
            return deploy::Reply::Refused(deploy::Refusal::StaleBoot);
        }
        let id = OperationId {
            client: ClientId(operation.client),
            sequence: operation.sequence,
        };
        if enter {
            // Never reinterpret an existing ordinary-shutdown operation as an update.
            if self.updater_operation != Some(id)
                && (self.pending.contains_key(&id) || self.results.iter().any(|r| r.id == id))
            {
                return deploy::Reply::Refused(deploy::Refusal::Conflict);
            }
            let message = self.management(ManagementRequest::ClaimShutdown {
                version: VERSION,
                operation: id,
            });
            if matches!(message, ManagementReply::Accepted { .. }) {
                self.updater_operation = Some(id);
            }
            reply(message, operation)
        } else {
            if self.updater_operation != Some(id) {
                return deploy::Reply::Refused(deploy::Refusal::Conflict);
            }
            let Some(authority) = self.last_authority else {
                return deploy::Reply::Refused(deploy::Refusal::Conflict);
            };
            reply(
                self.management(ManagementRequest::Result {
                    authority,
                    operation: id,
                }),
                operation,
            )
        }
    }
}
