-- akai_apc_mini_mk1.lua: Deep integration for the Akai APC Mini MK1.
--
-- This script will automatically open MIDI ports and connect to the APC Mini.
--
-- Once connected:
--
-- - Grid buttons will light up to indicate the state of each loop.
-- - Faders will control the track output gains.
-- - Clicking a grid button will perform the default loop action
--   (see generic ShoopDaLoop documentation) for that loop.
-- - The sync loop is mapped to the unmarked button above "Shift".
--   Since it does not support multiple LED colors, it just becomes
--   green if playing or recording, and off otherwise.
-- - Faders control the track gain levels.
--
-- Note that for now, the loops and tracks simply map to the first 8x8 items
-- on the grid. A movable window will be added in the future.

print_debug("Init akai_apc_mini_mk1.lua")

local shoop_control = require('shoop_control')
local shoop_helpers = require('shoop_helpers')
local shoop_format = require('shoop_format')
local shoop_midi = require('shoop_midi')

-- constants: LED colors
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
local BUTTON_stop_all_clips = 89
local BUTTON_shift = 98
local BUTTON_up = 64
local BUTTON_down = 65
local BUTTON_left = 66
local BUTTON_right = 67
local BUTTON_volume = 68
local BUTTON_pan = 69
local BUTTON_send = 70
local BUTTON_device = 71

-- constants: redefined button IDs
local BUTTON_grab = BUTTON_mute
local BUTTON_sync_loop = BUTTON_blank_2
local BUTTON_sync = BUTTON_blank_1
local BUTTON_dry = BUTTON_send

-- state
local STATE_shift_pressed = false
local STATE_select_pressed = false
local STATE_solo_pressed = false
local STATE_rec_arm_pressed = false
local STATE_grab_pressed = false
local STATE_stop_pressed = false
local STATE_dry_pressed = false

-- Our function for sending MIDI to the AKAI
local send_fn = nil

-- Convert a MIDI note to the corresponding loop location on the grid
local note_to_loop_coords = function(note)
    if note == BUTTON_sync_loop then return {-1,0} end -- Special case for sync loop button
    if note >= 64 or note < 0 then return nil end
    local x = note % 8
    local y = 7 - note // 8
    return {x,y}
end

-- Return the message for setting the given coordinate to the given color.
local led_message = function(coords, color)
    local x = coords[1]
    local y = coords[2]

    if x == -1 and y == 0 then
        if color == LED_yellow then
            color = LED_off
        end
        return {0x90, BUTTON_sync_loop, color}
    end

    local note = (7-y)*8 + x
    return {0x90, note, color}
end
local set_led = function(coords, color)
    if send_fn == nil then return end
    send_fn(led_message(coords, color))
end

-- Convert a CC to the corresponding fader track
local cc_to_fader_track = function(cc)
    if cc > 56 or cc < 48 then return nil end
    if cc == 56 then return -1 end -- sync track
    return cc - 48
end

-- Handle (a) loop(s) being pressed
local handle_loops_pressed = function(coords)
    if STATE_select_pressed then
        if STATE_shift_pressed then
            -- Shift + Select => Target
            shoop_control.loop_toggle_targeted(coords)
        else
            -- Select => Select
            shoop_control.loop_toggle_selected(coords)
        end
    elseif STATE_rec_arm_pressed then
        if STATE_shift_pressed then
            -- Shift + RecArm => Set N cycles
            shoop_control.set_apply_n_cycles(coords[1] + coords[2] * 8)
        elseif STATE_dry_pressed then
            -- RecArm + Dry => RecordDryIntoWet
            shoop_control.loop_trigger(coords, shoop_control.constants.LoopMode_RecordingDryIntoWet)
        else
            -- RecArm => Record
            shoop_control.loop_trigger(coords, shoop_control.constants.LoopMode_Record)
        end
    elseif STATE_grab_pressed then
        --  Grab => Grab
        shoop_control.loop_trigger_grab(coords)
    elseif STATE_stop_pressed then
        if STATE_shift_pressed then
            -- Shift + Stop => Clear
            shoop_control.loop_clear(coords)
        else
            -- Stop => Stop
            shoop_control.loop_trigger(coords, shoop_control.constants.LoopMode_Stopped)
        end
    else
        shoop_helpers.default_loop_action({coords}, STATE_dry_pressed)
    end
end

-- Handle a NoteOn message
local handle_noteOn = function(msg, port)
    local note = msg.bytes[1]
    local maybe_loop = note_to_loop_coords(note)

    if maybe_loop ~= nil then
        shoop_helpers.default_loop_action({maybe_loop})
    elseif note == BUTTON_shift then
        STATE_shift_pressed = true
    elseif note == BUTTON_select then
        STATE_select_pressed = true
    elseif note == BUTTON_solo then
        STATE_solo_pressed = true
    elseif note == BUTTON_rec_arm then
        STATE_rec_arm_pressed = true
    elseif note == BUTTON_grab then
        STATE_grab_pressed = true
    elseif note == BUTTON_stop then
        STATE_stop_pressed = true
    end
end

-- Handle a NoteOff message
local handle_noteOff = function(msg, port) {
    local note = msg.bytes[1]

    if note == BUTTON_shift then
        STATE_shift_pressed = false
    elseif note == BUTTON_select then
        STATE_select_pressed = false
    elseif note == BUTTON_solo then
        STATE_solo_pressed = false
    elseif note == BUTTON_rec_arm then
        STATE_rec_arm_pressed = false
    elseif note == BUTTON_grab then
        STATE_grab_pressed = false
    elseif note == BUTTON_stop then
        STATE_stop_pressed = false
    end
}

-- Handle a CC message
local handle_cc = function (msg, port)
    local cc = msg.bytes[1]
    local value = msg.bytes[2]
    local maybe_fader_track = cc_to_fader_track(cc)

    if maybe_fader_track ~= nil then
        shoop_control.track_set_gain_fader(maybe_fader_track, value / 127.0)
    end
end

-- Handle a Midi message (top-level handler)
local on_midi_in = function(msg, port)
    if shoop_midi.is_kind(msg, shoop_midi.NoteOn) then
        handle_noteOn(msg, port)
    elseif shoop_midi.is_kind(msg, shoop_midi.NoteOff) then
        handle_noteOff(msg, port)
    elseif shoop_midi.is_kind(msg, shoop_midi.ControlChange) then
        handle_cc(msg, port)
    end
end

-- Track the colors we sent already.
local loop_colors = {}

-- Push a mode light color to a loop
local push_loop_color = function(coords, event)
    if send_fn == nil then return end

    local prev_color = nil
    if loop_colors[coords[1]] == nil then
        loop_colors[coords[1]] = {}
    end
    if loop_colors[coords[1]][coords[2]] ~= nil then
        prev_color = loop_colors[coords[1]][coords[2]]
    end

    local color = LED_off
    if event.mode == shoop_control.constants.LoopMode_Playing or
       event.mode == shoop_control.constants.LoopMode_PlayingDryThroughWet then
        color = LED_green
    elseif event.mode == shoop_control.constants.LoopMode_Recording or
           event.mode == shoop_control.constants.LoopMode_RecordingDryIntoWet then
        color = LED_red
    elseif event.length > 0 then
        color = LED_yellow
    end

    if prev_color ~= color then
        set_led(coords, color)
        loop_colors[coords[1]][coords[2]] = color
    end
end

local get_loop_color = function(coords)
    if loop_colors[coords[1]] ~= nil then
        if loop_colors[coords[2]] ~= nil then
            return loop_colors[coords[1]][coords[2]]
        end
    end
    return nil
end

-- Push all our known loop colors to the device lights
local push_all_loop_colors = function()
    for i = 0, 7 do
        for j = 0, 7 do
            local coords = {i,j}
            local color = get_loop_color(coords)
            if color == nil then
                color = LED_off
            end
            set_led(coords, color)
        end
    end
    -- Sync loop
    local sync_coords = {-1, 0}
    local sync_color = get_loop_color(sync_coords)
    if sync_color == nil then
        sync_color = LED_off
    end
    set_led(sync_coords, sync_color)
end

-- Push all our known state to the device lights
local push_all_state = function()
    if send_fn == nil then return end
    push_all_loop_colors()
end

-- Handle our output port being opened
local on_output_port_opened = function(_send_fn)
    send_fn = _send_fn
    push_all_state()
end

-- Handle our output port being connected
local on_output_port_connected = function()
    push_all_state()
end

-- Handle loop events
local handle_loop_event = function(coords, event)
    push_loop_color(coords, event)
end

-- Open ports
shoop_control.auto_open_device_specific_midi_control_output(".*APC MINI MIDI.*", on_output_port_opened, on_output_port_connected)
shoop_control.auto_open_device_specific_midi_control_input(".*APC MINI MIDI.*", on_midi_in)

-- Register for loop callbacks
shoop_control.register_loop_event_cb(handle_loop_event)