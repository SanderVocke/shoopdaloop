use backend_bindings::{ProfilingReport, ProfilingReportItem};
use common::logging::macros::*;
shoop_log_unit!("Frontend.ProfilingReport");
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QString, QVariant};
use cxx_qt_lib_shoop::qvariant_helpers::qvariantmap_to_qvariant;

type QVariantMap = QMap<QMapPair_QString_QVariant>;

pub fn profiling_report_item_to_qvariantmap(item: &ProfilingReportItem) -> QVariantMap {
    let mut map = QVariantMap::default();
    map.insert(
        QString::from("key"),
        QVariant::from(&QString::from(&item.key)),
    );
    map.insert(QString::from("n_samples"), QVariant::from(&item.n_samples));
    map.insert(QString::from("average"), QVariant::from(&item.average));
    map.insert(QString::from("worst"), QVariant::from(&item.worst));
    map.insert(
        QString::from("most_recent"),
        QVariant::from(&item.most_recent),
    );

    map
}

pub fn profiling_report_to_qvariantmap(report: &ProfilingReport) -> QVariantMap {
    let mut map = QVariantMap::default();

    for item in report.items.iter() {
        let item_map = profiling_report_item_to_qvariantmap(&item);
        if let Ok(item_map_variant) = qvariantmap_to_qvariant(&item_map) {
            map.insert(QString::from(&item.key), item_map_variant);
        } else {
             error!("Failed to convert profiling report item for key {} to QVariant", item.key);
        }
    }

    map
}
