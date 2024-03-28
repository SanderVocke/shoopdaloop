-- keyboard.lua: Handle keyboard events.
--
-- This script allows controlling ShoopDaLoop through keyboard keys.
--
-- -  Arrow keys: Move the selection around. If the selection is empty, select the
--                loop at the origin. If multiple loops are selected, move all of
--                them as long as the group does not go out of bounds.
--                Holding the Ctrl key while pressing an arrow key will expand the
--                selection instead of moving it.
-- -  Escape key: Clear the selection.
-- -  Space key:  Perform the default action on the selected loop(s). The default
--                action is to cycle between recording, playing and stopped modes
--                respectively.
-- -  R key:      Set the selected loop(s) to recording mode.
--                If none selected, select all recording loops.
-- -  P key:      Set the selected loop(s) to playing mode.
--                If none selected, select all playing loops.
-- -  S key:      Set the selected loop(s) to stopped mode.
--                If none selected, stop all loops.
-- -  L key:      Set the selected loop(s) to playing dry through wet mode.
--                If none selected, select all "playing dry through wet" loops.
-- -  M key:      Set the selected loop(s) to recording dry into wet mode.
--                If none selected, select all "recording dry into wet" loops.
-- -  N key:      "Record next": Queue recording into the first empty loop of the
--                currently selected/recording track.
-- -  G key:      "Grab": grabs data from the running buffer to record it retro-
--                actively.
-- -  O key:      "Overdub": Queue recording into the first empty loop of the
--                currently selected/recording track while playing back currently
--                recording loops.
-- -  T key:      Target the selected loop. If more than one loop is selected, one
--                of the selected loops is arbitrarily chosen. If already the target,
--                loop is untargeted.
-- -  U key:      Untarget all loops.
-- -  W key:      Record the selected loop(s) in sync with the targeted loop(s).
-- -  C key:      Clear the selected loop(s).
-- -  0-9 keys:   Set the amount of sync loop cycles to apply future actions for.
--                0 disables this - all actions will be open-ended.
--
-- Note that for the loop-transitioning keys in the list above, whether the loop
-- transitions instantly or in sync with the sync loop depends on the global
-- "synchronization active" state. This can be toggled in the UI or momentarily
-- toggled by holding the Ctrl button.

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

    if (modifiers & shoop_control.constants.ControlModifier) > 0 then
        shoop_helpers.expand_selection(key)
    elseif (modifiers & shoop_control.constants.AltModifier) > 0 then
        shoop_helpers.shrink_selection(key)
    else
        shoop_helpers.move_selection(key)
    end
end

--  Handle keys designated as "default action" on currently selected loop(s).
local handle_default_loop_action = function()
    shoop_helpers.default_loop_action(shoop_control.loop_get_which_selected())
end

--  Handle number keypress
local handle_number_key = function(number, modifiers)
    shoop_control.set_apply_n_cycles(number)
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

--  Overall keyboard event handler.
local handle_keyboard = function(event_type, key, modifiers)
    if event_type == shoop_control.constants.KeyEventType_Pressed then
        local as_number = as_number_key(key)
        if is_direction_key(key) then
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
        elseif as_number ~= nil then
            handle_number_key(as_number, modifiers)
        end
    end
end

--  Register the keyboard event handler.
shoop_control.register_keyboard_event_cb(handle_keyboard)
