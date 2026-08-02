use anyhow::anyhow;
use cxx_qt_lib::QList;
use cxx_qt_lib_shoop::{
    connection_types,
    invokable::invoke,
    qobject::QObject,
    qvariant_helpers::QList_QVariant,
    qvariant_helpers::{qobject_ptr_to_qvariant, qvariant_to_qobject_ptr},
};
use shoop_engine::LoopMode;

use crate::cxx_qt_shoop::qobj_loop_gui_bridge::ffi::qobject_to_loop_gui_ptr;

pub fn transition_frontend_loops(
    loops: impl IntoIterator<Item = *mut QObject>,
    to_mode: LoopMode,
    maybe_cycles_delay: Option<i32>,
    maybe_to_sync_at_cycle: Option<i32>,
) -> Result<(), anyhow::Error> {
    // Distinguish primitive loops, which share one engine-level grouped transition, from
    // composite loops, which each enqueue their own transition. All QObject inspection stays on
    // the GUI thread; the invoked objects own stable engine handles.
    let mut unison_transition_loops: QList_QVariant = QList::default();

    unsafe {
        for l in loops {
            if !qobject_to_loop_gui_ptr(l).is_null() {
                unison_transition_loops.append(qobject_ptr_to_qvariant(&l)?);
            } else {
                let to_mode = to_mode as isize as i32;
                let cycles_delay = maybe_cycles_delay.unwrap_or(-1);
                let sync_at = maybe_to_sync_at_cycle.unwrap_or(-1);
                // Transition individually
                invoke(
                    &mut *l,
                    "transition(::std::int32_t,::std::int32_t,::std::int32_t)",
                    connection_types::DIRECT_CONNECTION,
                    &(to_mode, cycles_delay, sync_at),
                )?;
            }
        }

        if unison_transition_loops.len() > 0 {
            let first = unison_transition_loops
                .get(0)
                .ok_or(anyhow!("No loops to transition"))?;
            let first = qvariant_to_qobject_ptr(first)?;
            invoke(
                &mut *first,
                "transition_multiple_backend_in_unison(QList<QVariant>,::std::int32_t,::std::int32_t,::std::int32_t)"
                    ,
                connection_types::DIRECT_CONNECTION,
                &(
                    unison_transition_loops,
                    to_mode as isize as i32,
                    maybe_cycles_delay.unwrap_or(-1),
                    maybe_to_sync_at_cycle.unwrap_or(-1),
                ),
            )?;
        }
    }

    Ok(())
}
