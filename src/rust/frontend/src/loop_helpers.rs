use anyhow;
use backend_bindings::LoopMode;
use cxx_qt_lib::{QList, QVariant};
use cxx_qt_lib_shoop::{
    connection_types,
    invokable::invoke,
    qobject::QObject,
    qvariant_qobject::{qobject_ptr_to_qvariant, qvariant_to_qobject_ptr},
    qvariant_qvariantlist::QList_QVariant,
};

use common::logging::macros::*;
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
                        "get_backend_loop_shared_ptr()".to_string(),
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
    unsafe {
        let mut list : QList_QVariant = QList::default();
        let mut call_on : *mut QObject = std::ptr::null_mut();
        for l in loops {
            list.append(qobject_ptr_to_qvariant(l));
            call_on = l;
        }
        invoke(
            &mut *call_on,
            "transition_multiple(QList<QVariant>,::std::int32_t,::std::int32_t,::std::int32_t)"
                .to_string(),
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
    let mut to_transition_together: QList_QVariant = QList::default();

    unsafe {
        for l in loops {
            match invoke(
                &mut *l,
                "supports_transition_multiple()".to_string(),
                connection_types::DIRECT_CONNECTION,
                &(),
            )? {
                true => {
                    // Add to list to transition together
                    let variant = qobject_ptr_to_qvariant(l);
                    to_transition_together.append(variant);
                }
                false => {
                    let to_mode = to_mode as isize as i32;
                    let cycles_delay = maybe_cycles_delay.unwrap_or(-1);
                    let sync_at = maybe_to_sync_at_cycle.unwrap_or(-1);
                    // Transition individually
                    invoke(
                        &mut *l,
                        "transition(::std::int32_t,::std::int32_t,::std::int32_t)".to_string(),
                        connection_types::DIRECT_CONNECTION,
                        &(to_mode, cycles_delay, sync_at),
                    )?;
                }
            }
        }

        if to_transition_together.len() > 0 {
            let first = to_transition_together
                .get(0)
                .ok_or(anyhow::anyhow!("No loops to transition"))?;
            let mut first =
                qvariant_to_qobject_ptr(first).ok_or(anyhow::anyhow!("Not a QObject"))?;
            invoke(
                &mut *first,
                "transition_multiple(QList<QVariant>,::std::int32_t,::std::int32_t,::std::int32_t)"
                    .to_string(),
                connection_types::DIRECT_CONNECTION,
                &(
                    to_transition_together,
                    to_mode as isize as i32,
                    maybe_cycles_delay.unwrap_or(-1),
                    maybe_to_sync_at_cycle.unwrap_or(-1),
                ),
            )?;
        }
    }

    Ok(())
}
