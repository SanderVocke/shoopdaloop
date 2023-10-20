print_debug("Init akai_apc_mini_mk1.lua")

declare_in_context('docstring', [[
akai_apc_mini_mk1.lua: Deep integration for the Akai APC Mini MK1.
]])

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

-- Track whether we sent our complete state to the device yet
local pushed_all = nil

-- Track state of the "Fader Ctrl" buttons
local fader_settings = {'volume', 'pan', 'send', 'device'}
local fader_setting = fader_settings[1]

-- Our function for sending MIDI to the AKAI
local send_fn = nil

-- Convert a MIDI note to the corresponding loop location on the grid
local note_to_loop_coords = function(note)
    if note >= 64 or note < 0 then return nil end
    local x = note % 8
    local y = 7 - note // 8
    return {x,y}
end

-- Return the message for setting the given coordinate to the given color.
local led_message = function(coords, color)
    local x = coords[1]
    local y = coords[2]
    local note = (7-y)*8 + x
    return {0x90, note, color}
end
local set_led = function(coords, color)
    if send_fn == nil then return end
    send_fn(led_message(coords, color))
end

-- Convert a CC to the corresponding fader track
local cc_to_fader_track = function(cc)
    if cc >= 64 or cc < 48 then return nil end
    return cc - 48
end

-- Handle a NoteOn message
local handle_noteOn = function(msg, port)
    local note = msg.bytes[1]
    local maybe_loop = note_to_loop_coords(note)

    if maybe_loop ~= nil then
        shoop_helpers.default_loop_action({maybe_loop})
        set_led(maybe_loop, LED_green)
    end
end

-- Handle a CC message
local handle_cc = function (msg, port)
    local cc = msg.bytes[1]
    local value = msg.bytes[2]
    local maybe_fader_track = cc_to_fader_track(cc)

    if maybe_fader_track ~= nil then
        shoop_control.track_set_volume_fader(maybe_fader_track, value / 127.0)
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

-- Push all our known state to the device lights
local push_all_state = function()
    if send_fn == nil then return end
    push_fader_setting()
    pushed_all = true
end

-- Handle our output port being opened
local on_output_port_opened = function(_send_fn)
    send_fn = _send_fn
    if not pushed_all then push_all_state() end
end

-- Handle our output port being connected
local on_output_port_connected = function()
    if not pushed_all then push_all_state() end
end

shoop_control.auto_open_device_specific_midi_control_output(".*APC MINI MIDI.*", on_output_port_opened, on_output_port_connected)
shoop_control.auto_open_device_specific_midi_control_input(".*APC MINI MIDI.*", on_midi_in)