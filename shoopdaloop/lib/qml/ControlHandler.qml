import QtQuick 6.3
import QtQuick.Controls 6.3
import Logger

import "../backend/frontend_interface/types.js" as Types

// This item defines the interface through which external
// control of ShoopDaLoop can take place. E.g. through MIDI.
// Other items should derive from this and implement the _impl functions.
// Alternatively if they can't derive from this item, they can instead
// instantiate it and set the 'target' property to themselves.
Item {
    id: root
    property var target: root


    // LOOPS
    // loop_selector selects the loop(s) to target.
    // For some interfaces only one loop may be selected (e.g. getters).
    // For some interfaces, the selector may yield multiple loops (e.g. transitions).
    // The selector can be one of:
    // - [track_idx, loop_idx]   (select single by coords)
    // - [[track_idx, loop_idx], [track_idx, loop_idx], ...]  (select multiple by coords)
    // - (loop) => <true/false> (functor which is passed the loop widget and should return true/false)

    // Loop getter interfaces
    function loop_is_playing(loop_selector) { return target.loop_is_playing_impl(loop_selector) }
    function loop_is_selected(loop_selector) { return target.loop_is_selected_impl(loop_selector) }
    function loop_is_targeted(loop_selector) { return target.loop_is_targeted_impl(loop_selector) }
    function loop_get_volume(loop_selector) { return target.loop_get_volume_impl(loop_selector) }
    function loop_get_balance(loop_selector) { return target.loop_get_balance_impl(loop_selector) }
    // Loop action interfaces
    function loop_play(loop_selector, cycles_delay, wait_sync) { target.loop_play_impl(loop_selector, cycles_delay, wait_sync) }
    function loop_stop(loop_selector, cycles_delay, wait_sync) { target.loop_stop_impl(loop_selector, cycles_delay, wait_sync) }
    function loop_record(loop_selector, cycles_delay, wait_sync) { target.loop_record_impl(loop_selector, cycles_delay, wait_sync) }
    function loop_record_n(loop_selector, n, cycles_delay, wait_sync) { target.loop_record_n_impl(loop_selector, n, cycles_delay, wait_sync) }
    function loop_clear(loop_selector, cycles_delay, wait_sync) { target.loop_clear_impl(loop_selector, cycles_delay, wait_sync) }
    function loop_set_volume(loop_selector, volume) { target.loop_set_volume_impl(loop_selector, volume) }
    function loop_set_balance(loop_selector, balance) { target.loop_set_balance_impl(loop_selector, balance) }
    function loop_play_dry_through_wet(loop_selector, cycles_delay, wait_sync) { target.loop_play_dry_through_wet_impl(loop_selector, cycles_delay, wait_sync) }
    function loop_re_record_fx(loop_selector, cycles_delay, wait_sync) { target.loop_re_record_fx_impl(loop_selector, cycles_delay, wait_sync) }
    function loop_play_solo(loop_selector, cycles_delay, wait_sync) { target.loop_play_solo_impl(loop_selector, cycles_delay, wait_sync) }
    function loop_select(loop_selector) { target.loop_select_impl(loop_selector) }
    function loop_target(loop_selector) { target.loop_target_impl(loop_selector) }
    function loop_deselect(loop_selector) { target.loop_deselect_impl(loop_selector) }
    function loop_untarget(loop_selector) { target.loop_untarget_impl(loop_selector) }
    function loop_toggle_selected(loop_selector) { target.loop_toggle_selected_impl(loop_selector) }
    function loop_toggle_targeted(loop_selector) { target.loop_toggle_targeted_impl(loop_selector) }
    // Loop internally implemented interfaces
    function loop_toggle_playing(loop_selector, cycles_delay, wait_sync) {
        if (loop_is_playing(loop_selector)) { return loop_stop(loop_selector, cycles_delay, wait_sync) }
        else { return loop_play(loop_selector, cycles_delay, wait_sync) }
    }

    // PORTS
    // port_selector selects the port(s) to target.
    // For some interfaces only one port may be selected (e.g. getters).
    // For some interfaces, the selector may yield multiple ports (e.g. set volume).
    // The selector can be one of:
    // - [track_idx, fn] (fn is (port) => true/false, applied to ports of track)
    // - fn              (fn is (port) => true/false, applied to ports of all tracks)

    // Port getter interfaces
    function port_get_volume(port_selector) { return target.port_get_volume_impl(port_selector) }
    function port_get_muted(port_selector) { return target.port_get_muted_impl(port_selector) }
    function port_get_input_muted(port_selector) { return target.port_get_input_muted_impl(port_selector) }
    // Port action interfaces
    function port_mute(port_selector) { target.port_mute_impl(port_selector) }
    function port_mute_input(port_selector) { target.port_mute_input_impl(port_selector) }
    function port_unmute(port_selector) { target.port_unmute_impl(port_selector) }
    function port_unmute_input(port_selector) { target.port_unmute_input_impl(port_selector) }
    function port_set_volume(port_selector, vol) { target.port_set_volume_impl(port_selector, vol) }
}