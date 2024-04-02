local shoop_format = {}

--  @shoop_lua_fn_docstring.start
--  shoop_format.format_table(table, recursive) -> string
--  Format a table such that all elements can be inspected.
--  @shoop_lua_fn_docstring.end
function shoop_format.format_table(table, recursivez)
    local function format_table_recursively(table, recursive, indent)
        local result = ""
        for k, v in pairs(table) do
            result = result .. string.rep(" ", indent) .. k .. ": "
            if type(v) == "table" and recursive then
                result = result .. "\n" .. format_table_recursively(v, recursive, indent + 2)
            else
                result = result .. tostring(v) .. "\n"
            end
        end
        return result
    end

    return format_table_recursively(table, recursive, 0)
end

return shoop_format