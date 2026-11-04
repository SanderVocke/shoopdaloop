//! Inject the active driver's optional graph capability into the session service.
pub fn controller(
    host: &shoop_rt_host_native::NativeHost,
) -> Option<Box<dyn shoop_app::HostGraphController>> {
    host.graph_control()
        .map(|control| Box::new(control) as Box<dyn shoop_app::HostGraphController>)
}
