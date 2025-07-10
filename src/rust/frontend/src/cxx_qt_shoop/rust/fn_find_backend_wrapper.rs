use crate::cxx_qt_lib_shoop::qobject::{qobject_class_name, qobject_object_name};
use crate::cxx_qt_lib_shoop::qquickitem::{qquickitem_to_qobject_mut, QQuickItem};
use crate::cxx_qt_shoop::qobj_find_parent_item;
use crate::cxx_qt_shoop::qobj_find_parent_item::FindParentItem;
use crate::cxx_qt_shoop::qobj_signature_backend_wrapper::constants::*;
use cxx::UniquePtr;
use cxx_qt_lib::QString;
use regex::Regex;

pub fn create_find_parent_backend_wrapper() -> UniquePtr<FindParentItem> {
    let mut rval = qobj_find_parent_item::make_unique();

    rval.as_mut()
        .unwrap()
        .set_item_bool_property_to_check(QString::from(PROP_READY));
    rval.as_mut()
        .unwrap()
        .set_find_predicate(Box::new(|q: *mut QQuickItem| unsafe {
            let qobj = qquickitem_to_qobject_mut(q);
            let obj_name = qobject_object_name(qobj.as_ref().unwrap());
            let match_obj_name =
                obj_name.unwrap_or("".to_string()) == String::from("shoop_backend_wrapper");
            let class_name_re = Regex::new(r"Backend(?:_QMLTYPE)?.*").unwrap();
            let match_class_name =
                class_name_re.is_match(qobject_class_name(qobj.as_ref().unwrap()).unwrap());
            match_obj_name || match_class_name
        }));

    rval
}
