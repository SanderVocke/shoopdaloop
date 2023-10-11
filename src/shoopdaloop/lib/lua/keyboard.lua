print_debug("Init keyboard.lua")

declare_in_context('docstring', [[
keyboard.lua: Handle keyboard events.

This script allows controlling ShoopDaLoop through keyboard keys.

-  Arrow keys: Move the selection around. If the selection is empty, select the
               loop at the origin. If multiple loops are selected, move all of
               them as long as the group does not go out of bounds.
               Holding the Ctrl key while pressing an arrow key will expand the
               selection instead of moving it.
-  Escape key: Clear the selection.
-  Space key:  Perform the default action on the selected loop(s). The default
               action is to cycle between recording, playing and stopped modes
               respectively.
-  R key:      Set the selected loop(s) to recording mode.
-  P key:      Set the selected loop(s) to playing mode.
-  S key:      Set the selected loop(s) to stopped mode.
-  L key:      Set the selected loop(s) to playing dry through wet mode.
-  M key:      Set the selected loop(s) to recording dry into wet mode.
-  N key:      "Record next": Queue recording into the first empty loop of the
               currently selected/recording track.
-  O key:      "Overdub": Queue recording into the first empty loop of the
               currently selected/recording track while playing back currently
               recording loops.
-  T key:      Target the selected loop. If more than one loop is selected, one
               of the selected loops is arbitrarily chosen.
-  U key:      Untarget all loops.
-  W key:      Record the selected loop(s) in sync with the targeted loop(s).
-  C key:      Clear the selected loop(s).
-  0-9 keys:   Record the selected loop(s) for N cycles.

Note that for the loop-transitioning keys in the list above, whether the loop
transitions instantly or in sync with the master loop depends on the global
"synchronization active" state. This can be toggled in the UI or momentarily
toggled by holding the Ctrl button.
]])

--  Convert a list to a set.
local list_to_set = function(list)
    local set = {}
    for _, v in ipairs(list) do
        set[v] = true
    end
    return set
end

--  Compare sets.
local sets_equal = function(set1, set2)
    if next(set1) ~= next(set2) then
        return false
    else
        for k, _ in pairs(set1) do
            if not set2[k] then
                return false
            end
        end
        return true
    end
end

--  Check if a keyboard key is a direction key.
local is_direction_key = function(key)
    return key == shoop.constants.Key_Up or
           key == shoop.constants.Key_Down or
           key == shoop.constants.Key_Left or
           key == shoop.constants.Key_Right
end

--  Check if a keyboard key is a number key. If so, return the number.
--  Otherwise, return nil.
local as_number_key = function(key)
    if key >= shoop.constants.Key_0 and
       key <= shoop.constants.Key_9
    then
        return key - shoop.constants.Key_0
    end
    return nil
end

--  Take a single coordinates list and return coordinates
--  if they were moved to the direction indicated by the given
--  keyboard key.
local move_coords = function(coords, direction_key)
    local new_coords = {coords[1], coords[2]}
    if direction_key == shoop.constants.Key_Up then
        new_coords[2] = new_coords[2] - 1
    elseif direction_key == shoop.constants.Key_Down then
        new_coords[2] = new_coords[2] + 1
    elseif direction_key == shoop.constants.Key_Left then
        new_coords[1] = new_coords[1] - 1
    elseif direction_key == shoop.constants.Key_Right then
        new_coords[1] = new_coords[1] + 1
    end
    return new_coords
end

--  Handle a keypress of a direction key.
local handle_direction_key = function(key, modifiers)
    local loops = shoop.loop_get_which_selected()
    if #loops == 0 then
        shoop.loop_select({0, 0}, true)
        return
    end

    if (modifiers & shoop.constants.ControlModifier) > 0 then
        local new_coords = loops
        local add_coords = {}
        for i, v in ipairs(loops) do
            table.insert(add_coords, move_coords(v, key))
        end
        for i, v in ipairs(add_coords) do
            table.insert(new_coords, v)
        end
        shoop.loop_select(new_coords, true)
    else
        local new_coords = {}
        for i, v in ipairs(loops) do
            new_coords[i] = move_coords(v, key)
        end
        local new_count = shoop.loop_count(new_coords)
        if new_count == #loops then
            --  Only move the selection if the full selection is able to move within limits.
            shoop.loop_select(new_coords, true)
        end
    end
end

--  Handle keys designated as "default action" on currently selected loop(s).
local handle_default_loop_action = function()
    local loops = shoop.loop_get_which_selected()
    if #loops == 0 then
        return
    end
    local modes = shoop.loop_get_mode(loops)
    local new_mode = shoop.constants.LoopMode_Recording
    if sets_equal(list_to_set(modes), list_to_set({ shoop.constants.LoopMode_Recording })) then
        new_mode = shoop.constants.LoopMode_Playing
    elseif not sets_equal(list_to_set(modes), list_to_set({ shoop.constants.LoopMode_Stopped })) then
        new_mode = shoop.constants.LoopMode_Stopped
    end
    shoop.loop_transition(loops, new_mode, 0)
end

--  Handle number keypress
local handle_number_key = function(number, modifiers)
    shoop.loop_record_n(shoop.loop_get_which_selected(), number, 0)
end

local record_into_first_empty = function(overdub)
    local selected = shoop.loop_get_which_selected()
    local recording = shoop.loop_get_by_mode(shoop.constants.LoopMode_Recording)
    local selected_tracks_set = {}
    local recording_tracks_set = {}
    for _, v in ipairs(selected) do
        selected_tracks_set[v[1]] = true
    end
    for _, v in ipairs(recording) do
        recording_tracks_set[v[1]] = true
    end
    local selected_tracks = {}
    local recording_tracks = {}
    for k, _ in pairs(selected_tracks_set) do
        table.insert(selected_tracks, k)
    end
    for k, _ in pairs(recording_tracks_set) do
        table.insert(recording_tracks, k)
    end

    local chosen_tracks = {}
    if (#selected_tracks == 1) then
        chosen_tracks = selected_tracks
    else
        chosen_tracks = recording_tracks
    end

    local chosen_loops = {}
    for _, t in pairs(chosen_tracks) do
        local track_loops = shoop.loop_get_by_track(t)
        for _, l in pairs(track_loops) do
            if (shoop.loop_get_mode(l)[1] == shoop.constants.LoopMode_Stopped and
                shoop.loop_get_length(l)[1] == 0) then
                table.insert(chosen_loops, l)
                break
            end
        end
    end

    if (#chosen_loops > 0) then
        --  Stop the currently recording loops and start recording into the chosen ones.
        if (overdub)
        then shoop.loop_transition(recording, shoop.constants.LoopMode_Playing, 0)
        else shoop.loop_transition(recording, shoop.constants.LoopMode_Stopped, 0)
        end 
        shoop.loop_transition(chosen_loops, shoop.constants.LoopMode_Recording, 0)
    end
end

--  Overall keyboard event handler.
local handle_keyboard = function(event_type, key, modifiers)
    if event_type == shoop.constants.KeyEventType_Pressed then
        local as_number = as_number_key(key)
        if is_direction_key(key) then
            handle_direction_key(key, modifiers)
        elseif key == shoop.constants.Key_Space then
            handle_default_loop_action()
        elseif key == shoop.constants.Key_R then
            shoop.loop_transition(shoop.loop_get_which_selected(), shoop.constants.LoopMode_Recording, 0)
        elseif key == shoop.constants.Key_P then
            shoop.loop_transition(shoop.loop_get_which_selected(), shoop.constants.LoopMode_Playing, 0)
        elseif key == shoop.constants.Key_S then
            shoop.loop_transition(shoop.loop_get_which_selected(), shoop.constants.LoopMode_Stopped, 0)
        elseif key == shoop.constants.Key_L then
            shoop.loop_transition(shoop.loop_get_which_selected(), shoop.constants.LoopMode_PlayingDryThroughWet, 0)
        elseif key == shoop.constants.Key_M then
            shoop.loop_transition(shoop.loop_get_which_selected(), shoop.constants.LoopMode_RecordingDryIntoWet, 0)
        elseif key == shoop.constants.Key_N then
            record_into_first_empty(false)
        elseif key == shoop.constants.Key_O then
            record_into_first_empty(true)
        elseif key == shoop.constants.Key_T then
            shoop.loop_target(shoop.loop_get_which_selected())
        elseif key == shoop.constants.Key_U then
            shoop.loop_untarget_all()
        elseif key == shoop.constants.Key_W then
            shoop.loop_record_with_targeted(shoop.loop_get_which_selected())
        elseif key == shoop.constants.Key_C then
            shoop.loop_clear(shoop.loop_get_which_selected())
        elseif key == shoop.constants.Key_Escape then
            shoop.loop_select({}, true)
        elseif as_number ~= nil then
            handle_number_key(as_number, modifiers)
        end
    end
end

--  Register the keyboard event handler.
shoop.register_keyboard_event_cb(handle_keyboard)
