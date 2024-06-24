local shoop_control = shoop_control or require('shoop_control')
local shoop_coords = shoop_coords or require('shoop_coords')
local shoop_format = shoop_format or require('shoop_format')

local shoop_helpers = {}

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

--  @shoop_lua_fn_docstring.start
--  shoop_helpers.expand_selection(direction_key)
--  Given a direction key, expand the current selection of loops
--  by adding the loop(s) in the given direction.
--  @shoop_lua_fn_docstring.end
function shoop_helpers.expand_selection(direction_key)
    local loops = shoop_control.loop_get_which_selected()
    if #loops == 0 then
        shoop_control.loop_select({0, 0}, true)
        return
    end

    local new_coords = loops
    local add_coords = {}
    for i, v in ipairs(loops) do
        table.insert(add_coords, shoop_coords.move(v, direction_key))
    end
    for i, v in ipairs(add_coords) do
        table.insert(new_coords, v)
    end
    shoop_control.loop_select(new_coords, true)
end

--  @shoop_lua_fn_docstring.start
--  shoop_helpers.shrink_selection(direction_key)
--  Given a direction key, shrink the current selection of loops
--  by removing loops "coming from" that direction.
--  @shoop_lua_fn_docstring.end
function shoop_helpers.shrink_selection(direction_key)
    local loops = shoop_control.loop_get_which_selected()
    local key = direction_key
    if #loops == 0 then
        shoop_control.loop_select({0, 0}, true)
        return
    end

    local extreme = shoop_coords.extreme(loops, key, key == shoop_control.constants.Key_Up or key == shoop_control.constants.Key_Left)
    local deselect_row = (key == shoop_control.constants.Key_Up or key == shoop_control.constants.Key_Down) and extreme or nil
    local deselect_col = (key == shoop_control.constants.Key_Left or key == shoop_control.constants.Key_Right) and extreme or nil
    for i = #loops, 1, -1 do
        if loops[i][1] == deselect_col or loops[i][2] == deselect_row then
            table.remove(loops, i)
        end
    end
    shoop_control.loop_select(loops, true)
end

--  @shoop_lua_fn_docstring.start
--  shoop_helpers.move_selection(direction_key)
--  Given a direction key, move the selection of loops to that direction
--  if none of the loops would be out of bounds.
--  @shoop_lua_fn_docstring.end
function shoop_helpers.move_selection(direction_key)
    local loops = shoop_control.loop_get_which_selected()
    if #loops == 0 then
        shoop_control.loop_select({0, 0}, true)
        return
    end

    local new_coords = {}
    for i, v in ipairs(loops) do
        new_coords[i] = shoop_coords.move(v, direction_key)
    end
    local new_count = shoop_control.loop_count(new_coords)
    if new_count == #loops then
        --  Only move the selection if the full selection is able to move within limits.
        shoop_control.loop_select(new_coords, true)
    end
end

--  @shoop_lua_fn_docstring.start
--  shoop_helpers.default_loop_action(loop_selector, dry)
--  Perform the "default loop action" on a set of loop coordinates.
--  The default loop action is designed to cycle intuitively from empty to recording/grabbing, playing and stopping.
--  If "dry" is set to true, going to playback will go to playing dry through wet instead.
--  @shoop_lua_fn_docstring.end
function shoop_helpers.default_loop_action(loops, dry)
    dry = dry or false
    if #loops == 0 then
        return
    end
    local modes = list_to_set(shoop_control.loop_get_mode(loops))
    local lengths = list_to_set(shoop_control.loop_get_length(loops))
    local next_modes_list = shoop_control.loop_get_next_mode(loops)
    local next_modes_with_nil_and_stopped = next_modes_list
    table.insert(next_modes_with_nil_and_stopped, shoop_control.constants.LoopMode_Stopped)
    table.insert(next_modes_with_nil_and_stopped, nil)
    next_modes_with_nil_and_stopped = list_to_set(next_modes_with_nil_and_stopped)
    local new_mode = nil
    local all_recording = sets_equal(modes, list_to_set({ shoop_control.constants.LoopMode_Recording }))
    local all_empty = sets_equal(lengths, list_to_set({ 0 })) and sets_equal(modes, list_to_set({ shoop_control.constants.LoopMode_Stopped }))
    local all_stopped = not sets_equal(lengths, list_to_set({ 0 })) and sets_equal(modes, list_to_set({ shoop_control.constants.LoopMode_Stopped }))
    local any_transition_planned = not sets_equal(next_modes_with_nil_and_stopped, list_to_set({ shoop_control.constants.LoopMode_Stopped, nil }))
    if any_transition_planned then
        print_debug("Default loop action: Cancel planned transitions")
        new_mode = shoop_control.constants.LoopMode_Stopped
    elseif all_recording then
        if dry then
            print_debug("Default loop action: Recording -> Playing Dry")
            new_mode = shoop_control.constants.LoopMode_PlayingDryThroughWet
        else
            print_debug("Default loop action: Recording -> Playing")
            new_mode = shoop_control.constants.LoopMode_Playing
        end
    elseif all_empty then
        print_debug("Default loop action: Empty -> Recording / Grab")
        if shoop_control.get_default_recording_action() == 'record' then
            new_mode = shoop_control.constants.LoopMode_Recording
        else
            shoop_control.loop_trigger_grab(loops)
            return
        end
    elseif all_stopped then
        if dry then
            print_debug("Default loop action: Recording -> Playing Dry")
            new_mode = shoop_control.constants.LoopMode_PlayingDryThroughWet
        else
            print_debug("Default loop action: Stopped -> Playing")
            new_mode = shoop_control.constants.LoopMode_Playing
        end
    else
        print_debug("Default loop action: Any -> Stopped")
        new_mode = shoop_control.constants.LoopMode_Stopped
    end

    shoop_control.loop_trigger(loops, new_mode)
end

--  @shoop_lua_fn_docstring.start
--  shoop_helpers.record_into_first_empty(overdub)
--  In the track(s) of all selected loop(s) (or recording loop(s) of none selected),
--  find the first empty loop and start recording into it.
--  If overdub is true, already recording loops will transition to Playing.
--  Otherwise, they will transition to Stopped.
--  @shoop_lua_fn_docstring.end
function shoop_helpers.record_into_first_empty(overdub)
    local selected = shoop_control.loop_get_which_selected()
    local recording = shoop_control.loop_get_by_mode(shoop_control.constants.LoopMode_Recording)
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
        local track_loops = shoop_control.loop_get_by_track(t)
        for _, l in pairs(track_loops) do
            if (shoop_control.loop_get_mode(l)[1] == shoop_control.constants.LoopMode_Stopped and
                shoop_control.loop_get_length(l)[1] == 0) then
                table.insert(chosen_loops, l)
                break
            end
        end
    end

    if (#chosen_loops > 0) then
        --  Stop the currently recording loops and start recording into the chosen ones.
        if (overdub)
        then shoop_control.loop_trigger(recording, shoop_control.constants.LoopMode_Playing)
        else shoop_control.loop_trigger(recording, shoop_control.constants.LoopMode_Stopped)
        end 
        shoop_control.loop_trigger(chosen_loops, shoop_control.constants.LoopMode_Recording)
    end
end

--  @shoop_lua_fn_docstring.start
--  shoop_helpers.toggle_solo()
--  Toggle the global "solo" control
--  @shoop_lua_fn_docstring.end
function shoop_helpers.toggle_solo()
    local state = shoop_control.get_solo()
    shoop_control.set_solo(not state)
end

--  @shoop_lua_fn_docstring.start
--  shoop_helpers.toggle_sync_active()
--  Toggle the global "sync active" control
--  @shoop_lua_fn_docstring.end
function shoop_helpers.toggle_sync_active()
    local state = shoop_control.get_sync_active()
    shoop_control.set_sync_active(not state)
end

--  @shoop_lua_fn_docstring.start
--  shoop_helpers.toggle_play_after_record()
--  Toggle the global "sync active" control
--  @shoop_lua_fn_docstring.end
function shoop_helpers.toggle_play_after_record()
    local state = shoop_control.get_play_after_record()
    shoop_control.set_play_after_record(not state)
end

--  @shoop_lua_fn_docstring.start
--  shoop_helpers.track_toggle_muted(index)
--  Toggle the muted state of the given track. -1 is the sync track.
--  @shoop_lua_fn_docstring.end
function shoop_helpers.track_toggle_muted(index)
    local state = shoop_control.track_get_muted(index)[1]
    shoop_control.track_set_muted(index, not state)
end

--  @shoop_lua_fn_docstring.start
--  shoop_helpers.track_toggle_input_muted(index)
--  Toggle the input muted state of the given track. -1 is the sync track.
--  @shoop_lua_fn_docstring.end
function shoop_helpers.track_toggle_input_muted(index)
    local state = shoop_control.track_get_input_muted(index)[1]
    shoop_control.track_set_input_muted(index, not state)
end

return shoop_helpers