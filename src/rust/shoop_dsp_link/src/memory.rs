use crate::{Block, Descriptor, Endpoint, Error};
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicBool, Ordering};
use rtrb::{Consumer, Producer, RingBuffer};

pub type Lease = Box<Block>;

pub struct RenderEndpoint {
    descriptor: Descriptor,
    available: Arc<AtomicBool>,
    tx: Producer<Lease>,
    free_input: Consumer<Lease>,
    input_spares: Vec<Lease>,
    rx: Consumer<Lease>,
    free_output: Producer<Lease>,
}
pub struct ServiceEndpoint {
    descriptor: Descriptor,
    available: Arc<AtomicBool>,
    tx: Consumer<Lease>,
    free_input: Producer<Lease>,
    rx: Producer<Lease>,
    free_output: Consumer<Lease>,
    output_spares: Vec<Lease>,
}
pub fn pair(descriptor: Descriptor) -> Result<(RenderEndpoint, ServiceEndpoint), Error> {
    let d = descriptor.validate()?;
    let n = d.slots as usize;
    let (tx, stx) = RingBuffer::new(n);
    let (rx, rrx) = RingBuffer::new(n);
    let (mut fi, rfi) = RingBuffer::new(n);
    let (mut fo, sfo) = RingBuffer::new(n);
    for _ in 0..n {
        fi.push(Box::new(Block::prepare(d)?))
            .map_err(|_| Error::Capacity)?;
        fo.push(Box::new(Block::prepare(d)?))
            .map_err(|_| Error::Capacity)?;
    }
    let available = Arc::new(AtomicBool::new(true));
    Ok((
        RenderEndpoint {
            descriptor: d,
            available: available.clone(),
            tx,
            free_input: rfi,
            input_spares: Vec::with_capacity(n),
            rx: rrx,
            free_output: fo,
        },
        ServiceEndpoint {
            descriptor: d,
            available,
            tx: stx,
            free_input: fi,
            rx,
            free_output: sfo,
            output_spares: Vec::with_capacity(n),
        },
    ))
}
impl Endpoint for RenderEndpoint {
    type Lease = Lease;
    fn descriptor(&self) -> Descriptor {
        self.descriptor
    }
    fn available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }
    fn acquire_input(&mut self) -> Option<Lease> {
        self.input_spares
            .pop()
            .or_else(|| self.free_input.pop().ok())
    }
    fn submit(&mut self, lease: Lease) -> Result<(), Lease> {
        if !self.available() {
            return Err(lease);
        }
        self.tx.push(lease).map_err(|e| match e {
            rtrb::PushError::Full(value) => value,
        })
    }
    fn release_input(&mut self, lease: Lease) {
        assert!(
            self.input_spares.len() < self.input_spares.capacity(),
            "reserved input credit"
        );
        self.input_spares.push(lease);
    }
    fn receive(&mut self) -> Option<Lease> {
        self.rx.pop().ok()
    }
    fn release_output(&mut self, lease: Lease) {
        assert!(
            self.free_output.push(lease).is_ok(),
            "reserved output credit"
        );
    }
}
impl ServiceEndpoint {
    pub fn descriptor(&self) -> Descriptor {
        self.descriptor
    }
    pub fn available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }
    pub fn set_available(&self, value: bool) {
        self.available.store(value, Ordering::Release);
    }
    pub fn receive(&mut self) -> Option<Lease> {
        self.tx.pop().ok()
    }
    pub fn release_input(&mut self, lease: Lease) {
        assert!(self.free_input.push(lease).is_ok(), "reserved input credit");
    }
    pub fn acquire_output(&mut self) -> Option<Lease> {
        self.output_spares
            .pop()
            .or_else(|| self.free_output.pop().ok())
    }
    pub fn release_output(&mut self, lease: Lease) {
        assert!(
            self.output_spares.len() < self.output_spares.capacity(),
            "reserved output credit"
        );
        self.output_spares.push(lease);
    }
    pub fn publish(&mut self, lease: Lease) -> Result<(), Lease> {
        self.rx.push(lease).map_err(|e| match e {
            rtrb::PushError::Full(value) => value,
        })
    }
}
impl Drop for ServiceEndpoint {
    fn drop(&mut self) {
        self.set_available(false);
    }
}
