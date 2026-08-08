-- akai_apc_mini_mk2.lua: Deep integration for the Akai APC Mini MK2.
--
-- This script automatically opens MIDI ports and connects to the APC Mini MK2.
-- It leverages RGB feedback, pulsing animations, and startup handshakes.
--
-- Features:
--   - 8x8 grid pad loop recording, playback, and triggering
--   - Side buttons: solo toggle, sync active toggle, stop all
--   - Modifier buttons: shift, select, mute, recarm, dry, n-cycles
--   - Fader support via CC: volume, pan/balance, and send/device controls
--   - Modifier states alter grid behavior (mute, input mute, n-cycles, dry)
--   - Shift+stop-all clears all loops; select+stop-all deselects all
--   - SysEx handshake on connect for device initialization
--   - LED color/behavior cache for loop state visualization

print_debug("Init akai_apc_mini_mk2.lua")

-- Import modules
local shoop_control = require('shoop_control')
local shoop_helpers = require('shoop_helpers')
local shoop_format  = require('shoop_format')
local shoop_midi    = require('shoop_midi')

-- Constants: LED Colors (MK2 Palette)
local COLOR_off     = 0
local COLOR_white   = 3
local COLOR_red     = 5
local COLOR_orange  = 9
local COLOR_yellow  = 13
local COLOR_green   = 21
local COLOR_blue    = 45
local COLOR_magenta = 53

-- Constants: LED Behaviors (MIDI Channels)
local BEHAVIOR_solid = 6 -- 100% brightness
local BEHAVIOR_pulse = 9 -- 1/4 pulse
local BEHAVIOR_blink = 14 -- 1/4 blink

-- Constants: UI Button Notes
local BUTTON_track_1 = 100
local BUTTON_track_8 = 107
local BUTTON_scene_1 = 112
local BUTTON_scene_8 = 119
local BUTTON_shift   = 122

-- Constants: Redefined Button IDs (Legacy Mapping)
local BUTTON_clip_stop = 112
local BUTTON_solo      = 113
local BUTTON_mute      = 114
local BUTTON_recarm    = 115
local BUTTON_select    = 116
local BUTTON_drum      = 117
local BUTTON_note      = 118
local BUTTON_stop_all  = 119

local BUTTON_up     = 100 -- Unused but for reference
local BUTTON_down   = 101
local BUTTON_left   = 102
local BUTTON_right  = 103
local BUTTON_volume = 104
local BUTTON_pan    = 105
local BUTTON_send   = 106 -- "DRY"
local BUTTON_device = 107 -- "N CYCLES"

local BUTTON_dry      = BUTTON_send
local BUTTON_n_cycles = BUTTON_device

-- State Variables
local STATE_shift_pressed = false
local STATE_select_pressed = false
local STATE_mute_pressed = false
local STATE_recarm_pressed = false
local STATE_stop_pressed = false
local STATE_dry_pressed = false
local STATE_n_cycles_pressed = false
local STATE_drum_pressed = false
local STATE_volume_pressed = false
local STATE_pan_pressed = false

local STATE_composition_active = false
local STATE_composition_target_loop = nil
local STATE_composition_n_parallel = 0

-- LED Cache
local loop_colors = {}
local loop_behaviors = {}

-- MIDI Send Function
local send_fn = nil

-- SysEx Handshake: Introduction Message
local send_introduction = function()
    if send_fn == nil then return end
    print_debug("Sending APC Mini MK2 Introduction SysEx")
    -- Format: F0 47 7F 4F 60 00 04 00 <Ver High> <Ver Low> <Bugfix> F7
    send_fn({0xF0, 0x47, 0x7F, 0x4F, 0x60, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0xF7})
end

-- Convert note to grid coordinates (0,0 is Top-Left)
local note_to_loop_coords = function(note)
    if note == 7 then return {-1, 0} end -- Bottom-rightmost loop button is now sync loop
    if note >= 64 or note < 0 then return nil end
    local x = note % 8
    local y = 7 - note // 8
    return {x, y}
end

-- Helper to set specific LED
local set_led = function(note, color, behavior)
    if send_fn == nil then return end
    behavior = behavior or BEHAVIOR_solid
    
    -- UI Buttons use single-color mode, which doesn't support the channel-behavior scheme
    -- They use velocity 0=off, 1=on, 2=blink.
    if note >= 100 then
        local velocity = (color > 0) and 1 or 0
        if behavior == BEHAVIOR_blink then velocity = 2 end
        send_fn({0x90, note, velocity})
    else
        -- Grid pads use MIDI channels for behaviors (0x90 + channel 0..15)
        send_fn({0x90 + behavior, note, color})
    end
end

-- Helper to check if a loop is selected
local is_selected = function(coords)
    local selected = shoop_control.loop_get_which_selected()
    for _, s in ipairs(selected) do
        if s[1] == coords[1] and s[2] == coords[2] then
            return true
        end
    end
    return false
end

-- Helper to check if a loop is targeted
local is_targeted = function(coords)
    local targeted = shoop_control.loop_get_which_targeted()
    if targeted == nil then return false end
    return targeted[1] == coords[1] and targeted[2] == coords[2]
end

-- Update Loop LED
local push_loop_color = function(coords, event)
    if send_fn == nil then return end

    local color = COLOR_off
    local behavior = BEHAVIOR_solid

    if event.mode == shoop_control.constants.LoopMode_Recording or 
       event.mode == shoop_control.constants.LoopMode_RecordingDryIntoWet then
        color = COLOR_red
        behavior = BEHAVIOR_solid
    elseif event.mode == shoop_control.constants.LoopMode_Playing or
           event.mode == shoop_control.constants.LoopMode_PlayingDryThroughWet then
        color = COLOR_green
        behavior = BEHAVIOR_solid
    elseif event.length > 0 then
        color = COLOR_yellow
        behavior = BEHAVIOR_solid
    end

    -- Add pulsing if the loop is selected or targeted (user-friendliness boost)
    if is_selected(coords) or is_targeted(coords) then
        color = COLOR_blue
        behavior = BEHAVIOR_pulse
    end

    local note = (7 - coords[2]) * 8 + coords[1]
    if coords[1] == -1 then note = 7 end
    
    set_led(note, color, behavior)
end

-- Refresh All LEDs
local recheck_global_controls = function()
    if send_fn == nil then return end
    set_led(BUTTON_solo, (shoop_control.get_solo()) and COLOR_green or COLOR_off)
    set_led(BUTTON_drum, (not shoop_control.get_sync_active()) and COLOR_green or COLOR_off)
end

local reset = function()
    if send_fn == nil then return end
    print_debug("Resetting APC Mini MK2 State")
    
    -- Introduction to get fader states and enter remote mode
    send_introduction()
    
    -- Clear grid
    for n = 0, 63 do set_led(n, COLOR_off) end
    -- Clear side buttons
    for n = 100, 122 do set_led(n, COLOR_off) end
    
    -- Sync UI buttons to global state
    recheck_global_controls()
end

-- MIDI CC Handler (Faders)
local handle_cc = function(msg)
    local cc = msg[2]
    local val = msg[3]
    local track = (cc == 56) and -1 or (cc - 48)
    
    if track >= -1 and track < 8 then
        if STATE_volume_pressed then
            shoop_control.track_set_gain_fader(track, val / 127.0)
        elseif STATE_pan_pressed then
            shoop_control.track_set_balance(track, (val / 63.5) - 1.0)
        else
            -- Default to gain if no modifier is held
            shoop_control.track_set_gain_fader(track, val / 127.0)
        end
    end
end

-- MIDI Note Handler
local handle_noteOn = function(msg)
    local note = msg[2]
    local loop = note_to_loop_coords(note)

    if loop ~= nil then
        if STATE_n_cycles_pressed then
            local n = (loop[1] + loop[2] * 8 + 1) % 64
            shoop_control.set_apply_n_cycles(n)
        elseif STATE_volume_pressed then
            shoop_helpers.track_toggle_muted(loop)
        elseif STATE_pan_pressed then
            shoop_helpers.track_toggle_input_muted(loop)
        else
            shoop_helpers.default_loop_action(loop, STATE_dry_pressed)
        end
    elseif note == BUTTON_shift then
        STATE_shift_pressed = true
    elseif note == BUTTON_select then
        STATE_select_pressed = true
    elseif note == BUTTON_mute then
        STATE_mute_pressed = true
    elseif note == BUTTON_recarm then
        STATE_recarm_pressed = true
    elseif note == BUTTON_clip_stop then
        STATE_stop_pressed = true
    elseif note == BUTTON_dry then
        STATE_dry_pressed = true
    elseif note == BUTTON_n_cycles then
        STATE_n_cycles_pressed = true
    elseif note == BUTTON_volume then
        STATE_volume_pressed = true
    elseif note == BUTTON_pan then
        STATE_pan_pressed = true
    elseif note == BUTTON_solo then
        shoop_helpers.toggle_solo()
        set_led(BUTTON_solo, (shoop_control.get_solo()) and COLOR_green or COLOR_off)
    elseif note == BUTTON_drum then
        shoop_helpers.toggle_sync_active()
        set_led(BUTTON_drum, (not shoop_control.get_sync_active()) and COLOR_green or COLOR_off)
    elseif note == BUTTON_stop_all then
        if STATE_shift_pressed then
            shoop_control.loop_clear_all()
        elseif STATE_select_pressed then
            shoop_control.loop_select({}, true)
        else
            shoop_control.loop_trigger(shoop_control.loop_get_all(), shoop_control.constants.LoopMode_Stopped)
        end
    end
end

-- MIDI Input Switcher
local on_midi_in = function(msg)
    if shoop_midi.is_kind(msg, shoop_midi.NoteOn) then
        handle_noteOn(msg)
    elseif shoop_midi.is_kind(msg, shoop_midi.NoteOff) then
        local note = msg[2]
        if note == BUTTON_shift then STATE_shift_pressed = false
        elseif note == BUTTON_select then STATE_select_pressed = false
        elseif note == BUTTON_mute then STATE_mute_pressed = false
        elseif note == BUTTON_recarm then STATE_recarm_pressed = false
        elseif note == BUTTON_clip_stop then STATE_stop_pressed = false
        elseif note == BUTTON_dry then STATE_dry_pressed = false
        elseif note == BUTTON_n_cycles then STATE_n_cycles_pressed = false
        elseif note == BUTTON_volume then STATE_volume_pressed = false
        elseif note == BUTTON_pan then STATE_pan_pressed = false
        end
    elseif shoop_midi.is_kind(msg, shoop_midi.ControlChange) then
        handle_cc(msg)
    elseif shoop_midi.is_kind(msg, shoop_midi.SysEx) then
        -- Handle Introduction Response: F0 47 7F 4F 60 ... fader values ... F7
        if msg[2] == 0x47 and msg[5] == 0x60 then
            print_debug("Received Handshake Response - Ignoring initial fader values per user request")
            -- We explicitly ignore initialization based on fader positions for now.
            -- for i = 0, 8 do
            --     local val = msg[10 + i]
            --     if val then
            --         local track = (i == 8) and -1 or i
            --         shoop_control.track_set_gain_fader(track, val / 127.0)
            --     end
            -- end
        end
    end
end

-- Callback Setup
local on_output_port_opened = function(port)
    send_fn = port.send
end

local on_output_port_connected = function(port)
    reset()
end

local handle_loop_event = function(event)
    push_loop_color(event.coords, event)
end

-- Hardware Registration
shoop_control.auto_open_device_specific_midi_control_output(".*APC mini mk2 Control.*", on_output_port_opened, on_output_port_connected, 1000)
shoop_control.auto_open_device_specific_midi_control_input(".*APC mini mk2 Control.*", on_midi_in)
shoop_control.register_loop_event_cb(handle_loop_event)
shoop_control.register_global_event_cb(recheck_global_controls)

print_debug("akai_apc_mini_mk2.lua: ready")
