local shoop_control = shoop_control or require('shoop_control')
local shoop_coords = {}

--  @shoop_lua_fn_docstring.start
--  shoop_coords.move(coords, direction_key) -> coords
--  Take a single coordinates list and return coordinates
--  if they were moved to the direction indicated by the given
--  keyboard key.
--  @shoop_lua_fn_docstring.end
function shoop_coords.move(coords, direction_key)
    local new_coords = {coords[1], coords[2]}
    if direction_key == shoop_control.constants.Key_Up then
        new_coords[2] = new_coords[2] - 1
    elseif direction_key == shoop_control.constants.Key_Down then
        new_coords[2] = new_coords[2] + 1
    elseif direction_key == shoop_control.constants.Key_Left then
        new_coords[1] = new_coords[1] - 1
    elseif direction_key == shoop_control.constants.Key_Right then
        new_coords[1] = new_coords[1] + 1
    end
    return new_coords
end

--  @shoop_lua_fn_docstring.start
--  shoop_coords.extreme(all_coords, direction_key, highest) -> coord
--  Look for the highest (if highest == true) or lowest index (row/col) in the given direction
--  @shoop_lua_fn_docstring.end
function shoop_coords.extreme(all_coords, direction_key, highest)
    local extreme = nil
    for _, v in ipairs(all_coords) do
        local coord = v[1]
        if direction_key == shoop_control.constants.Key_Up or
           direction_key == shoop_control.constants.Key_Down
        then
            coord = v[2]
        end
        if extreme == nil or
           (highest and coord > extreme) or
           (not highest and coord < extreme)
        then
            extreme = coord
        end
    end
    return extreme
end

return shoop_coords