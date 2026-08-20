-- # Akai APC Mini MK1 controls
--
-- Deep ShoopDaLoop integration for the **Akai APC Mini MK1**. The script automatically opens MIDI ports and connects to the controller.
--
-- ## Relabeled buttons
--
-- This guide uses capitalized ShoopDaLoop button names. These controls differ from the labels printed on the device:
--
-- | Device label | ShoopDaLoop function |
-- | --- | --- |
-- | REC ARM | **RECORD** |
-- | MUTE | **GRAB** |
-- | First blank soft key | **SYNC** |
-- | Bottom-right grid button | Sync-loop button |
-- | SEND | **DRY** |
-- | DEVICE | **SET N CYCLES** |
--
-- ## Loops
--
-- Grid buttons light up to show loop state. Pressing a grid button performs that loop's default action. Hold a modifier to choose another action:
--
-- | Held control | Grid-button action | With an additional modifier |
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
-- | **SOLO** | Click to toggle solo permanently. Hold for 250 ms to toggle it momentarily until release. |
-- | **SYNC** | Click to toggle synchronization permanently. Hold for 250 ms to toggle it momentarily until release. |
-- | **STOP ALL CLIPS** | Stop all loops. Add **SELECT** to deselect all loops, or **SHIFT** to clear all loops. |
-- | **DEVICE** | Hold and press a grid button to set the number of recording cycles. Grid positions count from the top left, left to right; the bottom-right button resets to zero. |
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
-- The rightmost fader is reserved for the sync-loop track.
--
-- - **VOLUME + loop button** toggles output mute for the loop's track.
-- - **PAN + grid button** toggles input mute for that column's track.
-- - **PAN + sync-loop button** toggles input mute for the sync track.
--
-- Unmuting always respects the global auto-mute-other-inputs control.
--
-- ## Composition mode
--
-- Composition mode creates simple composite loops directly from the controller:
--
-- 1. Hold **SHIFT + DRY** throughout the composition process.
-- 2. Press a loop to choose the composition target.
-- 3. Press more loops to append them immediately. Existing composite content is retained.
-- 4. Press several loops together to insert the additional loops in parallel.
--
-- ## Script example
--
-- This source also serves as an implementation example. Lua executes it from top to bottom when loaded; callbacks registered near the end provide its continuing behavior.
if shoop_announce_api_version then
    shoop_announce_api_version(1, 1)
end

print_debug("Init akai_apc_mini_mk1.lua")

-- Import the necessary modules from the ShoopDaLoop API
local shoop_control = require('shoop_control')
local shoop_helpers = require('shoop_helpers')
local shoop_format  = require('shoop_format')
local shoop_midi    = require('shoop_midi')

-- constants: LED colors. These correspond to the defined MIDI note
-- values in the APC Mini MIDI spec.
local LED_off = 0
local LED_green = 1
local LED_green_blink = 2
local LED_red = 3
local LED_red_blink = 4
local LED_yellow = 5
local LED_yellow_blink = 6

-- constants: special button note IDs (as labeled on the hardware)
local BUTTON_clip_stop = 82
local BUTTON_solo = 83
local BUTTON_rec_arm = 84
local BUTTON_mute = 85
local BUTTON_select = 86
local BUTTON_blank_1 = 87
local BUTTON_blank_2 = 88
local BUTTON_clip_stop_all_clips = 89
local BUTTON_shift = 98
local BUTTON_up = 64
local BUTTON_down = 65
local BUTTON_left = 66
local BUTTON_right = 67
local BUTTON_volume = 68
local BUTTON_pan = 69
local BUTTON_send = 70
local BUTTON_device = 71

-- constants: redefined button IDs for ShoopDaLoop.
local BUTTON_grab = BUTTON_mute
local BUTTON_record = BUTTON_rec_arm
local BUTTON_sync = BUTTON_blank_1
local BUTTON_dry = BUTTON_send
local BUTTON_n_cycles = BUTTON_device

-- Global-control presses become holds after this shared timeout.
local GLOBAL_CONTROL_HOLD_TIMEOUT_MS = 250

-- state variables.
-- these will be used to track the current state
-- of button presses and other things.
local STATE_shift_pressed = false
local STATE_select_pressed = false
local STATE_record_pressed = false
local STATE_grab_pressed = false
local STATE_stop_pressed = false
local STATE_dry_pressed = false
local STATE_n_cycles_pressed = false
local STATE_volume_pressed = false
local STATE_pan_pressed = false
local STATE_composition_active = false
local STATE_composition_target_loop = nil
local STATE_composition_n_parallel = 0

-- This state variable table will be used to remember which color we
-- sent most recently to each loop.
local loop_colors = {}

-- Our function for sending MIDI to the AKAI.
-- Once it gets set, we can use it to send MIDI messages.
-- It is unset initially.
local send_fn = nil

-- Convert a MIDI note to the corresponding loop location on the grid
local note_to_loop_coords = function(note)
    if note == 7 then return {-1,0} end -- Bottom-rightmost loop button is now sync loop
    if note >= 64 or note < 0 then return nil end
    local x = note % 8
    local y = 7 - note // 8
    return {x,y}
end

-- Return the message for setting the given coordinate to the given color.
-- It is returned as an array of bytes for a binary MIDI message.
local led_message = function(coords, color)
    local x = coords[1]
    local y = coords[2]

    if x == -1 then
        if y == 0 then
            if color == LED_yellow then
                color = LED_off
            end
            return {0x90, 7, color}
        else
            print_warning("led_message: invalid coords: (" .. x .. "," .. y .. ")")
            return {}
        end
    end

    if x < -1 or x > 7 or y < 0 or y > 7 then
        print_warning("led_message: invalid coords: (" .. x .. "," .. y .. ")")
        return {}
    end

    local note = (7-y)*8 + x
    print_trace("LED @ (" .. x .. "," .. y .. ") - " .. note .. " -> " .. color)
    return {0x90, note, color}
end

-- Send a MIDI message to the device to set a LED color to a
-- specific loop (identified by its note number on the grid)
local set_led_by_note = function(note, color)
    if send_fn == nil then return end
    send_fn({0x90, note, color})
end

-- Send a MIDI message to the device to set a LED color to a
-- specific loop (identified by its coordinates on the grid)
local set_led_by_coords = function(coords, color)
    if send_fn == nil then return end
    send_fn(led_message(coords, color))
end

-- Convert a midi CC index to the corresponding fader track
-- on the device.
local cc_to_fader_track = function(cc)
    if cc > 56 or cc < 48 then return nil end
    if cc == 56 then return -1 end -- sync track
    return cc - 48
end

-- Handle (a) loop(s) being pressed
local handle_loop_pressed = function(coords)
    if STATE_composition_active then
        if STATE_composition_target_loop == nil then
            STATE_composition_target_loop = coords
        else
            STATE_composition_n_parallel = STATE_composition_n_parallel + 1
            if STATE_composition_n_parallel > 1 then
                shoop_control.loop_compose_add_to_end(STATE_composition_target_loop, coords, true)
            else
                shoop_control.loop_compose_add_to_end(STATE_composition_target_loop, coords, false)
            end
        end
    elseif STATE_select_pressed then
        if STATE_shift_pressed then
            -- Shift + Select => Target
            print_debug("-> target")
            shoop_control.loop_toggle_targeted(coords)
        else
            -- Select => Select
            print_debug("-> select")
            shoop_control.loop_toggle_selected(coords)
        end
    elseif STATE_record_pressed then
        if STATE_dry_pressed then
            -- RecArm + Dry => RecordDryIntoWet
            print_debug("-> re-record dry into wet")
            shoop_control.loop_trigger(coords, shoop_control.constants.LoopMode_RecordingDryIntoWet)
        else
            -- RecArm => Record
            print_debug("-> record")
            shoop_control.loop_trigger(coords, shoop_control.constants.LoopMode_Recording)
        end
    elseif STATE_grab_pressed then
        if STATE_shift_pressed then
            -- Shift + Grab => toggle default record action
            print_debug('-> default rec action toggle')
            if shoop_control.get_default_recording_action() == 'record' then
                shoop_control.set_default_recording_action('grab')
            else
                shoop_control.set_default_recording_action('record')
            end
        else
            --  Grab => Grab
            print_debug("-> grab")
            shoop_control.loop_trigger_grab(coords)
        end
    elseif STATE_stop_pressed then
        if STATE_shift_pressed then
            -- Shift + Stop => Clear
            print_debug("-> clear")
            shoop_control.loop_clear(coords)
        else
            -- Stop => Stop
            print_debug("-> stop")
            shoop_control.loop_trigger(coords, shoop_control.constants.LoopMode_Stopped)
        end
    elseif STATE_n_cycles_pressed then
        -- N Cycles => N Cycles instead of a loop action.
        -- The loops can be pressed to give a number (1 at the top left, last loop = 0)
        print_debug("-> set n cycles")
        local n = (coords[1] + coords[2] * 8 + 1) % 64 -- last button is 0
        shoop_control.set_apply_n_cycles(n)
    elseif STATE_volume_pressed then
        -- Volume => Mute
        print_debug("-> toggle track muted")
        shoop_helpers.track_toggle_muted(coords[1])
    elseif STATE_pan_pressed then
        -- Pan => Input Mute
        print_debug("-> toggle track input muted")
        shoop_helpers.track_toggle_input_muted(coords[1], true)
    elseif STATE_dry_pressed and STATE_shift_pressed then
        -- Shift + Dry => Composition mode
        print_debug("-> enter composition mode with loop")
        STATE_composition_active = true
        STATE_composition_target_loop = coords
        STATE_composition_n_parallel = 0
    else
        print_debug("-> default loop action")
        shoop_helpers.default_loop_action(coords, STATE_dry_pressed)
    end
end

-- Handle (a) loop(s) being released
local handle_loop_released = function(coords)
    if STATE_composition_active then
        STATE_composition_n_parallel = STATE_composition_n_parallel - 1
        if STATE_composition_n_parallel < 0 then
            STATE_composition_n_parallel = 0
        end
    end
end

local loop_state_color = function(mode, length)
    if mode == shoop_control.constants.LoopMode_Playing or
       mode == shoop_control.constants.LoopMode_PlayingDryThroughWet then
        return LED_green
    elseif mode == shoop_control.constants.LoopMode_Recording or
           mode == shoop_control.constants.LoopMode_RecordingDryIntoWet then
        return LED_red
    elseif length > 0 then
        return LED_yellow
    end
    return LED_off
end

-- Based on a event that happened on a particular loop, update that loop's
-- color and send it to the device.
local push_loop_color = function(coords, event)
    if send_fn == nil then return end

    local prev_color = nil
    if loop_colors[coords[1]] == nil then
        loop_colors[coords[1]] = {}
    end
    if loop_colors[coords[1]][coords[2]] ~= nil then
        prev_color = loop_colors[coords[1]][coords[2]]
    end

    local color = loop_state_color(event.mode, event.length)
    if prev_color ~= color then
        set_led_by_coords(coords, color)
        loop_colors[coords[1]][coords[2]] = color
    end
end

local push_current_loop_color = function(coords)
    local modes = shoop_control.loop_get_mode(coords)
    local lengths = shoop_control.loop_get_length(coords)
    local color = LED_off
    if #modes == 1 and #lengths == 1 then
        color = loop_state_color(modes[1], lengths[1])
    end
    if loop_colors[coords[1]] == nil then
        loop_colors[coords[1]] = {}
    end
    loop_colors[coords[1]][coords[2]] = color
    set_led_by_coords(coords, color)
end

-- Query and resend every loop color so reset also reflects changes which
-- happened before this script started or while the controller was disconnected.
local push_all_loop_colors = function()
    for i = 0, 7 do
        for j = 0, 7 do
            push_current_loop_color({i,j})
        end
    end
    push_current_loop_color({-1, 0})
end

-- Check the state of global ShoopDaLoop controls and update the resp. buttons' colors
-- on the device.
local recheck_global_controls = function()
    print_debug("recheck global controls")
    set_led_by_note(BUTTON_solo, (shoop_control.get_solo()) and LED_green or LED_off)
    set_led_by_note(BUTTON_sync, (not shoop_control.get_sync_active()) and LED_green or LED_off)
end

local create_global_control_detector = function(get_state, set_state)
    local held_restore_state = nil
    return shoop_helpers.create_click_hold_detector(
        GLOBAL_CONTROL_HOLD_TIMEOUT_MS,
        function()
            set_state(not get_state())
            recheck_global_controls()
        end,
        function()
            held_restore_state = get_state()
            set_state(not held_restore_state)
            recheck_global_controls()
        end,
        function()
            if held_restore_state ~= nil then
                set_state(held_restore_state)
                held_restore_state = nil
            end
            recheck_global_controls()
        end
    )
end

local solo_detector = create_global_control_detector(
    shoop_control.get_solo,
    shoop_control.set_solo
)
local sync_detector = create_global_control_detector(
    shoop_control.get_sync_active,
    shoop_control.set_sync_active
)

-- This reset function ensures the button and loop colors on the device match
-- the state of ShoopDaLoop.
local reset = function()
    print_debug("reset")
    if send_fn == nil then return end
    recheck_global_controls()
    push_all_loop_colors()
end

-- Handle a NoteOn message coming from the device (a button was pressed).
local handle_noteOn = function(msg)
    local note = msg[2]
    local maybe_loop = note_to_loop_coords(note)

    if maybe_loop ~= nil then
        print_debug("loop pressed")
        handle_loop_pressed(maybe_loop)
    elseif note == BUTTON_shift then
        print_debug("shift active")
        set_led_by_note(BUTTON_shift, LED_green)
        STATE_shift_pressed = true
    elseif note == BUTTON_select then
        print_debug("select active")
        set_led_by_note(BUTTON_select, LED_green)
        STATE_select_pressed = true
    elseif note == BUTTON_solo then
        print_debug("solo pressed")
        solo_detector.press()
    elseif note == BUTTON_record then
        print_debug("record active")
        set_led_by_note(BUTTON_record, LED_green)
        STATE_record_pressed = true
    elseif note == BUTTON_grab then
        print_debug("grab active")
        set_led_by_note(BUTTON_grab, LED_green)
        STATE_grab_pressed = true
    elseif note == BUTTON_clip_stop then
        print_debug("stop active")
        set_led_by_note(BUTTON_clip_stop, LED_green)
        STATE_stop_pressed = true
    elseif note == BUTTON_dry then
        print_debug("dry active")
        set_led_by_note(BUTTON_dry, LED_green)
        STATE_dry_pressed = true
    elseif note == BUTTON_n_cycles then
        if STATE_shift_pressed then
            -- Shift + N Cycles = Reset akai (debug)
            reset()
        else
            print_debug("set n cycles active")
            set_led_by_note(BUTTON_n_cycles, LED_green)
            STATE_n_cycles_pressed = true
        end
    elseif note == BUTTON_sync then
        print_debug("sync pressed")
        sync_detector.press()
    elseif note == BUTTON_clip_stop_all_clips then
        if STATE_shift_pressed then
            -- Shift + Stop All = Clear All
            print_debug("clear all")
            shoop_control.loop_clear_all()
        elseif STATE_select_pressed then
            -- Select + Stop All = Deselect All
            print_debug("deselect all")
            shoop_control.loop_select({}, true)
        else
            -- Stop All = Stop All
            print_debug("stop all")
            shoop_control.loop_trigger(shoop_control.loop_get_all(), shoop_control.constants.LoopMode_Stopped)
        end
    elseif note == BUTTON_volume then
        print_debug("volume active")
        STATE_volume_pressed = true
        set_led_by_note(BUTTON_volume, LED_green)
    elseif note == BUTTON_pan then
        print_debug("pan active")
        STATE_pan_pressed = true
        set_led_by_note(BUTTON_pan, LED_green)
    end
end

-- Handle a NoteOff message from the device (a button was released)
local handle_noteOff = function(msg)
    local note = msg[2]
    local maybe_loop = note_to_loop_coords(note)

    if maybe_loop ~= nil then
        print_debug("loop released")
        handle_loop_released(maybe_loop)
    elseif note == BUTTON_shift then
        print_debug("shift inactive")
        set_led_by_note(BUTTON_shift, LED_off)
        STATE_shift_pressed = false
    elseif note == BUTTON_select then
        print_debug("select inactive")
        set_led_by_note(BUTTON_select, LED_off)
        STATE_select_pressed = false
    elseif note == BUTTON_solo then
        print_debug("solo released")
        solo_detector.release()
    elseif note == BUTTON_record then
        print_debug("record inactive")
        set_led_by_note(BUTTON_record, LED_off)
        STATE_record_pressed = false
    elseif note == BUTTON_grab then
        print_debug("grab inactive")
        set_led_by_note(BUTTON_grab, LED_off)
        STATE_grab_pressed = false
    elseif note == BUTTON_clip_stop then
        print_debug("stop inactive")
        set_led_by_note(BUTTON_clip_stop, LED_off)
        STATE_stop_pressed = false
    elseif note == BUTTON_dry then
        print_debug("dry inactive")
        set_led_by_note(BUTTON_dry, LED_off)
        STATE_dry_pressed = false
        STATE_composition_active = false
        STATE_composition_target_loop = nil
    elseif note == BUTTON_n_cycles then
        print_debug("set n cycles inactive")
        set_led_by_note(BUTTON_n_cycles, LED_off)
        STATE_n_cycles_pressed = false
    elseif note == BUTTON_sync then
        print_debug("sync released")
        sync_detector.release()
    elseif note == BUTTON_volume then
        print_debug("volume inactive")
        STATE_volume_pressed = false
        set_led_by_note(BUTTON_volume, LED_off)
    elseif note == BUTTON_pan then
        print_debug("pan inactive")
        STATE_pan_pressed = false
        set_led_by_note(BUTTON_pan, LED_off)
    end
end

-- Handle a CC message coming from the device (a fader was moved)
local handle_cc = function (msg)
    local cc = msg[2]
    local value = msg[3]
    local maybe_fader_track = cc_to_fader_track(cc)

    if maybe_fader_track ~= nil then
        if STATE_volume_pressed then
            print_debug("set gain fader ")
            shoop_control.track_set_gain_fader(maybe_fader_track, value / 127.0)
        end
        if STATE_pan_pressed then
            print_debug("set pan fader")
            shoop_control.track_set_balance(maybe_fader_track, (value / 63.5) - 1.0)
        end
    end
end

-- Handle a Midi message coming from the device (top-level handler)
local on_midi_in = function(msg)
    print_debug("received: " .. shoop_format.format_table(msg, true))
    if shoop_midi.is_kind(msg, shoop_midi.NoteOn) then
        handle_noteOn(msg)
    elseif shoop_midi.is_kind(msg, shoop_midi.NoteOff) then
        handle_noteOff(msg)
    elseif shoop_midi.is_kind(msg, shoop_midi.ControlChange) then
        handle_cc(msg)
    end
end

-- We will register this callback to execute when ShoopDaLoop automatically
-- opens a MIDI port which can send messages to the AKAI device.
local on_output_port_opened = function(port)
    print_debug("output port opened")
    print_debug(shoop_format.format_table(port, true))
    send_fn = port.send
end

-- We will register this callback to execute when ShoopDaLoop automatically
-- connects to the AKAI device via MIDI.
local on_output_port_connected = function(port)
    print_debug("output port connected")
    shoop_control.register_one_shot_timer_cb(1000, reset)
end

-- We will register this callback to execute when a loop generates an event
-- from ShoopDaLoop. The event contains information such as the loop mode
-- and length.
local handle_loop_event = function(event)
    push_loop_color(event.coords, event)
end

-- Register the port-related callbacks
shoop_control.auto_open_device_specific_midi_control_output(".*APC MINI MIDI.*", on_output_port_opened, on_output_port_connected, 1000)
shoop_control.auto_open_device_specific_midi_control_input(".*APC MINI MIDI.*", on_midi_in)

-- Register the loop event handler
shoop_control.register_loop_event_cb(handle_loop_event)

-- Register the re-check function to handle global events from ShoopDaLoop.
shoop_control.register_global_event_cb(recheck_global_controls)

print_debug("akai_apc_mini_mk1.lua: ready")