use rt_fpga_rtl::host::{Contract, Error, Mailbox, RecoveryState, RegisterBus, registers::*};
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Clone, Copy, Debug, Default)]
enum Failure {
    #[default]
    None,
    BeforeWrite,
    AfterWrite,
    AfterStatus,
}
struct State {
    registers: BTreeMap<u32, u32>,
    failure: Failure,
    fail_read: Option<u32>,
    requested: bool,
    requests: usize,
    reads: usize,
    post_status: u32,
}
#[derive(Clone)]
struct Bus(Rc<RefCell<State>>);
impl Bus {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(State {
            registers: BTreeMap::from([
                (REG_ID, HOST_ID),
                (REG_VERSION, HOST_VERSION),
                (REG_CORE_CLOCK_HZ, 100_000_000),
                (REG_ARENA_BASE, 0x3000_0000),
                (REG_ARENA_BYTES, 0x0800_0000),
                (REG_EPOCH, 11),
                (REG_RECOVERY_ID, 0x5253_5431),
                (REG_RECOVERY_STATUS, 0),
            ]),
            failure: Failure::None,
            fail_read: None,
            requested: false,
            requests: 0,
            reads: 0,
            post_status: 1,
        })))
    }
    fn mailbox(&self) -> Mailbox<Self> {
        Mailbox::open(
            self.clone(),
            Contract {
                clock_hz: 100_000_000,
                arena_base: 0x3000_0000,
                arena_bytes: 0x0800_0000,
            },
        )
        .unwrap()
    }
    fn status(&self, status: u32, epoch: u32) {
        let mut state = self.0.borrow_mut();
        state.registers.insert(REG_RECOVERY_STATUS, status);
        state.registers.insert(REG_EPOCH, epoch);
    }
}
impl RegisterBus for Bus {
    type Error = u32;
    fn read(&mut self, offset: u32) -> Result<u32, u32> {
        let mut state = self.0.borrow_mut();
        state.reads += 1;
        if state.fail_read == Some(offset)
            || (offset == REG_RECOVERY_STATUS
                && state.requested
                && matches!(state.failure, Failure::AfterStatus))
        {
            state.fail_read = None;
            state.failure = Failure::None;
            return Err(99);
        }
        Ok(*state.registers.get(&offset).unwrap_or(&0))
    }
    fn write(&mut self, offset: u32, value: u32) -> Result<(), u32> {
        assert_eq!(offset, REG_RECOVERY_REQUEST);
        let mut state = self.0.borrow_mut();
        state.requests += 1;
        assert_eq!(value, state.registers[&REG_EPOCH]);
        if matches!(state.failure, Failure::BeforeWrite) {
            state.failure = Failure::None;
            return Err(99);
        }
        state.requested = true;
        let status = state.post_status;
        state.registers.insert(REG_RECOVERY_STATUS, status);
        if matches!(state.failure, Failure::AfterWrite) {
            state.failure = Failure::None;
            return Err(99);
        }
        Ok(())
    }
}

#[shoop_test]
fn uncertain_request_acknowledgements_preserve_query_authority_without_replay() {
    for failure in [
        Failure::BeforeWrite,
        Failure::AfterWrite,
        Failure::AfterStatus,
    ] {
        let bus = Bus::new();
        let mut recovery = bus.mailbox().recovery();
        bus.0.borrow_mut().failure = failure;
        assert_eq!(recovery.begin(), Err(Error::Bus(99)));
        assert!(matches!(recovery.reopen(), Err(Error::Full)));
        if matches!(failure, Failure::BeforeWrite) {
            bus.status(3, 11);
            assert_eq!(
                recovery.poll(),
                Err(Error::EpochChanged),
                "old reset-complete is not this request"
            );
            bus.status(3, 12);
            assert_eq!(recovery.poll(), Ok(RecoveryState::Failed));
        } else {
            bus.status(3, 12);
            assert_eq!(
                recovery.begin(),
                Err(Error::InvalidRequest),
                "a lost ACK cannot request another reset"
            );
            assert_eq!(recovery.poll(), Ok(RecoveryState::Ready(12)));
            assert!(recovery.reopen().is_ok());
            assert_eq!(recovery.poll(), Err(Error::InvalidRequest));
        }
        assert_eq!(bus.0.borrow().requests, 1);
    }
}

#[shoop_test]
fn failed_or_protocol_invalid_recovery_cannot_become_ready_on_a_late_status() {
    for status in [0, 4, 5, u32::MAX] {
        let bus = Bus::new();
        let mut recovery = bus.mailbox().recovery();
        recovery.begin().unwrap();
        bus.status(status, 11);
        assert_eq!(
            recovery.poll(),
            if status == 4 {
                Ok(RecoveryState::Failed)
            } else {
                Err(Error::UnexpectedResponse)
            }
        );
        bus.status(3, 12);
        let reads = bus.0.borrow().reads;
        assert_eq!(recovery.poll(), Ok(RecoveryState::Failed));
        assert_eq!(recovery.begin(), Err(Error::InvalidRequest));
        assert!(matches!(recovery.reopen(), Err(Error::Full)));
        assert_eq!(
            bus.0.borrow().reads,
            reads,
            "terminal recovery has no new authority"
        );
        assert_eq!(bus.0.borrow().requests, 1);
    }
    for epoch in [0, 11, 13, u32::MAX] {
        let bus = Bus::new();
        let mut recovery = bus.mailbox().recovery();
        recovery.begin().unwrap();
        bus.status(3, epoch);
        assert_eq!(recovery.poll(), Err(Error::EpochChanged));
        bus.status(3, 12);
        assert_eq!(recovery.poll(), Ok(RecoveryState::Failed));
        assert!(matches!(recovery.reopen(), Err(Error::Full)));
    }
    for status in [0, 5, u32::MAX] {
        let bus = Bus::new();
        let mut recovery = bus.mailbox().recovery();
        bus.0.borrow_mut().post_status = status;
        assert_eq!(recovery.begin(), Err(Error::EpochChanged));
        bus.status(3, 12);
        assert_eq!(recovery.poll(), Ok(RecoveryState::Failed));
        assert_eq!(recovery.begin(), Err(Error::InvalidRequest));
        assert_eq!(bus.0.borrow().requests, 1);
    }
}

#[shoop_test]
fn transient_query_errors_keep_the_same_pending_request_and_allow_only_fresh_completion() {
    let bus = Bus::new();
    let mut recovery = bus.mailbox().recovery();
    recovery.begin().unwrap();
    bus.0.borrow_mut().fail_read = Some(REG_RECOVERY_STATUS);
    assert_eq!(recovery.poll(), Err(Error::Bus(99)));
    assert_eq!(recovery.begin(), Err(Error::InvalidRequest));
    assert!(matches!(recovery.reopen(), Err(Error::Full)));
    bus.status(2, 11);
    assert_eq!(recovery.poll(), Ok(RecoveryState::Resetting));
    bus.status(3, 12);
    bus.0.borrow_mut().fail_read = Some(REG_EPOCH);
    assert_eq!(recovery.poll(), Err(Error::Bus(99)));
    assert!(matches!(recovery.reopen(), Err(Error::Full)));
    assert_eq!(recovery.poll(), Ok(RecoveryState::Ready(12)));
    assert_eq!(bus.0.borrow().requests, 1);
    assert!(recovery.reopen().is_ok());
}

#[shoop_test]
fn stale_mailboxes_cannot_recreate_recovery_authority_for_a_new_incarnation() {
    let bus = Bus::new();
    let mailbox = bus.mailbox();
    let mut recovery = mailbox.recovery();
    recovery.begin().unwrap();
    bus.status(3, 12);
    assert_eq!(recovery.poll(), Ok(RecoveryState::Ready(12)));
    assert!(recovery.reopen().is_ok());

    let mut stale = mailbox.recovery();
    assert!(matches!(stale.reopen(), Err(Error::EpochChanged)));
    assert_eq!(stale.begin(), Err(Error::EpochChanged));
    assert_eq!(bus.0.borrow().requests, 1);

    recovery.begin().unwrap();
    bus.status(3, 13);
    assert_eq!(recovery.poll(), Ok(RecoveryState::Ready(13)));
    assert!(recovery.reopen().is_ok());
    bus.status(3, 14);
    assert!(matches!(recovery.reopen(), Err(Error::EpochChanged)));
    assert_eq!(recovery.begin(), Err(Error::EpochChanged));
    assert_eq!(bus.0.borrow().requests, 2);
}

#[shoop_test]
fn refusals_before_a_recovery_request_have_no_reset_effect() {
    for epoch in [0, u32::MAX] {
        let bus = Bus::new();
        let mut recovery = bus.mailbox().recovery();
        bus.status(0, epoch);
        assert_eq!(recovery.begin(), Err(Error::EpochChanged));
        assert_eq!(bus.0.borrow().requests, 0);
    }
    for status in [1, 2, 4] {
        let bus = Bus::new();
        let mut recovery = bus.mailbox().recovery();
        bus.status(status, 11);
        assert_eq!(recovery.begin(), Err(Error::Full));
        assert_eq!(bus.0.borrow().requests, 0);
    }
    let bus = Bus::new();
    let mut recovery = bus.mailbox().recovery();
    bus.0.borrow_mut().registers.insert(REG_RECOVERY_ID, 0);
    assert_eq!(recovery.begin(), Err(Error::InvalidRequest));
    assert_eq!(bus.0.borrow().requests, 0);
}
