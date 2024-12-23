use cxx_qt_lib::QVariant;
use crate::cxx_qt_shoop::qobj_compositeloop_bridge::CompositeLoopRust;

pub use crate::cxx_qt_shoop::qobj_compositeloop_bridge::CompositeLoop;

use std::pin::Pin;
use common::logging::macros::*;
shoop_log_unit!("Frontend.CompositeLoop");

impl CompositeLoop {
    pub fn update_sync_position(&mut self) {
        // Implement the logic to update sync position
    }

    pub fn update_sync_length(&mut self) {
        // Implement the logic to update sync length
    }

    pub fn update_position(&mut self) {
        // Implement the logic to update position
    }

    pub fn transition(&mut self, mode: i32, maybe_delay: i32, maybe_to_sync_at_cycle: i32) {
        // Implement the logic for transition
    }

    pub fn handle_sync_loop_trigger(&mut self, cycle_nr: i32) {
        // Implement the logic to handle sync loop trigger
    }

    pub fn adopt_ringbuffers(&mut self, reverse_start_cycle: QVariant, cycles_length: QVariant, go_to_cycle: i32, go_to_mode: i32) {
        // Implement the logic to adopt ringbuffers
    }

    pub fn update_on_other_thread(&mut self) {
        // Implement the logic to update from the backend
    }

    pub fn dependent_will_handle_sync_loop_cycle(&mut self, cycle_nr: i32) {
        // Implement the logic for dependent handling sync loop cycle
    }

    pub unsafe fn initialize_impl_with_result(mut self : Pin<&mut CompositeLoop>) -> Result<(), anyhow::Error> {
        // Initialize properties
        self.as_mut().set_schedule(QVariant::default());
        self.as_mut().set_running_loops(QVariant::default());
        self.as_mut().set_iteration(0);
        self.as_mut().set_play_after_record(false);
        self.as_mut().set_sync_mode_active(false);
        self.as_mut().set_mode(0);
        self.as_mut().set_next_mode(0);
        self.as_mut().set_next_transition_delay(-1);
        self.as_mut().set_n_cycles(0);
        self.as_mut().set_length(0);
        self.as_mut().set_kind(QString::from("regular"));
        self.as_mut().set_sync_position(0);
        self.as_mut().set_sync_length(0);
        self.as_mut().set_position(0);

        // Connect signals
        self.as_mut().on_schedule_changed(|o| {
            o.update_length();
        }).release();

        self.as_mut().on_sync_loop_changed(|o| {
            o.update_sync_position();
            o.update_sync_length();
        }).release();

        self.as_mut().on_iteration_changed(|o| {
            o.update_position();
        }).release();

        self.as_mut().on_mode_changed(|o| {
            o.update_position();
        }).release();

        Ok(())
    }

    pub fn initialize_impl(self : Pin<&mut CompositeLoop>) {
        unsafe {
            match self.initialize_impl_with_result() {
                Ok(()) => (),
                Err(e) => { error!("Failed to initialize: {:?}", e); }
            }
        }
    }
}
