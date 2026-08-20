-- # Akai APC Mini MK2 controls
--
-- Deep ShoopDaLoop integration for the **Akai APC Mini MK2**. The script automatically discovers and connects to the controller's control MIDI ports.
--
-- ## Relabeled controls
--
-- | Device control | ShoopDaLoop function |
-- | --- | --- |
-- | REC ARM | **RECORD** |
-- | MUTE | **GRAB** |
-- | DRUM | **SYNC** |
-- | Bottom-right grid pad | Sync-loop button |
-- | SEND | **DRY** |
-- | DEVICE | **SET N CYCLES** |
--
-- ## Loops
--
-- Grid pads show loop state. Pressing a pad performs that loop's default action. Hold a modifier to choose another action:
--
-- | Held control | Grid-pad action | With an additional modifier |
-- | --- | --- | --- |
-- | **DRY** | Play dry through wet when transitioning to play | **SHIFT** enters composition mode |
-- | **CLIP STOP** | Stop the loop | **SHIFT** clears the loop |
-- | **RECORD** | Record the loop | **DRY** re-records dry into wet |
-- | **GRAB** | Grab the loop | **SHIFT** toggles the default recording action between record and grab |
-- | **SELECT** | Toggle loop selection | **SHIFT** toggles loop targeting |
--
-- ## Global controls
--
-- | Control | Action |
-- | --- | --- |
-- | **SOLO** | Toggle solo while held. Add **SHIFT** to make the toggle permanent. |
-- | **SYNC** | Toggle synchronization while held. Add **SHIFT** to make the toggle permanent. |
-- | **STOP ALL CLIPS** | Stop all loops. Add **SELECT** to deselect all loops, or **SHIFT** to clear all loops. |
-- | **DEVICE** | Hold and press a grid pad to set the number of recording cycles. Grid positions count from the top left, left to right; the bottom-right pad resets to zero. Add **SHIFT** to resynchronize the controller state and LEDs. |
--
-- ## Faders and track controls
--
-- Faders act only while a mode button is held:
--
-- | Held control | Fader action |
-- | --- | --- |
-- | **VOLUME** | Set track gain. |
-- | **PAN** | Set stereo track balance. |
--
-- The master fader controls the sync track.
--
-- - **VOLUME + loop button** toggles output mute for the loop's track.
-- - **PAN + grid pad** toggles input mute for that column's track.
-- - **PAN + sync-loop button** toggles input mute for the sync track.
--
-- Unmuting always respects the global auto-mute-other-inputs control.
--
-- ## Composition mode
--
-- 1. Hold **SHIFT + DRY** throughout the composition process.
-- 2. Press a loop to choose the composition target.
-- 3. Press more loops to append them immediately. Existing composite content is retained.
-- 4. Press several loops together to insert the additional loops in parallel.

if shoop_announce_api_version then
    shoop_announce_api_version(1, 1)
end

print_debug("Init akai_apc_mini_mk2_v3.lua")

local shoop_control = require('shoop_control')
local shoop_helpers = require('shoop_helpers')
local shoop_format = require('shoop_format')
local shoop_midi = require('shoop_midi')

local COLOR_off = 0
local COLOR_red = 5
local COLOR_yellow = 13
local COLOR_green = 21
local COLOR_blue = 45
local COLOR_magenta = 53

local BEHAVIOR_solid = 6
local BEHAVIOR_pulse = 9
local BEHAVIOR_blink = 14

local BUTTON_volume = 100
local BUTTON_pan = 101
local BUTTON_send = 102
local BUTTON_device = 103
local BUTTON_clip_stop = 112
local BUTTON_solo = 113
local BUTTON_mute = 114
local BUTTON_rec_arm = 115
local BUTTON_select = 116
local BUTTON_drum = 117
local BUTTON_note = 118
local BUTTON_stop_all = 119
local BUTTON_shift = 122

local BUTTON_grab = BUTTON_mute
local BUTTON_record = BUTTON_rec_arm
local BUTTON_sync = BUTTON_drum
local BUTTON_sync_loop = 7
local BUTTON_dry = BUTTON_send
local BUTTON_n_cycles = BUTTON_device

local STATE_shift_pressed = false
local STATE_select_pressed = false
local STATE_solo_toggle_permanent = false
local STATE_record_pressed = false
local STATE_grab_pressed = false
local STATE_stop_pressed = false
local STATE_dry_pressed = false
local STATE_n_cycles_pressed = false
local STATE_sync_toggle_permanent = false
local STATE_volume_pressed = false
local STATE_pan_pressed = false
local STATE_composition_active = false
local STATE_composition_target_loop = nil
local STATE_composition_n_parallel = 0

local loop_led_cache = {}
local send_fn = nil
local device_initialized = false

local coords_key = function(coords)
    return coords[1] .. ":" .. coords[2]
end

local coords_are_supported = function(coords)
    if coords[1] == -1 then
        return coords[2] == 0
    end
    return coords[1] >= 0 and coords[1] <= 7 and coords[2] >= 0 and coords[2] <= 7
end

local note_to_loop_coords = function(note)
    if note == BUTTON_sync_loop then return {-1, 0} end
    if note < 0 or note >= 64 then return nil end
    return {note % 8, 7 - note // 8}
end

local loop_coords_to_note = function(coords)
    if not coords_are_supported(coords) then return nil end
    if coords[1] == -1 then return BUTTON_sync_loop end
    return (7 - coords[2]) * 8 + coords[1]
end

local is_ui_button = function(note)
    return (note >= 100 and note <= 107) or (note >= 112 and note <= 119)
end

local set_led = function(note, color, behavior)
    if send_fn == nil or not device_initialized then return end
    behavior = behavior or BEHAVIOR_solid

    if is_ui_button(note) then
        local velocity = color == COLOR_off and 0 or 1
        if behavior == BEHAVIOR_blink then velocity = 2 end
        send_fn({0x90, note, velocity})
    elseif note >= 0 and note < 64 then
        send_fn({0x90 + behavior, note, color})
    end
end

local normalize_sync_loop_led = function(color, behavior)
    if color == COLOR_yellow then
        if behavior == BEHAVIOR_pulse then return COLOR_green, BEHAVIOR_solid end
        return COLOR_off, BEHAVIOR_solid
    elseif color == COLOR_red then
        return COLOR_green, BEHAVIOR_blink
    end
    return color, behavior
end

local set_loop_led = function(coords, color, behavior, force)
    local note = loop_coords_to_note(coords)
    if note == nil then return end

    if coords[1] == -1 then
        color, behavior = normalize_sync_loop_led(color, behavior)
    end

    local key = coords_key(coords)
    local previous = loop_led_cache[key]
    if force or previous == nil or previous.color ~= color or previous.behavior ~= behavior then
        set_led(note, color, behavior)
        loop_led_cache[key] = {color = color, behavior = behavior}
    end
end

local loop_led_style = function(mode, length, selected, targeted)
    local color = COLOR_off
    local behavior = BEHAVIOR_solid

    if mode == shoop_control.constants.LoopMode_Recording or
       mode == shoop_control.constants.LoopMode_RecordingDryIntoWet then
        color = COLOR_red
    elseif mode == shoop_control.constants.LoopMode_Playing or
           mode == shoop_control.constants.LoopMode_PlayingDryThroughWet then
        color = COLOR_green
    elseif length > 0 then
        color = COLOR_yellow
    end

    if selected or targeted then
        behavior = BEHAVIOR_pulse
        if color == COLOR_off then
            color = targeted and COLOR_magenta or COLOR_blue
        end
    end

    return color, behavior
end

local push_loop_event = function(event, force)
    if not coords_are_supported(event.coords) then return end
    local color, behavior = loop_led_style(
        event.mode,
        event.length,
        event.selected,
        event.targeted
    )
    set_loop_led(event.coords, color, behavior, force)
end

local refresh_all_loop_leds = function(force)
    local states = {}
    local all_loops = shoop_control.loop_get_all()
    if #all_loops > 0 then
        local modes = shoop_control.loop_get_mode(all_loops)
        local lengths = shoop_control.loop_get_length(all_loops)
        for i, coords in ipairs(all_loops) do
            if coords_are_supported(coords) then
                states[coords_key(coords)] = {
                    mode = modes[i],
                    length = lengths[i]
                }
            end
        end
    end

    local selected = {}
    for _, coords in ipairs(shoop_control.loop_get_which_selected()) do
        selected[coords_key(coords)] = true
    end

    local targeted = shoop_control.loop_get_which_targeted()
    local targeted_key = targeted ~= nil and coords_key(targeted) or nil

    local refresh = function(coords)
        local key = coords_key(coords)
        local state = states[key] or {
            mode = shoop_control.constants.LoopMode_Unknown,
            length = 0
        }
        local color, behavior = loop_led_style(
            state.mode,
            state.length,
            selected[key] == true,
            key == targeted_key
        )
        set_loop_led(coords, color, behavior, force)
    end

    for x = 0, 7 do
        for y = 0, 7 do
            refresh({x, y})
        end
    end
    refresh({-1, 0})
end

local recheck_global_controls = function()
    set_led(BUTTON_solo, shoop_control.get_solo() and COLOR_green or COLOR_off)
    set_led(BUTTON_sync, shoop_control.get_sync_active() and COLOR_green or COLOR_off)
end

local send_introduction = function()
    if send_fn == nil then return end
    send_fn({0xF0, 0x47, 0x7F, 0x4F, 0x60, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0xF7})
end

local reset = function()
    if send_fn == nil then return end
    print_debug("Resetting APC Mini MK2 state")

    device_initialized = false
    send_introduction()
    device_initialized = true
    loop_led_cache = {}

    for note = 100, 107 do set_led(note, COLOR_off) end
    for note = 112, 119 do set_led(note, COLOR_off) end
    refresh_all_loop_leds(true)
    recheck_global_controls()
end

local handle_loop_pressed = function(coords)
    if STATE_composition_active then
        if STATE_composition_target_loop == nil then
            STATE_composition_target_loop = coords
        else
            STATE_composition_n_parallel = STATE_composition_n_parallel + 1
            shoop_control.loop_compose_add_to_end(
                STATE_composition_target_loop,
                coords,
                STATE_composition_n_parallel > 1
            )
        end
    elseif STATE_select_pressed then
        if STATE_shift_pressed then
            shoop_control.loop_toggle_targeted(coords)
        else
            shoop_control.loop_toggle_selected(coords)
        end
    elseif STATE_record_pressed then
        local mode = STATE_dry_pressed and
            shoop_control.constants.LoopMode_RecordingDryIntoWet or
            shoop_control.constants.LoopMode_Recording
        shoop_control.loop_trigger(coords, mode)
    elseif STATE_grab_pressed then
        if STATE_shift_pressed then
            local action = shoop_control.get_default_recording_action() == 'record' and 'grab' or 'record'
            shoop_control.set_default_recording_action(action)
        else
            shoop_control.loop_trigger_grab(coords)
        end
    elseif STATE_stop_pressed then
        if STATE_shift_pressed then
            shoop_control.loop_clear(coords)
        else
            shoop_control.loop_trigger(coords, shoop_control.constants.LoopMode_Stopped)
        end
    elseif STATE_n_cycles_pressed then
        local n = (coords[1] + coords[2] * 8 + 1) % 64
        shoop_control.set_apply_n_cycles(n)
    elseif STATE_volume_pressed then
        shoop_helpers.track_toggle_muted(coords[1])
    elseif STATE_pan_pressed then
        shoop_helpers.track_toggle_input_muted(coords[1], true)
    elseif STATE_dry_pressed and STATE_shift_pressed then
        STATE_composition_active = true
        STATE_composition_target_loop = coords
        STATE_composition_n_parallel = 0
    else
        shoop_helpers.default_loop_action(coords, STATE_dry_pressed)
    end
end

local handle_loop_released = function()
    if STATE_composition_active then
        STATE_composition_n_parallel = math.max(0, STATE_composition_n_parallel - 1)
    end
end

local handle_note_on = function(msg)
    local note = msg[2]
    local loop = note_to_loop_coords(note)

    if loop ~= nil then
        handle_loop_pressed(loop)
    elseif note == BUTTON_shift then
        STATE_shift_pressed = true
    elseif note == BUTTON_select then
        STATE_select_pressed = true
        set_led(BUTTON_select, COLOR_green)
    elseif note == BUTTON_solo then
        STATE_solo_toggle_permanent = STATE_shift_pressed
        shoop_helpers.toggle_solo()
        recheck_global_controls()
    elseif note == BUTTON_record then
        STATE_record_pressed = true
        set_led(BUTTON_record, COLOR_green)
    elseif note == BUTTON_grab then
        STATE_grab_pressed = true
        set_led(BUTTON_grab, COLOR_green)
    elseif note == BUTTON_clip_stop then
        STATE_stop_pressed = true
        set_led(BUTTON_clip_stop, COLOR_green)
    elseif note == BUTTON_dry then
        STATE_dry_pressed = true
        set_led(BUTTON_dry, COLOR_green)
    elseif note == BUTTON_n_cycles then
        if STATE_shift_pressed then
            reset()
        else
            STATE_n_cycles_pressed = true
            set_led(BUTTON_n_cycles, COLOR_green)
        end
    elseif note == BUTTON_sync then
        STATE_sync_toggle_permanent = STATE_shift_pressed
        shoop_helpers.toggle_sync_active()
        recheck_global_controls()
    elseif note == BUTTON_stop_all then
        set_led(BUTTON_stop_all, COLOR_green)
        if STATE_shift_pressed then
            shoop_control.loop_clear_all()
        elseif STATE_select_pressed then
            shoop_control.loop_select({}, true)
        else
            shoop_control.loop_trigger(
                shoop_control.loop_get_all(),
                shoop_control.constants.LoopMode_Stopped
            )
        end
    elseif note == BUTTON_volume then
        STATE_volume_pressed = true
        set_led(BUTTON_volume, COLOR_green)
    elseif note == BUTTON_pan then
        STATE_pan_pressed = true
        set_led(BUTTON_pan, COLOR_green)
    end
end

local handle_note_off = function(msg)
    local note = msg[2]
    local loop = note_to_loop_coords(note)

    if loop ~= nil then
        handle_loop_released()
    elseif note == BUTTON_shift then
        STATE_shift_pressed = false
    elseif note == BUTTON_select then
        STATE_select_pressed = false
        set_led(BUTTON_select, COLOR_off)
    elseif note == BUTTON_solo then
        if not STATE_solo_toggle_permanent then shoop_helpers.toggle_solo() end
        STATE_solo_toggle_permanent = false
        recheck_global_controls()
    elseif note == BUTTON_record then
        STATE_record_pressed = false
        set_led(BUTTON_record, COLOR_off)
    elseif note == BUTTON_grab then
        STATE_grab_pressed = false
        set_led(BUTTON_grab, COLOR_off)
    elseif note == BUTTON_clip_stop then
        STATE_stop_pressed = false
        set_led(BUTTON_clip_stop, COLOR_off)
    elseif note == BUTTON_dry then
        STATE_dry_pressed = false
        STATE_composition_active = false
        STATE_composition_target_loop = nil
        STATE_composition_n_parallel = 0
        set_led(BUTTON_dry, COLOR_off)
    elseif note == BUTTON_n_cycles then
        STATE_n_cycles_pressed = false
        set_led(BUTTON_n_cycles, COLOR_off)
    elseif note == BUTTON_sync then
        if not STATE_sync_toggle_permanent then shoop_helpers.toggle_sync_active() end
        STATE_sync_toggle_permanent = false
        recheck_global_controls()
    elseif note == BUTTON_stop_all then
        set_led(BUTTON_stop_all, COLOR_off)
    elseif note == BUTTON_volume then
        STATE_volume_pressed = false
        set_led(BUTTON_volume, COLOR_off)
    elseif note == BUTTON_pan then
        STATE_pan_pressed = false
        set_led(BUTTON_pan, COLOR_off)
    end
end

local cc_to_fader_track = function(cc)
    if cc < 48 or cc > 56 then return nil end
    if cc == 56 then return -1 end
    return cc - 48
end

local handle_cc = function(msg)
    local track = cc_to_fader_track(msg[2])
    if track == nil then return end

    local value = msg[3]
    if STATE_volume_pressed then
        shoop_control.track_set_gain_fader(track, value / 127.0)
    end
    if STATE_pan_pressed then
        shoop_control.track_set_balance(track, value / 63.5 - 1.0)
    end
end

local handle_sysex = function(msg)
    if #msg < 17 or msg[2] ~= 0x47 or msg[3] ~= 0x7F or
       msg[4] ~= 0x4F or msg[5] ~= 0x61 or msg[6] ~= 0x00 or
       msg[7] ~= 0x04 or msg[17] ~= 0xF7 then
        return
    end

    local fader_values = {}
    for i = 0, 8 do
        fader_values[i + 1] = msg[8 + i]
    end
    print_debug("APC Mini MK2 introduction response: " ..
        shoop_format.format_table(fader_values, true))
end

local on_midi_in = function(msg)
    if shoop_midi.is_kind(msg, shoop_midi.NoteOn) then
        if msg[3] == 0 then
            handle_note_off(msg)
        else
            handle_note_on(msg)
        end
    elseif shoop_midi.is_kind(msg, shoop_midi.NoteOff) then
        handle_note_off(msg)
    elseif shoop_midi.is_kind(msg, shoop_midi.ControlChange) then
        handle_cc(msg)
    elseif msg[1] == shoop_midi.SysEx then
        handle_sysex(msg)
    end
end

local on_output_port_opened = function(port)
    send_fn = port.send
    device_initialized = false
end

local on_output_port_connected = function()
    device_initialized = false
    shoop_control.register_one_shot_timer_cb(500, reset)
end

local handle_loop_event = function(event)
    if event.type == shoop_control.constants.LoopEventType_CoordsChanged then
        refresh_all_loop_leds(false)
    else
        push_loop_event(event, false)
    end
end

local DEVICE_REGEX = "(?i)(.*apc\\s*mini\\s*mk\\s*2.*control.*|.*apc\\s*mini\\s*mk\\s*2.*midi\\s*1.*|apc\\s*mini\\s*mk\\s*2)"

shoop_control.auto_open_device_specific_midi_control_output(
    DEVICE_REGEX,
    on_output_port_opened,
    on_output_port_connected,
    500
)
shoop_control.auto_open_device_specific_midi_control_input(DEVICE_REGEX, on_midi_in)
shoop_control.register_loop_event_cb(handle_loop_event)
shoop_control.register_global_event_cb(recheck_global_controls)

print_debug("akai_apc_mini_mk2_v3.lua: ready")
