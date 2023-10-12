local shoop_control = shoop_control or require('shoop_control')
local shoop_coords = shoop_coords or require('shoop_coords')

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

--  Given a direction key, expand the current selection of loops
--  by adding the loop(s) in the given direction.
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


--  Given a direction key, shrink the current selection of loops
--  by removing loops "coming from" that direction.
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

--  Given a direction key, move the selection of loops to that direction
--  if none of the loops would be out of bounds.
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

--  Perform the "default loop action" on a set of loop coordinates.
--  The default action is to:
--  - if all given loops are recording, transition them all to playback
--  - if all given loops are stopped and empty, transition them all to recording
--  - if all given loops are stopped but not all empty, transition them all to playback
--  - otherwise, stop all selected loops
--  (this usually means that using the default action cycles between
--   stopped, recording and playing, with some corner cases for multiple
--   selected loops)
function shoop_helpers.default_loop_action(loops)
    if #loops == 0 then
        return
    end
    local modes = list_to_set(shoop_control.loop_get_mode(loops))
    local lengths = list_to_set(shoop_control.loop_get_length(loops))
    local new_mode = nil
    if sets_equal(modes, list_to_set({ shoop_control.constants.LoopMode_Recording })) then
        new_mode = shoop_control.constants.LoopMode_Playing
    elseif sets_equal(lengths, list_to_set({ 0 })) and sets_equal(modes, list_to_set({ shoop_control.constants.LoopMode_Stopped })) then
        new_mode = shoop_control.constants.LoopMode_Recording
    elseif not sets_equal(lengths, list_to_set({ 0 })) and sets_equal(modes, list_to_set({ shoop_control.constants.LoopMode_Stopped })) then
        new_mode = shoop_control.constants.LoopMode_Playing
    else
        new_mode = shoop_control.constants.LoopMode_Stopped
    end
    shoop_control.loop_transition(loops, new_mode, 0)
end

--  In the track(s) of all selected loop(s) (or recording loop(s) of none selected),
--  find the first empty loop and start recording into it.
--  If overdub is true, already recording loops will transition to Playing.
--  Otherwise, they will transition to Stopped.
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
        then shoop_control.loop_transition(recording, shoop_control.constants.LoopMode_Playing, 0)
        else shoop_control.loop_transition(recording, shoop_control.constants.LoopMode_Stopped, 0)
        end 
        shoop_control.loop_transition(chosen_loops, shoop_control.constants.LoopMode_Recording, 0)
    end
end

return shoop_helpers