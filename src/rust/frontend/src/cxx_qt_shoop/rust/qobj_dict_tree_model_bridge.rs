// A tree model which can be used with e.g. QML TreeView.
// The model is read-only and low-performance (not meant for large datasets).
// At construction, pass the data as a QVariantMap.
// The outer dict keys will be used as the tree item names.
// The inner dict keys shall be the roles and data for each item.
// Achieving the hierarchy is done by a separator string in the item names.
// For example: by supplying items "Root" and "Root.Something" with separator ".",
// the model will yield an item Something that is a child of Root.
#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
        type QList_f32 = cxx_qt_lib::QList<f32>;
        type QList_QString = cxx_qt_lib::QList<QString>;
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;

        include!(<QtCore/QAbstractItemModel>);
        type QAbstractItemModel;
        type QModelIndex;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QAbstractItemModel]
        type DictTreeModel = super::DictTreeModelRust;

        #[qinvokable]
        #[cxx_override]
        pub fn data(self: &DictTreeModel, index: &QModelIndex, role: i32) -> QVariant;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_dicttreemodel"]
        fn make_unique() -> UniquePtr<DictTreeModel>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_dicttreemodel"]
        unsafe fn register_qml_singleton(
            inference_example: *mut DictTreeModel,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

use std::rc::{Rc, Weak};

struct Node {
    pub name: String,
    pub parent: Option<Weak<Node>>,
    pub children: Vec<Rc<Node>>,
}

pub struct DictTreeModelRust {
    pub root_node: Rc<Node>,
}

impl Default for DictTreeModelRust {
    fn default() -> Self {
        Self {
            root_node: Rc::new(Node {
                name: "root".to_string(),
                parent: None,
                children: Vec::new(),
            }),
        }
    }
}
