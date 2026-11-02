use shoop_app::{Service, ServiceOptions};
use shoop_app_protocol::{HostId, SessionId};
use shoop_rt_backend::{EngineClock, RtBackend};
use shoop_rt_engine::{testing::OfflineRender, EngineDescription};
use std::{cell::RefCell, rc::Rc, time::Duration};
struct Clock(Rc<RefCell<OfflineRender>>);
impl EngineClock for Clock {
    fn advance(&mut self, elapsed: Duration) {
        self.0.borrow_mut().run_for(elapsed, |_, _| 0.25).unwrap();
    }
    fn wait(&mut self) -> bool {
        self.0.borrow_mut().run(16, |_, _| 0.25).unwrap();
        true
    }
}
pub fn service() -> Service {
    let (render, client) = OfflineRender::new(&EngineDescription::default()).unwrap();
    let backend = RtBackend::connect(client, Clock(Rc::new(RefCell::new(render)))).unwrap();
    let mut service = Service::new(
        Box::new(backend),
        ServiceOptions {
            host: HostId(1),
            session: SessionId(2),
            master_auto_connect: true,
            loop_smoothing_ms: 3,
        },
    )
    .unwrap();
    for _ in 0..128 {
        service.tick(Duration::from_millis(10));
        if service.is_ready() {
            return service;
        }
    }
    panic!("service did not initialize");
}
