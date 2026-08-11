use crate::cxx_qt_shoop::qobj_frontend_refresh_bridge::FrontendRefresh;
use cxx_qt_lib_shoop::qobject::AsQObject;
use std::cell::RefCell;

thread_local! {
    static FRONTEND_REFRESH: RefCell<cxx::UniquePtr<FrontendRefresh>> =
        RefCell::new(FrontendRefresh::make_unique());
}

pub fn qobject_ptr() -> *mut cxx_qt::QObject {
    FRONTEND_REFRESH.with(|refresh| {
        let mut refresh = refresh.borrow_mut();
        refresh
            .as_mut()
            .map(|refresh| unsafe { refresh.get_unchecked_mut().mut_qobject_ptr() })
            .unwrap_or(std::ptr::null_mut())
    })
}

pub fn init() {
    FRONTEND_REFRESH.with(|refresh| {
        let _ = refresh.borrow();
    });
}
