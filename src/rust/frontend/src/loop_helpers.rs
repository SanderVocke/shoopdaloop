use anyhow;
use backend_bindings::LoopMode;
use cxx_qt_lib::{QList, QVariant};
use cxx_qt_lib_shoop::{
    connection_types,
    invokable::invoke,
    qobject::{ffi::qobject_meta_type_name, QObject},
    qvariant_qobject::{qobject_ptr_to_qvariant, qvariant_to_qobject_ptr},
    qvariant_qvariantlist::QList_QVariant,
};

use common::logging::macros::*;

use crate::cxx_qt_shoop::qobj_loop_backend_bridge::LoopBackend;
shoop_log_unit!("Frontend.Loop");

pub fn get_backend_loop_handles_variant_list(
    loop_guis: &QList_QVariant,
) -> Result<QList_QVariant, anyhow::Error> {
    let mut backend_loop_handles: QList_QVariant = QList_QVariant::default();

    // Increment the reference count for all loops involved by storing shared pointers
    // into QVariants.
    loop_guis
            .iter()
            .map(|loop_variant| -> Result<(), anyhow::Error> {
                unsafe {
                    let loop_qobj = qvariant_to_qobject_ptr(loop_variant).ok_or(
                        anyhow::anyhow!("Failed to convert QVariant to QObject pointer"),
                    )?;
                    let backend_loop_handle : QVariant = invoke(
                        &mut *loop_qobj,
                        "get_backend_loop_shared_ptr()",
                        connection_types::DIRECT_CONNECTION,
                        &(),
                    )?;
                    backend_loop_handles.append(backend_loop_handle);
                    Ok(())
                }
            })
            .for_each(|result| match result {
                Ok(_) => (),
                Err(err) => {
                    error!("Failed to increment reference count for loop. This loop will be omitted. Details: {:?}", err)
                }
            });

    Ok(backend_loop_handles)
}

pub fn transition_gui_loops(
    loops: impl IntoIterator<Item = *mut QObject>,
    to_mode: LoopMode,
    maybe_cycles_delay: Option<i32>,
    maybe_to_sync_at_cycle: Option<i32>,
) -> Result<(), anyhow::Error> {
    // For loops on the GUI side, all they do when a multiple-transition is requested is forward
    // the call to the backend wrapper thread with a queued call.
    // It doesn't matter what kind of loop we are dealing with - the details will be
    // handled by the backend wrapper objects.

    unsafe {
        let mut list: QList_QVariant = QList::default();
        let mut call_on: *mut QObject = std::ptr::null_mut();
        for l in loops {
            list.append(qobject_ptr_to_qvariant(l));
            call_on = l;
        }
        invoke(
            &mut *call_on,
            "transition_multiple(QList<QVariant>,::std::int32_t,::std::int32_t,::std::int32_t)"
                ,
            connection_types::DIRECT_CONNECTION,
            &(
                list,
                to_mode as isize as i32,
                maybe_cycles_delay.unwrap_or(-1),
                maybe_to_sync_at_cycle.unwrap_or(-1),
            ),
        )?;
    }

    Ok(())
}

pub fn transition_backend_loops(
    loops: impl IntoIterator<Item = *mut QObject>,
    to_mode: LoopMode,
    maybe_cycles_delay: Option<i32>,
    maybe_to_sync_at_cycle: Option<i32>,
) -> Result<(), anyhow::Error> {
    // In the back-end wrappers thread, we need to make a distinction between kinds of loops:
    // - "true" loops need to be transitioned as a group with a single back-end API call,
    //   to ensure the transition at the exact same time
    // - composite loops, which don't exist in the back-end, need to be transitioned one
    //   at a time.
    let mut unison_transition_loops: QList_QVariant = QList::default();

    unsafe {
        for l in loops {
            if qobject_meta_type_name(&*l)? == LoopBackend::metatype_name() {
                unison_transition_loops.append(qobject_ptr_to_qvariant(l));
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
                .ok_or(anyhow::anyhow!("No loops to transition"))?;
            let first = qvariant_to_qobject_ptr(first).ok_or(anyhow::anyhow!("Not a QObject"))?;
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
