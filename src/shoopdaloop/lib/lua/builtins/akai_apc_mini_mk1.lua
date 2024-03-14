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

-- constants
local LED_off = 0
local LED_green = 1
local LED_green_blink = 2
local LED_red = 3
local LED_red_blink = 4
local LED_yellow = 5
local LED_yellow_blink = 6

-- Track state of the "Fader Ctrl" buttons
local fader_settings = {'gain', 'pan', 'send', 'device'}
local fader_setting = fader_settings[1]

-- Our function for sending MIDI to the AKAI
local send_fn = nil

-- Convert a MIDI note to the corresponding loop location on the grid
local note_to_loop_coords = function(note)
    if note == 88 then return {-1,0} end -- Special case for sync loop button
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
        return {0x90, 88, color}
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

-- Handle a NoteOn message
local handle_noteOn = function(msg, port)
    local note = msg.bytes[1]
    local maybe_loop = note_to_loop_coords(note)

    if maybe_loop ~= nil then
        shoop_helpers.default_loop_action({maybe_loop})
    end
end

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
    elseif shoop_midi.is_kind(msg, shoop_midi.ControlChange) then
        handle_cc(msg, port)
    end
end

-- Push our known fader control state to the device lights
local push_fader_setting = function()
    if send_fn == nil then return end
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
    push_fader_setting()
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