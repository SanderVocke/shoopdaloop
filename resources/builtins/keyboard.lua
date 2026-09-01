-- # Keyboard controls
--
-- Control ShoopDaLoop directly from the computer keyboard.
--
-- ## Key bindings
--
-- | Key | Action |
-- | --- | --- |
-- | **Arrow keys** | Move the selection. With no selection, select the loop at the origin. Hold **Ctrl** to expand the selection instead of moving it. |
-- | **Escape** | Clear the selection. |
-- | **Space** | Perform the default action on selected loops, cycling between recording, playing, and stopped. |
-- | **R** | Record selected loops. With no selection, select all recording loops. |
-- | **P** | Play selected loops. With no selection, select all playing loops. |
-- | **S** | Stop selected loops. With no selection, stop all loops. |
-- | **L** | Play selected loops dry through wet. With no selection, select all loops already playing dry through wet. |
-- | **M** | Record selected loops dry into wet. With no selection, select all loops already recording dry into wet. |
-- | **I** | Toggle input mute for tracks containing selected loops. Unmuting respects the global auto-mute-other-inputs control. |
-- | **N** | **Record next:** queue recording into the first empty loop of the selected or recording track. |
-- | **G** | **Grab:** retroactively record data from the running buffer. |
-- | **O** | **Overdub:** queue recording into the first empty loop while currently recording loops play back. |
-- | **T** | Toggle one selected loop as the target. If several loops are selected, one is chosen. |
-- | **U** | Untarget all loops. |
-- | **W** | Record selected loops in sync with targeted loops. |
-- | **C** | Clear selected loops. |
-- | **.** | Sampling mode: selected loops record or play immediately until the key is released, without synchronization. |
-- | **0–9** | Set the number of sync-loop cycles for future actions. **0** makes actions open-ended. Hold digits together to enter larger values, for example **1**, then **2**, for **12**. |
--
-- ## Synchronization
--
-- Loop-transition actions follow the global **synchronization active** state. Toggle it in the UI, or hold **Ctrl** to invert it momentarily.

if shoop_announce_api_version then
    shoop_announce_api_version(1, 5)
end

print_debug("Init keyboard.lua")

local shoop_control = require('shoop_control')
local shoop_helpers = require('shoop_helpers')

--  Check if a keyboard key is a direction key.
local is_direction_key = function(key)
    return key == shoop_control.constants.Key_Up or
           key == shoop_control.constants.Key_Down or
           key == shoop_control.constants.Key_Left or
           key == shoop_control.constants.Key_Right
end

--  Check if a keyboard key is a number key. If so, return the number.
--  Otherwise, return nil.
local as_number_key = function(key)
    if key >= shoop_control.constants.Key_0 and
       key <= shoop_control.constants.Key_9
    then
        return key - shoop_control.constants.Key_0
    end
    return nil
end

--  Handle a keypress of a direction key.
local handle_direction_key = function(key, modifiers)
    local loops = shoop_control.loop_get_which_selected()
    if #loops == 0 then
        shoop_control.loop_select({0, 0}, true)
        if #shoop_control.loop_get_which_selected() == 0 then
            --  Probably there is no track 0 yet.
            shoop_control.loop_select({-1, 0}, true)
            return
        end
        return
    end

    if (modifiers & shoop_control.constants.KeyModifier_ControlModifier) > 0 then
        shoop_helpers.expand_selection(key)
    elseif (modifiers & shoop_control.constants.KeyModifier_AltModifier) > 0 then
        shoop_helpers.shrink_selection(key)
    else
        shoop_helpers.move_selection(key)
    end
end

--  Handle keys designated as "default action" on currently selected loop(s).
local handle_default_loop_action = function()
    shoop_helpers.default_loop_action(shoop_control.loop_get_which_selected())
end

--  Handle number keypresses
--  The behavior intended is that by e.g. pressing "1", holding it,
--  then also pressing "2" will result in the n_cycles setting being
--  set to "12".
local pressed_numbers_state = {}
local update_n_cycles = function()
    local result = 0
    for idx, value in ipairs(pressed_numbers_state) do
        result = result + (10 ^ (#pressed_numbers_state - idx)) * value
    end
    result = math.floor(result)
    shoop_control.set_apply_n_cycles(result)
end
local handle_number_pressed = function(number, modifiers)
    --  Remove other instances of same number from the list,
    --  then add to the end and update
    local new_table = {}
    for _, value in ipairs(pressed_numbers_state) do
        if value ~= number then
        table.insert(new_table, value)
        end
    end
    table.insert(new_table, number)
    pressed_numbers_state = new_table
    update_n_cycles()
end
local handle_number_released = function(number, modifiers)
    --  Remove all instances of the number from the table.
    local new_table = {}
    for _, value in ipairs(pressed_numbers_state) do
        if value ~= number then
        table.insert(new_table, value)
        end
    end
    pressed_numbers_state = new_table
end

local handle_loop_action = function(mode)
    local selected = shoop_control.loop_get_which_selected()
    if (#selected > 0) then
        shoop_control.loop_trigger(selected, mode)
    elseif (mode == shoop_control.constants.LoopMode_Stopped) then
        shoop_control.loop_trigger(shoop_control.loop_get_all(), mode)
    else
        shoop_control.loop_select(shoop_control.loop_get_by_mode(mode), true)
    end
end

local toggle_selected_track_inputs = function()
    local tracks = {}
    local seen = {}
    for _, coords in ipairs(shoop_control.loop_get_which_selected()) do
        local track = coords[1]
        if not seen[track] then
            seen[track] = true
            table.insert(tracks, track)
        end
    end
    if #tracks > 0 then
        shoop_helpers.track_toggle_input_muted(tracks, true)
    end
end

--  Overall keyboard event handler.
local handle_keyboard = function(event)
    local key = event.key
    local modifiers = event.modifiers
    if event.type == shoop_control.constants.KeyEventType_Pressed then
        local as_number = as_number_key(key)
        if key == shoop_control.constants.Key_Control then
            shoop_helpers.toggle_sync_active()
        elseif is_direction_key(key) then
            handle_direction_key(key, modifiers)
        elseif key == shoop_control.constants.Key_Space then
            handle_default_loop_action()
        elseif key == shoop_control.constants.Key_R then
            handle_loop_action(shoop_control.constants.LoopMode_Recording)
        elseif key == shoop_control.constants.Key_P then
            handle_loop_action(shoop_control.constants.LoopMode_Playing)
        elseif key == shoop_control.constants.Key_S then
            handle_loop_action(shoop_control.constants.LoopMode_Stopped)
        elseif key == shoop_control.constants.Key_L then
            handle_loop_action(shoop_control.constants.LoopMode_PlayingDryThroughWet)
        elseif key == shoop_control.constants.Key_M then
            handle_loop_action(shoop_control.constants.LoopMode_RecordingDryIntoWet)
        elseif key == shoop_control.constants.Key_I then
            toggle_selected_track_inputs()
        elseif key == shoop_control.constants.Key_N then
            shoop_helpers.record_into_first_empty(false)
        elseif key == shoop_control.constants.Key_O then
            shoop_helpers.record_into_first_empty(true)
        elseif key == shoop_control.constants.Key_T then
            shoop_control.loop_toggle_targeted(shoop_control.loop_get_which_selected())
        elseif key == shoop_control.constants.Key_U then
            shoop_control.loop_untarget_all()
        elseif key == shoop_control.constants.Key_W then
            shoop_control.loop_record_with_targeted(shoop_control.loop_get_which_selected())
        elseif key == shoop_control.constants.Key_C then
            shoop_control.loop_clear(shoop_control.loop_get_which_selected())
        elseif key == shoop_control.constants.Key_G then
            shoop_control.loop_trigger_grab(shoop_control.loop_get_which_selected())
        elseif key == shoop_control.constants.Key_Escape then
            shoop_control.loop_select({}, true)
        elseif key == shoop_control.constants.Key_Period then
            shoop_helpers.start_sampler(shoop_control.loop_get_which_selected())
        elseif as_number ~= nil then
            handle_number_pressed(as_number, modifiers)
        end
    elseif event.type == shoop_control.constants.KeyEventType_Released then
        local as_number = as_number_key(key)
        if key == shoop_control.constants.Key_Control then
            shoop_helpers.toggle_sync_active()
        elseif key == shoop_control.constants.Key_Period then
            shoop_helpers.stop_sampler()
        elseif as_number ~= nil then
            handle_number_released(as_number, modifiers)
        end
    end
end

--  Register the keyboard event handler.
shoop_control.register_keyboard_event_cb(handle_keyboard)
