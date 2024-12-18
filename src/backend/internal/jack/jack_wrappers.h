/* NOTICE

This wrapper was generated using https://github.com/hpvb/dynload-wrapper.
*/

#ifndef DYLIBLOAD_WRAPPER_JACK_WRAPPERS
#define DYLIBLOAD_WRAPPER_JACK_WRAPPERS
// This file is generated. Do not edit!
// see https://github.com/hpvb/dynload-wrapper for details
// generated by /home/sander/shoopdaloop/src/../third_party/dynload-wrapper/generate-wrapper.py 0.3 on 2023-11-15 15:04:11
// flags: /home/sander/shoopdaloop/src/../third_party/dynload-wrapper/generate-wrapper.py --include /usr/include/jack/jack.h --include /usr/include/jack/midiport.h --sys-include <jack/jack.h> --sys-include <jack/midiport.h> --soname libjack.so --init-name jack_wrappers --output-header /home/sander/shoopdaloop/build/cp311-cp311-linux_x86_64/libshoopdaloop_backend/jack_wrappers/jack_wrappers.h --output-implementation /home/sander/shoopdaloop/build/cp311-cp311-linux_x86_64/libshoopdaloop_backend/jack_wrappers/jack_wrappers.c
//
// LCOV_EXCL_START
#include <stdint.h>

#define jack_release_timebase jack_release_timebase_dylibloader_orig_jack_wrappers
#define jack_set_sync_callback jack_set_sync_callback_dylibloader_orig_jack_wrappers
#define jack_set_sync_timeout jack_set_sync_timeout_dylibloader_orig_jack_wrappers
#define jack_set_timebase_callback jack_set_timebase_callback_dylibloader_orig_jack_wrappers
#define jack_transport_locate jack_transport_locate_dylibloader_orig_jack_wrappers
#define jack_transport_query jack_transport_query_dylibloader_orig_jack_wrappers
#define jack_get_current_transport_frame jack_get_current_transport_frame_dylibloader_orig_jack_wrappers
#define jack_transport_reposition jack_transport_reposition_dylibloader_orig_jack_wrappers
#define jack_transport_start jack_transport_start_dylibloader_orig_jack_wrappers
#define jack_transport_stop jack_transport_stop_dylibloader_orig_jack_wrappers
#define jack_get_transport_info jack_get_transport_info_dylibloader_orig_jack_wrappers
#define jack_set_transport_info jack_set_transport_info_dylibloader_orig_jack_wrappers
#define jack_get_version jack_get_version_dylibloader_orig_jack_wrappers
#define jack_get_version_string jack_get_version_string_dylibloader_orig_jack_wrappers
#define jack_client_open jack_client_open_dylibloader_orig_jack_wrappers
#define jack_client_new jack_client_new_dylibloader_orig_jack_wrappers
#define jack_client_close jack_client_close_dylibloader_orig_jack_wrappers
#define jack_client_name_size jack_client_name_size_dylibloader_orig_jack_wrappers
#define jack_get_client_name jack_get_client_name_dylibloader_orig_jack_wrappers
#define jack_get_uuid_for_client_name jack_get_uuid_for_client_name_dylibloader_orig_jack_wrappers
#define jack_get_client_name_by_uuid jack_get_client_name_by_uuid_dylibloader_orig_jack_wrappers
#define jack_internal_client_new jack_internal_client_new_dylibloader_orig_jack_wrappers
#define jack_internal_client_close jack_internal_client_close_dylibloader_orig_jack_wrappers
#define jack_activate jack_activate_dylibloader_orig_jack_wrappers
#define jack_deactivate jack_deactivate_dylibloader_orig_jack_wrappers
#define jack_get_client_pid jack_get_client_pid_dylibloader_orig_jack_wrappers
#define jack_client_thread_id jack_client_thread_id_dylibloader_orig_jack_wrappers
#define jack_is_realtime jack_is_realtime_dylibloader_orig_jack_wrappers
#define jack_thread_wait jack_thread_wait_dylibloader_orig_jack_wrappers
#define jack_cycle_wait jack_cycle_wait_dylibloader_orig_jack_wrappers
#define jack_cycle_signal jack_cycle_signal_dylibloader_orig_jack_wrappers
#define jack_set_process_thread jack_set_process_thread_dylibloader_orig_jack_wrappers
#define jack_set_thread_init_callback jack_set_thread_init_callback_dylibloader_orig_jack_wrappers
#define jack_on_shutdown jack_on_shutdown_dylibloader_orig_jack_wrappers
#define jack_on_info_shutdown jack_on_info_shutdown_dylibloader_orig_jack_wrappers
#define jack_set_process_callback jack_set_process_callback_dylibloader_orig_jack_wrappers
#define jack_set_freewheel_callback jack_set_freewheel_callback_dylibloader_orig_jack_wrappers
#define jack_set_buffer_size_callback jack_set_buffer_size_callback_dylibloader_orig_jack_wrappers
#define jack_set_sample_rate_callback jack_set_sample_rate_callback_dylibloader_orig_jack_wrappers
#define jack_set_client_registration_callback jack_set_client_registration_callback_dylibloader_orig_jack_wrappers
#define jack_set_port_registration_callback jack_set_port_registration_callback_dylibloader_orig_jack_wrappers
#define jack_set_port_connect_callback jack_set_port_connect_callback_dylibloader_orig_jack_wrappers
#define jack_set_port_rename_callback jack_set_port_rename_callback_dylibloader_orig_jack_wrappers
#define jack_set_graph_order_callback jack_set_graph_order_callback_dylibloader_orig_jack_wrappers
#define jack_set_xrun_callback jack_set_xrun_callback_dylibloader_orig_jack_wrappers
#define jack_set_latency_callback jack_set_latency_callback_dylibloader_orig_jack_wrappers
#define jack_set_freewheel jack_set_freewheel_dylibloader_orig_jack_wrappers
#define jack_set_buffer_size jack_set_buffer_size_dylibloader_orig_jack_wrappers
#define jack_get_sample_rate jack_get_sample_rate_dylibloader_orig_jack_wrappers
#define jack_get_buffer_size jack_get_buffer_size_dylibloader_orig_jack_wrappers
#define jack_engine_takeover_timebase jack_engine_takeover_timebase_dylibloader_orig_jack_wrappers
#define jack_cpu_load jack_cpu_load_dylibloader_orig_jack_wrappers
#define jack_port_register jack_port_register_dylibloader_orig_jack_wrappers
#define jack_port_unregister jack_port_unregister_dylibloader_orig_jack_wrappers
#define jack_port_get_buffer jack_port_get_buffer_dylibloader_orig_jack_wrappers
#define jack_port_uuid jack_port_uuid_dylibloader_orig_jack_wrappers
#define jack_port_name jack_port_name_dylibloader_orig_jack_wrappers
#define jack_port_short_name jack_port_short_name_dylibloader_orig_jack_wrappers
#define jack_port_flags jack_port_flags_dylibloader_orig_jack_wrappers
#define jack_port_type jack_port_type_dylibloader_orig_jack_wrappers
#define jack_port_type_id jack_port_type_id_dylibloader_orig_jack_wrappers
#define jack_port_is_mine jack_port_is_mine_dylibloader_orig_jack_wrappers
#define jack_port_connected jack_port_connected_dylibloader_orig_jack_wrappers
#define jack_port_connected_to jack_port_connected_to_dylibloader_orig_jack_wrappers
#define jack_port_get_connections jack_port_get_connections_dylibloader_orig_jack_wrappers
#define jack_port_get_all_connections jack_port_get_all_connections_dylibloader_orig_jack_wrappers
#define jack_port_tie jack_port_tie_dylibloader_orig_jack_wrappers
#define jack_port_untie jack_port_untie_dylibloader_orig_jack_wrappers
#define jack_port_set_name jack_port_set_name_dylibloader_orig_jack_wrappers
#define jack_port_rename jack_port_rename_dylibloader_orig_jack_wrappers
#define jack_port_set_alias jack_port_set_alias_dylibloader_orig_jack_wrappers
#define jack_port_unset_alias jack_port_unset_alias_dylibloader_orig_jack_wrappers
#define jack_port_get_aliases jack_port_get_aliases_dylibloader_orig_jack_wrappers
#define jack_port_request_monitor jack_port_request_monitor_dylibloader_orig_jack_wrappers
#define jack_port_request_monitor_by_name jack_port_request_monitor_by_name_dylibloader_orig_jack_wrappers
#define jack_port_ensure_monitor jack_port_ensure_monitor_dylibloader_orig_jack_wrappers
#define jack_port_monitoring_input jack_port_monitoring_input_dylibloader_orig_jack_wrappers
#define jack_connect jack_connect_dylibloader_orig_jack_wrappers
#define jack_disconnect jack_disconnect_dylibloader_orig_jack_wrappers
#define jack_port_disconnect jack_port_disconnect_dylibloader_orig_jack_wrappers
#define jack_port_name_size jack_port_name_size_dylibloader_orig_jack_wrappers
#define jack_port_type_size jack_port_type_size_dylibloader_orig_jack_wrappers
#define jack_port_type_get_buffer_size jack_port_type_get_buffer_size_dylibloader_orig_jack_wrappers
#define jack_port_set_latency jack_port_set_latency_dylibloader_orig_jack_wrappers
#define jack_port_get_latency_range jack_port_get_latency_range_dylibloader_orig_jack_wrappers
#define jack_port_set_latency_range jack_port_set_latency_range_dylibloader_orig_jack_wrappers
#define jack_recompute_total_latencies jack_recompute_total_latencies_dylibloader_orig_jack_wrappers
#define jack_port_get_latency jack_port_get_latency_dylibloader_orig_jack_wrappers
#define jack_port_get_total_latency jack_port_get_total_latency_dylibloader_orig_jack_wrappers
#define jack_recompute_total_latency jack_recompute_total_latency_dylibloader_orig_jack_wrappers
#define jack_get_ports jack_get_ports_dylibloader_orig_jack_wrappers
#define jack_port_by_name jack_port_by_name_dylibloader_orig_jack_wrappers
#define jack_port_by_id jack_port_by_id_dylibloader_orig_jack_wrappers
#define jack_frames_since_cycle_start jack_frames_since_cycle_start_dylibloader_orig_jack_wrappers
#define jack_frame_time jack_frame_time_dylibloader_orig_jack_wrappers
#define jack_last_frame_time jack_last_frame_time_dylibloader_orig_jack_wrappers
#define jack_get_cycle_times jack_get_cycle_times_dylibloader_orig_jack_wrappers
#define jack_frames_to_time jack_frames_to_time_dylibloader_orig_jack_wrappers
#define jack_time_to_frames jack_time_to_frames_dylibloader_orig_jack_wrappers
#define jack_get_time jack_get_time_dylibloader_orig_jack_wrappers
#define jack_set_error_function jack_set_error_function_dylibloader_orig_jack_wrappers
#define jack_set_info_function jack_set_info_function_dylibloader_orig_jack_wrappers
#define jack_free jack_free_dylibloader_orig_jack_wrappers
#define jack_midi_get_event_count jack_midi_get_event_count_dylibloader_orig_jack_wrappers
#define jack_midi_event_get jack_midi_event_get_dylibloader_orig_jack_wrappers
#define jack_midi_clear_buffer jack_midi_clear_buffer_dylibloader_orig_jack_wrappers
#define jack_midi_reset_buffer jack_midi_reset_buffer_dylibloader_orig_jack_wrappers
#define jack_midi_max_event_size jack_midi_max_event_size_dylibloader_orig_jack_wrappers
#define jack_midi_event_reserve jack_midi_event_reserve_dylibloader_orig_jack_wrappers
#define jack_midi_event_write jack_midi_event_write_dylibloader_orig_jack_wrappers
#define jack_midi_get_lost_event_count jack_midi_get_lost_event_count_dylibloader_orig_jack_wrappers
#define NOMINMAX
#include <jack/jack.h>
#include <jack/midiport.h>
#undef min
#undef max
#undef jack_release_timebase
#undef jack_set_sync_callback
#undef jack_set_sync_timeout
#undef jack_set_timebase_callback
#undef jack_transport_locate
#undef jack_transport_query
#undef jack_get_current_transport_frame
#undef jack_transport_reposition
#undef jack_transport_start
#undef jack_transport_stop
#undef jack_get_transport_info
#undef jack_set_transport_info
#undef jack_get_version
#undef jack_get_version_string
#undef jack_client_open
#undef jack_client_new
#undef jack_client_close
#undef jack_client_name_size
#undef jack_get_client_name
#undef jack_get_uuid_for_client_name
#undef jack_get_client_name_by_uuid
#undef jack_internal_client_new
#undef jack_internal_client_close
#undef jack_activate
#undef jack_deactivate
#undef jack_get_client_pid
#undef jack_client_thread_id
#undef jack_is_realtime
#undef jack_thread_wait
#undef jack_cycle_wait
#undef jack_cycle_signal
#undef jack_set_process_thread
#undef jack_set_thread_init_callback
#undef jack_on_shutdown
#undef jack_on_info_shutdown
#undef jack_set_process_callback
#undef jack_set_freewheel_callback
#undef jack_set_buffer_size_callback
#undef jack_set_sample_rate_callback
#undef jack_set_client_registration_callback
#undef jack_set_port_registration_callback
#undef jack_set_port_connect_callback
#undef jack_set_port_rename_callback
#undef jack_set_graph_order_callback
#undef jack_set_xrun_callback
#undef jack_set_latency_callback
#undef jack_set_freewheel
#undef jack_set_buffer_size
#undef jack_get_sample_rate
#undef jack_get_buffer_size
#undef jack_engine_takeover_timebase
#undef jack_cpu_load
#undef jack_port_register
#undef jack_port_unregister
#undef jack_port_get_buffer
#undef jack_port_uuid
#undef jack_port_name
#undef jack_port_short_name
#undef jack_port_flags
#undef jack_port_type
#undef jack_port_type_id
#undef jack_port_is_mine
#undef jack_port_connected
#undef jack_port_connected_to
#undef jack_port_get_connections
#undef jack_port_get_all_connections
#undef jack_port_tie
#undef jack_port_untie
#undef jack_port_set_name
#undef jack_port_rename
#undef jack_port_set_alias
#undef jack_port_unset_alias
#undef jack_port_get_aliases
#undef jack_port_request_monitor
#undef jack_port_request_monitor_by_name
#undef jack_port_ensure_monitor
#undef jack_port_monitoring_input
#undef jack_connect
#undef jack_disconnect
#undef jack_port_disconnect
#undef jack_port_name_size
#undef jack_port_type_size
#undef jack_port_type_get_buffer_size
#undef jack_port_set_latency
#undef jack_port_get_latency_range
#undef jack_port_set_latency_range
#undef jack_recompute_total_latencies
#undef jack_port_get_latency
#undef jack_port_get_total_latency
#undef jack_recompute_total_latency
#undef jack_get_ports
#undef jack_port_by_name
#undef jack_port_by_id
#undef jack_frames_since_cycle_start
#undef jack_frame_time
#undef jack_last_frame_time
#undef jack_get_cycle_times
#undef jack_frames_to_time
#undef jack_time_to_frames
#undef jack_get_time
#undef jack_set_error_function
#undef jack_set_info_function
#undef jack_free
#undef jack_midi_get_event_count
#undef jack_midi_event_get
#undef jack_midi_clear_buffer
#undef jack_midi_reset_buffer
#undef jack_midi_max_event_size
#undef jack_midi_event_reserve
#undef jack_midi_event_write
#undef jack_midi_get_lost_event_count
#ifdef __cplusplus
extern "C" {
#endif
#define jack_release_timebase jack_release_timebase_dylibloader_wrapper_jack_wrappers
#define jack_set_sync_callback jack_set_sync_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_sync_timeout jack_set_sync_timeout_dylibloader_wrapper_jack_wrappers
#define jack_set_timebase_callback jack_set_timebase_callback_dylibloader_wrapper_jack_wrappers
#define jack_transport_locate jack_transport_locate_dylibloader_wrapper_jack_wrappers
#define jack_transport_query jack_transport_query_dylibloader_wrapper_jack_wrappers
#define jack_get_current_transport_frame jack_get_current_transport_frame_dylibloader_wrapper_jack_wrappers
#define jack_transport_reposition jack_transport_reposition_dylibloader_wrapper_jack_wrappers
#define jack_transport_start jack_transport_start_dylibloader_wrapper_jack_wrappers
#define jack_transport_stop jack_transport_stop_dylibloader_wrapper_jack_wrappers
#define jack_get_transport_info jack_get_transport_info_dylibloader_wrapper_jack_wrappers
#define jack_set_transport_info jack_set_transport_info_dylibloader_wrapper_jack_wrappers
#define jack_get_version jack_get_version_dylibloader_wrapper_jack_wrappers
#define jack_get_version_string jack_get_version_string_dylibloader_wrapper_jack_wrappers
#define jack_client_open jack_client_open_dylibloader_wrapper_jack_wrappers
#define jack_client_new jack_client_new_dylibloader_wrapper_jack_wrappers
#define jack_client_close jack_client_close_dylibloader_wrapper_jack_wrappers
#define jack_client_name_size jack_client_name_size_dylibloader_wrapper_jack_wrappers
#define jack_get_client_name jack_get_client_name_dylibloader_wrapper_jack_wrappers
#define jack_get_uuid_for_client_name jack_get_uuid_for_client_name_dylibloader_wrapper_jack_wrappers
#define jack_get_client_name_by_uuid jack_get_client_name_by_uuid_dylibloader_wrapper_jack_wrappers
#define jack_internal_client_new jack_internal_client_new_dylibloader_wrapper_jack_wrappers
#define jack_internal_client_close jack_internal_client_close_dylibloader_wrapper_jack_wrappers
#define jack_activate jack_activate_dylibloader_wrapper_jack_wrappers
#define jack_deactivate jack_deactivate_dylibloader_wrapper_jack_wrappers
#define jack_get_client_pid jack_get_client_pid_dylibloader_wrapper_jack_wrappers
#define jack_client_thread_id jack_client_thread_id_dylibloader_wrapper_jack_wrappers
#define jack_is_realtime jack_is_realtime_dylibloader_wrapper_jack_wrappers
#define jack_thread_wait jack_thread_wait_dylibloader_wrapper_jack_wrappers
#define jack_cycle_wait jack_cycle_wait_dylibloader_wrapper_jack_wrappers
#define jack_cycle_signal jack_cycle_signal_dylibloader_wrapper_jack_wrappers
#define jack_set_process_thread jack_set_process_thread_dylibloader_wrapper_jack_wrappers
#define jack_set_thread_init_callback jack_set_thread_init_callback_dylibloader_wrapper_jack_wrappers
#define jack_on_shutdown jack_on_shutdown_dylibloader_wrapper_jack_wrappers
#define jack_on_info_shutdown jack_on_info_shutdown_dylibloader_wrapper_jack_wrappers
#define jack_set_process_callback jack_set_process_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_freewheel_callback jack_set_freewheel_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_buffer_size_callback jack_set_buffer_size_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_sample_rate_callback jack_set_sample_rate_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_client_registration_callback jack_set_client_registration_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_port_registration_callback jack_set_port_registration_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_port_connect_callback jack_set_port_connect_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_port_rename_callback jack_set_port_rename_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_graph_order_callback jack_set_graph_order_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_xrun_callback jack_set_xrun_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_latency_callback jack_set_latency_callback_dylibloader_wrapper_jack_wrappers
#define jack_set_freewheel jack_set_freewheel_dylibloader_wrapper_jack_wrappers
#define jack_set_buffer_size jack_set_buffer_size_dylibloader_wrapper_jack_wrappers
#define jack_get_sample_rate jack_get_sample_rate_dylibloader_wrapper_jack_wrappers
#define jack_get_buffer_size jack_get_buffer_size_dylibloader_wrapper_jack_wrappers
#define jack_engine_takeover_timebase jack_engine_takeover_timebase_dylibloader_wrapper_jack_wrappers
#define jack_cpu_load jack_cpu_load_dylibloader_wrapper_jack_wrappers
#define jack_port_register jack_port_register_dylibloader_wrapper_jack_wrappers
#define jack_port_unregister jack_port_unregister_dylibloader_wrapper_jack_wrappers
#define jack_port_get_buffer jack_port_get_buffer_dylibloader_wrapper_jack_wrappers
#define jack_port_uuid jack_port_uuid_dylibloader_wrapper_jack_wrappers
#define jack_port_name jack_port_name_dylibloader_wrapper_jack_wrappers
#define jack_port_short_name jack_port_short_name_dylibloader_wrapper_jack_wrappers
#define jack_port_flags jack_port_flags_dylibloader_wrapper_jack_wrappers
#define jack_port_type jack_port_type_dylibloader_wrapper_jack_wrappers
#define jack_port_type_id jack_port_type_id_dylibloader_wrapper_jack_wrappers
#define jack_port_is_mine jack_port_is_mine_dylibloader_wrapper_jack_wrappers
#define jack_port_connected jack_port_connected_dylibloader_wrapper_jack_wrappers
#define jack_port_connected_to jack_port_connected_to_dylibloader_wrapper_jack_wrappers
#define jack_port_get_connections jack_port_get_connections_dylibloader_wrapper_jack_wrappers
#define jack_port_get_all_connections jack_port_get_all_connections_dylibloader_wrapper_jack_wrappers
#define jack_port_tie jack_port_tie_dylibloader_wrapper_jack_wrappers
#define jack_port_untie jack_port_untie_dylibloader_wrapper_jack_wrappers
#define jack_port_set_name jack_port_set_name_dylibloader_wrapper_jack_wrappers
#define jack_port_rename jack_port_rename_dylibloader_wrapper_jack_wrappers
#define jack_port_set_alias jack_port_set_alias_dylibloader_wrapper_jack_wrappers
#define jack_port_unset_alias jack_port_unset_alias_dylibloader_wrapper_jack_wrappers
#define jack_port_get_aliases jack_port_get_aliases_dylibloader_wrapper_jack_wrappers
#define jack_port_request_monitor jack_port_request_monitor_dylibloader_wrapper_jack_wrappers
#define jack_port_request_monitor_by_name jack_port_request_monitor_by_name_dylibloader_wrapper_jack_wrappers
#define jack_port_ensure_monitor jack_port_ensure_monitor_dylibloader_wrapper_jack_wrappers
#define jack_port_monitoring_input jack_port_monitoring_input_dylibloader_wrapper_jack_wrappers
#define jack_connect jack_connect_dylibloader_wrapper_jack_wrappers
#define jack_disconnect jack_disconnect_dylibloader_wrapper_jack_wrappers
#define jack_port_disconnect jack_port_disconnect_dylibloader_wrapper_jack_wrappers
#define jack_port_name_size jack_port_name_size_dylibloader_wrapper_jack_wrappers
#define jack_port_type_size jack_port_type_size_dylibloader_wrapper_jack_wrappers
#define jack_port_type_get_buffer_size jack_port_type_get_buffer_size_dylibloader_wrapper_jack_wrappers
#define jack_port_set_latency jack_port_set_latency_dylibloader_wrapper_jack_wrappers
#define jack_port_get_latency_range jack_port_get_latency_range_dylibloader_wrapper_jack_wrappers
#define jack_port_set_latency_range jack_port_set_latency_range_dylibloader_wrapper_jack_wrappers
#define jack_recompute_total_latencies jack_recompute_total_latencies_dylibloader_wrapper_jack_wrappers
#define jack_port_get_latency jack_port_get_latency_dylibloader_wrapper_jack_wrappers
#define jack_port_get_total_latency jack_port_get_total_latency_dylibloader_wrapper_jack_wrappers
#define jack_recompute_total_latency jack_recompute_total_latency_dylibloader_wrapper_jack_wrappers
#define jack_get_ports jack_get_ports_dylibloader_wrapper_jack_wrappers
#define jack_port_by_name jack_port_by_name_dylibloader_wrapper_jack_wrappers
#define jack_port_by_id jack_port_by_id_dylibloader_wrapper_jack_wrappers
#define jack_frames_since_cycle_start jack_frames_since_cycle_start_dylibloader_wrapper_jack_wrappers
#define jack_frame_time jack_frame_time_dylibloader_wrapper_jack_wrappers
#define jack_last_frame_time jack_last_frame_time_dylibloader_wrapper_jack_wrappers
#define jack_get_cycle_times jack_get_cycle_times_dylibloader_wrapper_jack_wrappers
#define jack_frames_to_time jack_frames_to_time_dylibloader_wrapper_jack_wrappers
#define jack_time_to_frames jack_time_to_frames_dylibloader_wrapper_jack_wrappers
#define jack_get_time jack_get_time_dylibloader_wrapper_jack_wrappers
#define jack_set_error_function jack_set_error_function_dylibloader_wrapper_jack_wrappers
#define jack_set_info_function jack_set_info_function_dylibloader_wrapper_jack_wrappers
#define jack_free jack_free_dylibloader_wrapper_jack_wrappers
#define jack_midi_get_event_count jack_midi_get_event_count_dylibloader_wrapper_jack_wrappers
#define jack_midi_event_get jack_midi_event_get_dylibloader_wrapper_jack_wrappers
#define jack_midi_clear_buffer jack_midi_clear_buffer_dylibloader_wrapper_jack_wrappers
#define jack_midi_reset_buffer jack_midi_reset_buffer_dylibloader_wrapper_jack_wrappers
#define jack_midi_max_event_size jack_midi_max_event_size_dylibloader_wrapper_jack_wrappers
#define jack_midi_event_reserve jack_midi_event_reserve_dylibloader_wrapper_jack_wrappers
#define jack_midi_event_write jack_midi_event_write_dylibloader_wrapper_jack_wrappers
#define jack_midi_get_lost_event_count jack_midi_get_lost_event_count_dylibloader_wrapper_jack_wrappers
extern int (*jack_release_timebase_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern int (*jack_set_sync_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackSyncCallback, void*);
extern int (*jack_set_sync_timeout_dylibloader_wrapper_jack_wrappers)( jack_client_t*, jack_time_t);
extern int (*jack_set_timebase_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, int, JackTimebaseCallback, void*);
extern int (*jack_transport_locate_dylibloader_wrapper_jack_wrappers)( jack_client_t*, jack_nframes_t);
extern jack_transport_state_t (*jack_transport_query_dylibloader_wrapper_jack_wrappers)(const jack_client_t*, jack_position_t*);
extern jack_nframes_t (*jack_get_current_transport_frame_dylibloader_wrapper_jack_wrappers)(const jack_client_t*);
extern int (*jack_transport_reposition_dylibloader_wrapper_jack_wrappers)( jack_client_t*,const jack_position_t*);
extern void (*jack_transport_start_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern void (*jack_transport_stop_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern void (*jack_get_transport_info_dylibloader_wrapper_jack_wrappers)( jack_client_t*, jack_transport_info_t*);
extern void (*jack_set_transport_info_dylibloader_wrapper_jack_wrappers)( jack_client_t*, jack_transport_info_t*);
extern void (*jack_get_version_dylibloader_wrapper_jack_wrappers)( int*, int*, int*, int*);
extern const char* (*jack_get_version_string_dylibloader_wrapper_jack_wrappers)( void);
extern jack_client_t* (*jack_client_open_dylibloader_wrapper_jack_wrappers)(const char*, jack_options_t, jack_status_t*,...);
extern jack_client_t* (*jack_client_new_dylibloader_wrapper_jack_wrappers)(const char*);
extern int (*jack_client_close_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern int (*jack_client_name_size_dylibloader_wrapper_jack_wrappers)( void);
extern char* (*jack_get_client_name_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern char* (*jack_get_uuid_for_client_name_dylibloader_wrapper_jack_wrappers)( jack_client_t*,const char*);
extern char* (*jack_get_client_name_by_uuid_dylibloader_wrapper_jack_wrappers)( jack_client_t*,const char*);
extern int (*jack_internal_client_new_dylibloader_wrapper_jack_wrappers)(const char*,const char*,const char*);
extern void (*jack_internal_client_close_dylibloader_wrapper_jack_wrappers)(const char*);
extern int (*jack_activate_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern int (*jack_deactivate_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern int (*jack_get_client_pid_dylibloader_wrapper_jack_wrappers)(const char*);
extern jack_native_thread_t (*jack_client_thread_id_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern int (*jack_is_realtime_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern jack_nframes_t (*jack_thread_wait_dylibloader_wrapper_jack_wrappers)( jack_client_t*, int);
extern jack_nframes_t (*jack_cycle_wait_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern void (*jack_cycle_signal_dylibloader_wrapper_jack_wrappers)( jack_client_t*, int);
extern int (*jack_set_process_thread_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackThreadCallback, void*);
extern int (*jack_set_thread_init_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackThreadInitCallback, void*);
extern void (*jack_on_shutdown_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackShutdownCallback, void*);
extern void (*jack_on_info_shutdown_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackInfoShutdownCallback, void*);
extern int (*jack_set_process_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackProcessCallback, void*);
extern int (*jack_set_freewheel_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackFreewheelCallback, void*);
extern int (*jack_set_buffer_size_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackBufferSizeCallback, void*);
extern int (*jack_set_sample_rate_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackSampleRateCallback, void*);
extern int (*jack_set_client_registration_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackClientRegistrationCallback, void*);
extern int (*jack_set_port_registration_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackPortRegistrationCallback, void*);
extern int (*jack_set_port_connect_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackPortConnectCallback, void*);
extern int (*jack_set_port_rename_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackPortRenameCallback, void*);
extern int (*jack_set_graph_order_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackGraphOrderCallback, void*);
extern int (*jack_set_xrun_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackXRunCallback, void*);
extern int (*jack_set_latency_callback_dylibloader_wrapper_jack_wrappers)( jack_client_t*, JackLatencyCallback, void*);
extern int (*jack_set_freewheel_dylibloader_wrapper_jack_wrappers)( jack_client_t*, int);
extern int (*jack_set_buffer_size_dylibloader_wrapper_jack_wrappers)( jack_client_t*, jack_nframes_t);
extern jack_nframes_t (*jack_get_sample_rate_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern jack_nframes_t (*jack_get_buffer_size_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern int (*jack_engine_takeover_timebase_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern float (*jack_cpu_load_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern jack_port_t* (*jack_port_register_dylibloader_wrapper_jack_wrappers)( jack_client_t*,const char*,const char*, unsigned long, unsigned long);
extern int (*jack_port_unregister_dylibloader_wrapper_jack_wrappers)( jack_client_t*, jack_port_t*);
extern void* (*jack_port_get_buffer_dylibloader_wrapper_jack_wrappers)( jack_port_t*, jack_nframes_t);
extern jack_uuid_t (*jack_port_uuid_dylibloader_wrapper_jack_wrappers)(const jack_port_t*);
extern const char* (*jack_port_name_dylibloader_wrapper_jack_wrappers)(const jack_port_t*);
extern const char* (*jack_port_short_name_dylibloader_wrapper_jack_wrappers)(const jack_port_t*);
extern int (*jack_port_flags_dylibloader_wrapper_jack_wrappers)(const jack_port_t*);
extern const char* (*jack_port_type_dylibloader_wrapper_jack_wrappers)(const jack_port_t*);
extern jack_port_type_id_t (*jack_port_type_id_dylibloader_wrapper_jack_wrappers)(const jack_port_t*);
extern int (*jack_port_is_mine_dylibloader_wrapper_jack_wrappers)(const jack_client_t*,const jack_port_t*);
extern int (*jack_port_connected_dylibloader_wrapper_jack_wrappers)(const jack_port_t*);
extern int (*jack_port_connected_to_dylibloader_wrapper_jack_wrappers)(const jack_port_t*,const char*);
extern const char** (*jack_port_get_connections_dylibloader_wrapper_jack_wrappers)(const jack_port_t*);
extern const char** (*jack_port_get_all_connections_dylibloader_wrapper_jack_wrappers)(const jack_client_t*,const jack_port_t*);
extern int (*jack_port_tie_dylibloader_wrapper_jack_wrappers)( jack_port_t*, jack_port_t*);
extern int (*jack_port_untie_dylibloader_wrapper_jack_wrappers)( jack_port_t*);
extern int (*jack_port_set_name_dylibloader_wrapper_jack_wrappers)( jack_port_t*,const char*);
extern int (*jack_port_rename_dylibloader_wrapper_jack_wrappers)( jack_client_t*, jack_port_t*,const char*);
extern int (*jack_port_set_alias_dylibloader_wrapper_jack_wrappers)( jack_port_t*,const char*);
extern int (*jack_port_unset_alias_dylibloader_wrapper_jack_wrappers)( jack_port_t*,const char*);
extern int (*jack_port_get_aliases_dylibloader_wrapper_jack_wrappers)(const jack_port_t*, char* [2]);
extern int (*jack_port_request_monitor_dylibloader_wrapper_jack_wrappers)( jack_port_t*, int);
extern int (*jack_port_request_monitor_by_name_dylibloader_wrapper_jack_wrappers)( jack_client_t*,const char*, int);
extern int (*jack_port_ensure_monitor_dylibloader_wrapper_jack_wrappers)( jack_port_t*, int);
extern int (*jack_port_monitoring_input_dylibloader_wrapper_jack_wrappers)( jack_port_t*);
extern int (*jack_connect_dylibloader_wrapper_jack_wrappers)( jack_client_t*,const char*,const char*);
extern int (*jack_disconnect_dylibloader_wrapper_jack_wrappers)( jack_client_t*,const char*,const char*);
extern int (*jack_port_disconnect_dylibloader_wrapper_jack_wrappers)( jack_client_t*, jack_port_t*);
extern int (*jack_port_name_size_dylibloader_wrapper_jack_wrappers)( void);
extern int (*jack_port_type_size_dylibloader_wrapper_jack_wrappers)( void);
extern size_t (*jack_port_type_get_buffer_size_dylibloader_wrapper_jack_wrappers)( jack_client_t*,const char*);
extern void (*jack_port_set_latency_dylibloader_wrapper_jack_wrappers)( jack_port_t*, jack_nframes_t);
extern void (*jack_port_get_latency_range_dylibloader_wrapper_jack_wrappers)( jack_port_t*, jack_latency_callback_mode_t, jack_latency_range_t*);
extern void (*jack_port_set_latency_range_dylibloader_wrapper_jack_wrappers)( jack_port_t*, jack_latency_callback_mode_t, jack_latency_range_t*);
extern int (*jack_recompute_total_latencies_dylibloader_wrapper_jack_wrappers)( jack_client_t*);
extern jack_nframes_t (*jack_port_get_latency_dylibloader_wrapper_jack_wrappers)( jack_port_t*);
extern jack_nframes_t (*jack_port_get_total_latency_dylibloader_wrapper_jack_wrappers)( jack_client_t*, jack_port_t*);
extern int (*jack_recompute_total_latency_dylibloader_wrapper_jack_wrappers)( jack_client_t*, jack_port_t*);
extern const char** (*jack_get_ports_dylibloader_wrapper_jack_wrappers)( jack_client_t*,const char*,const char*, unsigned long);
extern jack_port_t* (*jack_port_by_name_dylibloader_wrapper_jack_wrappers)( jack_client_t*,const char*);
extern jack_port_t* (*jack_port_by_id_dylibloader_wrapper_jack_wrappers)( jack_client_t*, jack_port_id_t);
extern jack_nframes_t (*jack_frames_since_cycle_start_dylibloader_wrapper_jack_wrappers)(const jack_client_t*);
extern jack_nframes_t (*jack_frame_time_dylibloader_wrapper_jack_wrappers)(const jack_client_t*);
extern jack_nframes_t (*jack_last_frame_time_dylibloader_wrapper_jack_wrappers)(const jack_client_t*);
extern int (*jack_get_cycle_times_dylibloader_wrapper_jack_wrappers)(const jack_client_t*, jack_nframes_t*, jack_time_t*, jack_time_t*, float*);
extern jack_time_t (*jack_frames_to_time_dylibloader_wrapper_jack_wrappers)(const jack_client_t*, jack_nframes_t);
extern jack_nframes_t (*jack_time_to_frames_dylibloader_wrapper_jack_wrappers)(const jack_client_t*, jack_time_t);
extern jack_time_t (*jack_get_time_dylibloader_wrapper_jack_wrappers)( void);
extern void (*jack_set_error_function_dylibloader_wrapper_jack_wrappers)( void*);
extern void (*jack_set_info_function_dylibloader_wrapper_jack_wrappers)( void*);
extern void (*jack_free_dylibloader_wrapper_jack_wrappers)( void*);
extern uint32_t (*jack_midi_get_event_count_dylibloader_wrapper_jack_wrappers)( void*);
extern int (*jack_midi_event_get_dylibloader_wrapper_jack_wrappers)( jack_midi_event_t*, void*, uint32_t);
extern void (*jack_midi_clear_buffer_dylibloader_wrapper_jack_wrappers)( void*);
extern void (*jack_midi_reset_buffer_dylibloader_wrapper_jack_wrappers)( void*);
extern size_t (*jack_midi_max_event_size_dylibloader_wrapper_jack_wrappers)( void*);
extern jack_midi_data_t* (*jack_midi_event_reserve_dylibloader_wrapper_jack_wrappers)( void*, jack_nframes_t, size_t);
extern int (*jack_midi_event_write_dylibloader_wrapper_jack_wrappers)( void*, jack_nframes_t,const jack_midi_data_t*, size_t);
extern uint32_t (*jack_midi_get_lost_event_count_dylibloader_wrapper_jack_wrappers)( void*);
int initialize_jack_wrappers(int verbose);
#ifdef __cplusplus
}
#endif
#endif

// LCOV_EXCL_STOP